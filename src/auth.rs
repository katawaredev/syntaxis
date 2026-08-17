use std::{
    collections::HashMap,
    io::{self, Write},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use dioxus::server::axum::{
    self, Router,
    extract::{DefaultBodyLimit, Form, Request, State},
    http::{
        HeaderMap, HeaderValue, Method, StatusCode,
        header::{AUTHORIZATION, COOKIE, HOST, ORIGIN, SET_COOKIE},
    },
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rand_core::OsRng;
use serde::Deserialize;
use subtle::ConstantTimeEq;

const SECURE_COOKIE_NAME: &str = "__Host-syntaxis-session";
const INSECURE_COOKIE_NAME: &str = "syntaxis-session";
const SESSION_LIFETIME: Duration = Duration::from_hours(720);
const LOGIN_WINDOW: Duration = Duration::from_mins(5);
const MAX_LOGIN_FAILURES: u8 = 5;
const LOGIN_HTML: &str = include_str!("auth/login.html");

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
        .unwrap_or_else(|message| panic!("Authentication configuration error: {message}"));

    dioxus::serve(move || {
        let state = state.clone();
        async move {
            let auth_layer =
                axum::middleware::from_fn_with_state(state.clone(), require_authentication);
            let preview_layer = axum::middleware::from_fn(crate::preview::server::dispatch);
            let login_page_state = state.clone();
            let login_state = state.clone();
            let logout_state = state.clone();
            let router = Router::new()
                .route("/health", get(health))
                .route("/api/lsp-socket", get(crate::lsp::server::socket))
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
                .layer(auth_layer)
                .layer(preview_layer);
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
    if path == "/api/lsp-socket" && request.headers().get("upgrade").is_some() {
        if !origin_is_allowed(&request) {
            return (StatusCode::FORBIDDEN, "Cross-origin request rejected").into_response();
        }
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

fn login_html(error: Option<&str>) -> String {
    let login_favicon = crate::app::FAVICON.to_string();
    let login_font = crate::app::GEIST_FONT.to_string();
    let error = error.map_or_else(String::new, |message| {
        format!("<p class=\"error\" role=\"alert\">{message}</p>")
    });
    LOGIN_HTML
        .replace("<!-- LOGIN_FAVICON -->", &login_favicon)
        .replace("<!-- LOGIN_FONT -->", &login_font)
        .replace("<!-- LOGIN_ERROR -->", &error)
}

#[cfg(test)]
mod tests;
