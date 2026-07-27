use std::{
    collections::HashMap,
    io::{self, Write},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use dioxus::server::axum::{
    self,
    extract::{DefaultBodyLimit, Form, Request, State},
    http::{
        header::{AUTHORIZATION, COOKIE, HOST, ORIGIN, SET_COOKIE},
        HeaderMap, HeaderValue, Method, StatusCode,
    },
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use rand_core::OsRng;
use serde::Deserialize;
use subtle::ConstantTimeEq;

const SECURE_COOKIE_NAME: &str = "__Host-syntaxis-session";
const INSECURE_COOKIE_NAME: &str = "syntaxis-session";
const SESSION_LIFETIME: Duration = Duration::from_hours(720);
const LOGIN_WINDOW: Duration = Duration::from_mins(5);
const MAX_LOGIN_FAILURES: u8 = 5;

#[derive(Clone)]
struct AuthState {
    inner: Arc<AuthStateInner>,
}

struct AuthStateInner {
    disabled: bool,
    password_hash: String,
    api_token: Option<String>,
    sessions: Mutex<HashMap<String, Instant>>,
    login_failures: Mutex<HashMap<String, LoginFailures>>,
    secure_cookie: bool,
}

#[derive(Clone, Copy)]
struct LoginFailures {
    started_at: Instant,
    failures: u8,
}

#[derive(Deserialize)]
struct LoginForm {
    password: String,
}

pub(crate) fn serve() -> ! {
    let state = AuthState::from_environment()
        .unwrap_or_else(|message| panic!("Syntaxis authentication configuration error: {message}"));

    dioxus::serve(move || {
        let state = state.clone();
        async move {
            let auth_layer =
                axum::middleware::from_fn_with_state(state.clone(), require_authentication);
            let login_page_state = state.clone();
            let login_state = state.clone();
            let logout_state = state.clone();
            let router = Router::new()
                .route("/health", get(health))
                .route(
                    "/login",
                    get(move || login_page(login_page_state.clone()))
                        .post(move |headers, form| login(login_state.clone(), headers, form))
                        .layer(DefaultBodyLimit::max(4 * 1024)),
                )
                .route(
                    "/auth/logout",
                    post(move |headers| logout(logout_state.clone(), headers)),
                )
                .merge(dioxus::server::router(crate::app::App))
                .layer(auth_layer);
            Ok(router)
        }
    })
}

impl AuthState {
    fn from_environment() -> Result<Self, String> {
        let disabled = cfg!(debug_assertions)
            && std::env::var("SYNTAXIS_AUTH_DISABLED").is_ok_and(|value| value == "true");
        if disabled {
            return Ok(Self {
                inner: Arc::new(AuthStateInner {
                    disabled: true,
                    password_hash: String::new(),
                    api_token: None,
                    sessions: Mutex::new(HashMap::new()),
                    login_failures: Mutex::new(HashMap::new()),
                    secure_cookie: false,
                }),
            });
        }

        let password_hash = std::env::var("SYNTAXIS_PASSWORD_HASH")
            .map_err(|_| "SYNTAXIS_PASSWORD_HASH is not set".to_owned())?;
        PasswordHash::new(&password_hash)
            .map_err(|error| format!("SYNTAXIS_PASSWORD_HASH is not a valid PHC hash: {error}"))?;

        let api_token = std::env::var("SYNTAXIS_API_TOKEN")
            .ok()
            .filter(|token| !token.is_empty());
        if api_token.as_ref().is_some_and(|token| token.len() < 32) {
            return Err("SYNTAXIS_API_TOKEN must contain at least 32 characters".to_owned());
        }

        let secure_cookie =
            std::env::var("SYNTAXIS_INSECURE_COOKIE").map_or(true, |value| value != "true");

        Ok(Self {
            inner: Arc::new(AuthStateInner {
                disabled: false,
                password_hash,
                api_token,
                sessions: Mutex::new(HashMap::new()),
                login_failures: Mutex::new(HashMap::new()),
                secure_cookie,
            }),
        })
    }

    fn bearer_is_valid(&self, headers: &HeaderMap) -> bool {
        let Some(expected) = self.inner.api_token.as_deref() else {
            return false;
        };
        let Some(provided) = headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
        else {
            return false;
        };
        provided.len() == expected.len()
            && bool::from(provided.as_bytes().ct_eq(expected.as_bytes()))
    }

    fn session_is_valid(&self, headers: &HeaderMap) -> bool {
        let Some(token) = cookie_value(headers, self.cookie_name()) else {
            return false;
        };
        let now = Instant::now();
        let mut sessions = self
            .inner
            .sessions
            .lock()
            .expect("authentication session mutex poisoned");
        sessions.retain(|_, expires_at| *expires_at > now);
        sessions
            .get(token)
            .is_some_and(|expires_at| *expires_at > now)
    }

    fn create_session(&self) -> String {
        let token = URL_SAFE_NO_PAD.encode(rand_bytes());
        self.inner
            .sessions
            .lock()
            .expect("authentication session mutex poisoned")
            .insert(token.clone(), Instant::now() + SESSION_LIFETIME);
        token
    }

    fn remove_session(&self, headers: &HeaderMap) {
        if let Some(token) = cookie_value(headers, self.cookie_name()) {
            self.inner
                .sessions
                .lock()
                .expect("authentication session mutex poisoned")
                .remove(token);
        }
    }

    fn session_cookie(&self, token: &str) -> HeaderValue {
        let cookie_name = self.cookie_name();
        let secure = if self.inner.secure_cookie {
            "; Secure"
        } else {
            ""
        };
        HeaderValue::from_str(&format!(
            "{cookie_name}={token}; Path=/; HttpOnly{secure}; SameSite=Strict; Max-Age={}",
            SESSION_LIFETIME.as_secs()
        ))
        .expect("generated session cookie must be a valid header")
    }

    fn clear_cookie(&self) -> HeaderValue {
        let cookie_name = self.cookie_name();
        let secure = if self.inner.secure_cookie {
            "; Secure"
        } else {
            ""
        };
        HeaderValue::from_str(&format!(
            "{cookie_name}=; Path=/; HttpOnly{secure}; SameSite=Strict; Max-Age=0"
        ))
        .expect("generated clear cookie must be a valid header")
    }

    fn cookie_name(&self) -> &'static str {
        if self.inner.secure_cookie {
            SECURE_COOKIE_NAME
        } else {
            INSECURE_COOKIE_NAME
        }
    }

    fn login_is_limited(&self, client: &str) -> bool {
        let now = Instant::now();
        let mut failures = self
            .inner
            .login_failures
            .lock()
            .expect("login failure mutex poisoned");
        failures.retain(|_, entry| now.duration_since(entry.started_at) < LOGIN_WINDOW);
        failures
            .get(client)
            .is_some_and(|entry| entry.failures >= MAX_LOGIN_FAILURES)
    }

    fn record_login_failure(&self, client: String) {
        let now = Instant::now();
        let mut failures = self
            .inner
            .login_failures
            .lock()
            .expect("login failure mutex poisoned");
        let entry = failures.entry(client).or_insert(LoginFailures {
            started_at: now,
            failures: 0,
        });
        if now.duration_since(entry.started_at) >= LOGIN_WINDOW {
            *entry = LoginFailures {
                started_at: now,
                failures: 1,
            };
        } else {
            entry.failures = entry.failures.saturating_add(1);
        }
    }

    fn clear_login_failures(&self, client: &str) {
        self.inner
            .login_failures
            .lock()
            .expect("login failure mutex poisoned")
            .remove(client);
    }
}

async fn require_authentication(
    State(state): State<AuthState>,
    request: Request,
    next: Next,
) -> Response {
    if state.inner.disabled {
        return next.run(request).await;
    }

    let path = request.uri().path();
    if matches!(path, "/health" | "/login") || path.starts_with("/assets/") {
        return next.run(request).await;
    }

    let bearer_authenticated = state.bearer_is_valid(request.headers());
    let session_authenticated = state.session_is_valid(request.headers());
    if !bearer_authenticated && !session_authenticated {
        return unauthorized(path);
    }
    if session_authenticated && !origin_is_allowed(&request) {
        return (StatusCode::FORBIDDEN, "Cross-origin request rejected").into_response();
    }

    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

async fn health() -> impl IntoResponse {
    StatusCode::NO_CONTENT
}

async fn login_page(state: AuthState) -> Response {
    if state.inner.disabled {
        return Redirect::to("/").into_response();
    }
    no_store(Html(login_html(None)))
}

async fn login(state: AuthState, headers: HeaderMap, Form(form): Form<LoginForm>) -> Response {
    if state.inner.disabled {
        return Redirect::to("/").into_response();
    }

    let client = client_identifier(&headers);
    if state.login_is_limited(&client) {
        let mut response = no_store(Html(login_html(Some(
            "Too many attempts. Try again in a few minutes.",
        ))));
        *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
        response
            .headers_mut()
            .insert("retry-after", HeaderValue::from_static("300"));
        return response;
    }

    let hash = state.inner.password_hash.clone();
    let password = form.password;
    let verified = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash).is_ok_and(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
    })
    .await
    .unwrap_or(false);

    if !verified {
        state.record_login_failure(client);
        let mut response = no_store(Html(login_html(Some("Incorrect password."))));
        *response.status_mut() = StatusCode::UNAUTHORIZED;
        return response;
    }

    state.clear_login_failures(&client);
    let token = state.create_session();
    let mut response = Redirect::to("/").into_response();
    response
        .headers_mut()
        .insert(SET_COOKIE, state.session_cookie(&token));
    response
}

async fn logout(state: AuthState, headers: HeaderMap) -> Response {
    if state.inner.disabled {
        return Redirect::to("/").into_response();
    }

    state.remove_session(&headers);
    let mut response = Redirect::to("/login").into_response();
    response
        .headers_mut()
        .insert(SET_COOKIE, state.clear_cookie());
    response
}

fn unauthorized(path: &str) -> Response {
    if path.starts_with("/api/") {
        (
            StatusCode::UNAUTHORIZED,
            [("www-authenticate", "Bearer realm=\"Syntaxis\"")],
            "Authentication required",
        )
            .into_response()
    } else {
        Redirect::to("/login").into_response()
    }
}

fn origin_is_allowed(request: &Request) -> bool {
    if matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) && request.headers().get("upgrade").is_none()
    {
        return true;
    }
    let Some(origin) = request.headers().get(ORIGIN) else {
        return true;
    };
    let Some(host) = request.headers().get(HOST) else {
        return false;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Ok(host) = host.to_str() else {
        return false;
    };
    url::Url::parse(origin)
        .ok()
        .and_then(|url| url.host_str().map(|name| (name.to_owned(), url.port())))
        .is_some_and(|(name, port)| {
            let origin_authority = port.map_or(name.clone(), |port| format!("{name}:{port}"));
            origin_authority.eq_ignore_ascii_case(host)
        })
}

fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|cookie| cookie.trim().split_once('='))
        .find_map(|(cookie_name, value)| (cookie_name == name).then_some(value))
}

fn client_identifier(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("direct")
        .to_owned()
}

fn no_store(content: impl IntoResponse) -> Response {
    let mut response = content.into_response();
    let headers = response.headers_mut();
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    headers.insert(
        "content-security-policy",
        HeaderValue::from_static(
            "default-src 'none'; style-src 'unsafe-inline'; font-src 'self'; img-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'",
        ),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn rand_bytes() -> [u8; 32] {
    use rand_core::RngCore;

    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

pub(crate) fn print_password_hash() -> Result<(), String> {
    let password = rpassword::prompt_password("Password: ").map_err(|error| error.to_string())?;
    if password.is_empty() {
        return Err("password must not be empty".to_owned());
    }

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|error| error.to_string())?;
    writeln!(io::stdout().lock(), "{hash}").map_err(|error| error.to_string())
}

#[allow(
    clippy::too_many_lines,
    reason = "the self-contained login document keeps unauthenticated assets minimal"
)]
fn login_html(error: Option<&str>) -> String {
    let login_font = crate::app::GEIST_FONT;
    let login_favicon = crate::app::FAVICON;
    let error = error.map_or_else(String::new, |message| {
        format!("<p class=\"error\" role=\"alert\">{message}</p>")
    });
    let html = format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="robots" content="noindex,nofollow">
<title>Sign in · Syntaxis</title>
<link rel="icon" href="{login_favicon}">
<style>
:root {{
  color-scheme:dark;
  --background:oklch(0.205 0.003 247.9);
  --foreground:oklch(0.842 0.004 247.9);
  --card:oklch(24.29% 0.0024 247.93);
  --primary:oklch(0.63 0.123 236.5);
  --primary-foreground:oklch(1 0 0);
  --muted-foreground:oklch(0.626 0.006 247.9);
  --input:oklch(1 0 0 / 13%);
  --ring:oklch(0.612 0.118 236.5);
  --destructive:oklch(0.687 0.174 25.7);
}}
@font-face {{
  font-family:"Geist Variable";
  src:url("{login_font}") format("woff2");
  font-style:normal;
  font-weight:100 900;
  font-display:swap;
}}
* {{ box-sizing: border-box; }}
body {{
  margin:0;
  min-height:100dvh;
  display:grid;
  place-items:center;
  padding:max(1.5rem,env(safe-area-inset-top)) max(1.25rem,env(safe-area-inset-right)) max(1.5rem,env(safe-area-inset-bottom)) max(1.25rem,env(safe-area-inset-left));
  background:var(--background);
  color:var(--foreground);
  font:14px/1.45 "Geist Variable",ui-sans-serif,system-ui,sans-serif;
  font-synthesis:none;
  -webkit-font-smoothing:antialiased;
}}
main {{ width:min(100%,23rem); }}
.eyebrow {{
  margin:0 0 .45rem;
  color:var(--primary);
  font-size:.625rem;
  font-weight:750;
  letter-spacing:.14em;
}}
h1 {{ margin:0; font-size:1.75rem; font-weight:600; letter-spacing:-.035em; line-height:1.15; }}
.description {{ margin:.45rem 0 0; color:var(--muted-foreground); font-size:.875rem; }}
form {{ display:grid; gap:.9rem; margin-top:1.75rem; }}
label {{ display:grid; gap:.42rem; font-size:.75rem; font-weight:650; }}
input,button {{ width:100%; height:2.65rem; border-radius:.45rem; font:inherit; }}
input {{
  border:1px solid var(--input);
  background:var(--card);
  color:var(--foreground);
  padding:0 .75rem;
}}
input:focus {{ border-color:var(--ring); outline:2px solid color-mix(in oklch,var(--ring),transparent 68%); outline-offset:1px; }}
button {{
  border:0;
  background:var(--primary);
  color:var(--primary-foreground);
  font-size:.8rem;
  font-weight:700;
  cursor:pointer;
  transition:filter 120ms ease;
}}
button:hover {{ filter:brightness(1.08); }}
button:focus-visible {{ outline:2px solid var(--ring); outline-offset:2px; }}
.error {{
  margin:0;
  border:1px solid color-mix(in oklch,var(--destructive),transparent 65%);
  border-radius:.45rem;
  background:color-mix(in oklch,var(--destructive),transparent 92%);
  color:var(--destructive);
  padding:.6rem .7rem;
  font-size:.75rem;
}}
@media (max-width:420px) {{
  body {{ place-items:start center; padding-top:max(12vh,env(safe-area-inset-top)); }}
}}
</style>
</head>
<body>
<main>
<p class="eyebrow">PRIVATE WORKSPACE</p>
<h1>Welcome back</h1>
<p class="description">Sign in to continue to Syntaxis.</p>
<form action="/login" method="post">
<!-- LOGIN_ERROR -->
<label>Password
<input name="password" type="password" autocomplete="current-password" required autofocus>
</label>
<button type="submit">Sign in</button>
</form>
</main>
</body>
</html>"#
    );
    html.replace("<!-- LOGIN_ERROR -->", &error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_parser_matches_complete_cookie_names() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_static("other=x; __Host-syntaxis-session=secret; suffix=y"),
        );

        assert_eq!(cookie_value(&headers, SECURE_COOKIE_NAME), Some("secret"));
        assert_eq!(cookie_value(&headers, "session"), None);
    }

    #[test]
    fn same_origin_compares_request_host() {
        let matching = Request::builder()
            .method(Method::POST)
            .header(HOST, "code.example.test")
            .header(ORIGIN, "https://code.example.test")
            .body(axum::body::Body::empty())
            .unwrap();
        let foreign = Request::builder()
            .method(Method::POST)
            .header(HOST, "code.example.test")
            .header(ORIGIN, "https://evil.example.test")
            .body(axum::body::Body::empty())
            .unwrap();

        assert!(origin_is_allowed(&matching));
        assert!(!origin_is_allowed(&foreign));
    }

    #[test]
    fn login_page_replaces_the_error_placeholder() {
        let without_error = login_html(None);
        let with_error = login_html(Some("Incorrect password."));

        assert!(!without_error.contains("LOGIN_ERROR"));
        assert!(!without_error.contains("{error}"));
        assert!(with_error.contains("Incorrect password."));
    }
}
