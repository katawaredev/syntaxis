use std::collections::{HashMap, HashSet};

use dioxus::{
    fullstack::HeaderMap,
    prelude::ServerFnError,
    server::axum::{
        body::{Body, to_bytes},
        extract::{FromRequest, Request, WebSocketUpgrade, ws::Message as AxumMessage},
        http::{
            HeaderName, HeaderValue, StatusCode,
            header::{
                CONNECTION, CONTENT_LENGTH, HOST, LOCATION, ORIGIN, PROXY_AUTHENTICATE,
                PROXY_AUTHORIZATION, SET_COOKIE, TE, TRAILER, TRANSFER_ENCODING, UPGRADE,
            },
            uri::Authority,
        },
        middleware::Next,
        response::{IntoResponse, Response},
    },
};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message as TungsteniteMessage, client::IntoClientRequest},
};
use url::Url;

use super::{
    authority, origin, request_error,
    state::{Lease, invalidate_lease, leases},
    target::{TARGET_PROBE_TIMEOUT, http_client, target_label},
};

const MAX_REQUEST_BODY_BYTES: usize = 32 * 1024 * 1024;

pub(super) fn gateway_url(base: &Url, label: &str) -> Result<Url, ServerFnError> {
    let mut url = base.clone();
    let base_host = url
        .host_str()
        .ok_or_else(|| request_error("The preview origin has no hostname.", 500))?;
    let public_host = format!("{label}.{base_host}");
    url.set_host(Some(&public_host))
        .map_err(|_| request_error("The preview hostname is invalid.", 500))?;
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

pub(super) fn preview_base_url(parent_origin: &str) -> Result<Url, ServerFnError> {
    if let Ok(configured) = std::env::var("SYNTAXIS_PREVIEW_ORIGIN") {
        if !configured.trim().is_empty() {
            return validate_preview_origin(&configured);
        }
    }
    local_preview_base_url(parent_origin, dioxus_backend_port())
}

pub(super) fn request_origin(headers: &HeaderMap) -> Result<String, ServerFnError> {
    for name in ["origin", "referer"] {
        let Some(value) = headers.get(name).and_then(|value| value.to_str().ok()) else {
            continue;
        };
        let Ok(url) = Url::parse(value) else {
            continue;
        };
        if matches!(url.scheme(), "http" | "https") {
            return origin(&url);
        }
    }
    Err(request_error(
        "The browser did not provide a usable origin for the preview.",
        400,
    ))
}

pub(crate) async fn dispatch(request: Request, next: Next) -> Response {
    let access = match preview_access(request.headers()) {
        Some(access) => access,
        None if is_preview_hostname(request.headers()) => return unauthorized(),
        None => return next.run(request).await,
    };
    let request_authority = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    let authorization = match authorize_request(&access, request_authority) {
        Ok(authorization) => authorization,
        Err(response) => return *response,
    };
    if !preview_origin_is_allowed(request.headers(), &authorization.lease) {
        return (
            StatusCode::FORBIDDEN,
            "Cross-origin preview request rejected.",
        )
            .into_response();
    }

    if is_websocket_request(&request) {
        return proxy_websocket(request, authorization.lease).await;
    }
    proxy_http(request, &authorization.lease_id, authorization.lease).await
}

struct Authorization {
    lease_id: String,
    lease: Lease,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PreviewAccess {
    Owner { lease_id: String },
    Share { token: String },
}

fn authorize_request(
    access: &PreviewAccess,
    request_authority: &str,
) -> Result<Authorization, Box<Response>> {
    let leases = leases().map_err(|error| Box::new(error_response(error)))?;
    let Some((lease_id, lease, label)) = resolve_access(&leases, access) else {
        return Err(Box::new(unauthorized()));
    };
    let public_url = gateway_url(&lease.gateway_base, &label)
        .map_err(|error| Box::new(error_response(error)))?;
    let public_authority =
        authority(&public_url).map_err(|error| Box::new(error_response(error)))?;
    if !public_authority.eq_ignore_ascii_case(request_authority) {
        return Err(Box::new(unauthorized()));
    }
    let mut lease = lease.clone();
    lease.public_authority = public_authority;
    lease.public_origin = origin(&public_url).map_err(|error| Box::new(error_response(error)))?;
    Ok(Authorization {
        lease_id: lease_id.to_owned(),
        lease,
    })
}

fn resolve_access<'a>(
    leases: &'a HashMap<String, Lease>,
    access: &PreviewAccess,
) -> Option<(&'a str, &'a Lease, String)> {
    match access {
        PreviewAccess::Owner { lease_id } => {
            let (lease_id, lease) = leases.get_key_value(lease_id)?;
            Some((lease_id, lease, format!("p-{lease_id}")))
        }
        PreviewAccess::Share { token } => {
            let (lease_id, lease) = leases.iter().find(|(_, lease)| {
                lease
                    .share_token
                    .as_deref()
                    .is_some_and(|active| constant_time_eq(active, token))
            })?;
            Some((lease_id, lease, format!("s-{token}")))
        }
    }
}

async fn proxy_http(request: Request, lease_id: &str, lease: Lease) -> Response {
    let client = match http_client() {
        Ok(client) => client,
        Err(error) => return error_response(error),
    };
    let (parts, body) = request.into_parts();
    let Ok(body) = to_bytes(body, MAX_REQUEST_BODY_BYTES).await else {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            "The preview request body is too large.",
        )
            .into_response();
    };
    let upstream_url = upstream_url(&lease, &parts.uri, false);
    let rewrite_origin = parts.headers.contains_key(ORIGIN);
    let referer = parts
        .headers
        .get("referer")
        .and_then(|value| rewrite_referer(value, &lease));
    let mut upstream = client.request(parts.method, upstream_url).body(body);
    let request_headers = filtered_request_headers(&parts.headers);
    for (name, value) in &request_headers {
        upstream = upstream.header(name, value);
    }
    upstream = upstream
        .header(HOST, authority(&lease.upstream).unwrap_or_default())
        .header("x-forwarded-host", &lease.public_authority)
        .header(
            "x-forwarded-proto",
            if lease.secure { "https" } else { "http" },
        );
    if rewrite_origin {
        upstream = upstream.header(ORIGIN, target_label(&lease.upstream));
    }
    if let Some(referer) = referer {
        upstream = upstream.header("referer", referer);
    }

    let Ok(upstream) = upstream.send().await else {
        invalidate_lease(lease_id);
        return bad_gateway(&lease);
    };
    let status = upstream.status();
    let headers = upstream.headers().clone();
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = status;
    copy_response_headers(&headers, response.headers_mut(), &lease);
    harden_gateway_response(&mut response, &lease);
    response
}

async fn proxy_websocket(request: Request, lease: Lease) -> Response {
    let uri = request.uri().clone();
    let subprotocol = request.headers().get("sec-websocket-protocol").cloned();
    let rewrite_origin = request.headers().contains_key(ORIGIN);
    let referer = request
        .headers()
        .get("referer")
        .and_then(|value| rewrite_referer(value, &lease));
    let request_headers = filtered_websocket_request_headers(request.headers());
    let Ok(upgrade) = WebSocketUpgrade::from_request(request, &()).await else {
        return (
            StatusCode::BAD_REQUEST,
            "The preview WebSocket upgrade is invalid.",
        )
            .into_response();
    };

    let url = upstream_url(&lease, &uri, true);
    let Ok(mut upstream_request) = url.as_str().into_client_request() else {
        return gateway_error(
            StatusCode::BAD_GATEWAY,
            "The preview WebSocket target is invalid.",
            &lease,
        );
    };
    for (name, value) in &request_headers {
        upstream_request
            .headers_mut()
            .append(name.clone(), value.clone());
    }
    if let Some(subprotocol) = subprotocol {
        upstream_request
            .headers_mut()
            .insert("sec-websocket-protocol", subprotocol);
    }
    if rewrite_origin {
        if let Ok(origin) = HeaderValue::from_str(&target_label(&lease.upstream)) {
            upstream_request.headers_mut().insert(ORIGIN, origin);
        }
    }
    if let Some(referer) = referer {
        upstream_request.headers_mut().insert("referer", referer);
    }
    upstream_request.headers_mut().insert(
        "x-forwarded-host",
        HeaderValue::from_str(&lease.public_authority)
            .expect("validated preview authorities are valid header values"),
    );
    upstream_request.headers_mut().insert(
        "x-forwarded-proto",
        HeaderValue::from_static(if lease.secure { "https" } else { "http" }),
    );

    let connection =
        tokio::time::timeout(TARGET_PROBE_TIMEOUT, connect_async(upstream_request)).await;
    let (upstream, upstream_response) = match connection {
        Ok(Ok(connection)) => connection,
        Ok(Err(_)) => {
            return gateway_error(
                StatusCode::BAD_GATEWAY,
                "The preview WebSocket endpoint rejected the connection.",
                &lease,
            );
        }
        Err(_) => {
            return gateway_error(
                StatusCode::GATEWAY_TIMEOUT,
                "The preview WebSocket endpoint did not respond in time.",
                &lease,
            );
        }
    };
    let selected_protocol = upstream_response
        .headers()
        .get("sec-websocket-protocol")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let upgrade = if let Some(protocol) = selected_protocol {
        upgrade.protocols([protocol])
    } else {
        upgrade
    };

    upgrade
        .on_upgrade(move |downstream| async move {
            bridge_websockets(downstream, upstream).await;
        })
        .into_response()
}

async fn bridge_websockets(
    downstream: dioxus::server::axum::extract::ws::WebSocket,
    upstream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    let (mut downstream_tx, mut downstream_rx) = downstream.split();
    let (mut upstream_tx, mut upstream_rx) = upstream.split();
    loop {
        tokio::select! {
            message = downstream_rx.next() => {
                let Some(Ok(message)) = message else {
                    break;
                };
                let Some(message) = to_tungstenite(message) else {
                    break;
                };
                if upstream_tx.send(message).await.is_err() {
                    break;
                }
            }
            message = upstream_rx.next() => {
                let Some(Ok(message)) = message else {
                    break;
                };
                let Some(message) = to_axum(message) else {
                    break;
                };
                if downstream_tx.send(message).await.is_err() {
                    break;
                }
            }
        }
    }
}

fn to_tungstenite(message: AxumMessage) -> Option<TungsteniteMessage> {
    match message {
        AxumMessage::Text(text) => Some(TungsteniteMessage::Text(text.to_string().into())),
        AxumMessage::Binary(bytes) => Some(TungsteniteMessage::Binary(bytes)),
        AxumMessage::Ping(bytes) => Some(TungsteniteMessage::Ping(bytes)),
        AxumMessage::Pong(bytes) => Some(TungsteniteMessage::Pong(bytes)),
        AxumMessage::Close(_) => None,
    }
}

fn to_axum(message: TungsteniteMessage) -> Option<AxumMessage> {
    match message {
        TungsteniteMessage::Text(text) => Some(AxumMessage::Text(text.to_string().into())),
        TungsteniteMessage::Binary(bytes) => Some(AxumMessage::Binary(bytes)),
        TungsteniteMessage::Ping(bytes) => Some(AxumMessage::Ping(bytes)),
        TungsteniteMessage::Pong(bytes) => Some(AxumMessage::Pong(bytes)),
        TungsteniteMessage::Close(_) | TungsteniteMessage::Frame(_) => None,
    }
}

fn filtered_request_headers(headers: &HeaderMap) -> HeaderMap {
    filtered_headers(headers, false)
}

fn filtered_websocket_request_headers(headers: &HeaderMap) -> HeaderMap {
    filtered_headers(headers, true)
}

fn filtered_headers(headers: &HeaderMap, websocket: bool) -> HeaderMap {
    let mut filtered = HeaderMap::new();
    let connection_headers = connection_header_names(headers);
    for (name, value) in headers {
        if is_hop_by_hop(name)
            || connection_headers.contains(name)
            || matches!(*name, HOST | CONTENT_LENGTH | ORIGIN)
            || name == "referer"
            || is_forwarding_header(name)
            || (websocket && name.as_str().starts_with("sec-websocket-"))
        {
            continue;
        }
        filtered.append(name.clone(), value.clone());
    }
    filtered
}

fn copy_response_headers(source: &HeaderMap, target: &mut HeaderMap, lease: &Lease) {
    let connection_headers = connection_header_names(source);
    for (name, value) in source {
        if is_hop_by_hop(name)
            || connection_headers.contains(name)
            || *name == CONTENT_LENGTH
            || name == "x-frame-options"
        {
            continue;
        }
        if name == "content-security-policy" {
            if let Some(rewritten) = rewrite_frame_ancestors(value, &lease.parent_origin) {
                target.append(name.clone(), rewritten);
            }
            continue;
        }
        if *name == SET_COOKIE {
            if let Some(rewritten) = host_only_cookie(value) {
                target.append(name.clone(), rewritten);
            }
            continue;
        }
        if *name == LOCATION {
            if let Some(rewritten) = rewrite_location(value, lease) {
                target.append(name.clone(), rewritten);
                continue;
            }
        }
        target.append(name.clone(), value.clone());
    }
}

fn host_only_cookie(value: &HeaderValue) -> Option<HeaderValue> {
    let cookie = value.to_str().ok()?;
    let attributes = cookie
        .split(';')
        .map(str::trim)
        .filter(|attribute| {
            let attribute = *attribute;
            !attribute
                .split_once('=')
                .map_or(attribute, |(name, _)| name)
                .trim()
                .eq_ignore_ascii_case("domain")
        })
        .collect::<Vec<_>>();
    HeaderValue::from_str(&attributes.join("; ")).ok()
}

fn rewrite_frame_ancestors(value: &HeaderValue, parent_origin: &str) -> Option<HeaderValue> {
    let policy = value.to_str().ok()?;
    let mut directives = policy
        .split(';')
        .map(str::trim)
        .filter(|directive| !directive.is_empty())
        .filter(|directive| {
            !directive
                .split_ascii_whitespace()
                .next()
                .is_some_and(|name| name.eq_ignore_ascii_case("frame-ancestors"))
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    directives.push(format!("frame-ancestors {parent_origin}"));
    HeaderValue::from_str(&directives.join("; ")).ok()
}

fn rewrite_location(value: &HeaderValue, lease: &Lease) -> Option<HeaderValue> {
    let location = value.to_str().ok()?;
    let mut url = if location.starts_with("//") {
        lease.upstream.join(location).ok()?
    } else {
        Url::parse(location).ok()?
    };
    if origin(&url).ok()? != origin(&lease.upstream).ok()? {
        return None;
    }
    let public = Url::parse(&lease.public_origin).ok()?;
    url.set_scheme(public.scheme()).ok()?;
    url.set_host(public.host_str()).ok()?;
    url.set_port(public.port()).ok()?;
    HeaderValue::from_str(url.as_str()).ok()
}

fn rewrite_referer(value: &HeaderValue, lease: &Lease) -> Option<HeaderValue> {
    let mut referer = Url::parse(value.to_str().ok()?).ok()?;
    if origin(&referer).ok()? != lease.public_origin {
        return None;
    }
    referer.set_scheme(lease.upstream.scheme()).ok()?;
    referer.set_host(lease.upstream.host_str()).ok()?;
    referer.set_port(lease.upstream.port()).ok()?;
    HeaderValue::from_str(referer.as_str()).ok()
}

fn harden_gateway_response(response: &mut Response, lease: &Lease) {
    let headers = response.headers_mut();
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if !headers.contains_key("content-security-policy") {
        if let Ok(value) =
            HeaderValue::from_str(&format!("frame-ancestors {}", lease.parent_origin))
        {
            headers.insert("content-security-policy", value);
        }
    }
    if let Ok(value) =
        HeaderValue::from_str(&format!("{} {}", lease.workspace_id.0, lease.target_label))
    {
        headers.insert("x-syntaxis-preview", value);
    }
}

fn dioxus_backend_port() -> Option<u16> {
    std::env::var("DIOXUS_CLI_ENABLED")
        .is_ok_and(|value| value == "true")
        .then(|| std::env::var("PORT").ok()?.parse().ok())
        .flatten()
}

fn local_preview_base_url(
    parent_origin: &str,
    backend_port: Option<u16>,
) -> Result<Url, ServerFnError> {
    let parent = Url::parse(parent_origin)
        .map_err(|_| request_error("The browser origin is unavailable.", 400))?;
    let local = match parent.host() {
        Some(url::Host::Domain(host)) => host == "localhost" || host.ends_with(".localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    };
    if !local {
        return Err(request_error(
            "Set SYNTAXIS_PREVIEW_ORIGIN to an HTTP(S) origin whose wildcard subdomains route to Syntaxis.",
            503,
        ));
    }
    let mut preview = parent;
    preview
        .set_host(Some("localhost"))
        .map_err(|_| request_error("The local preview hostname is invalid.", 500))?;
    if let Some(port) = backend_port {
        preview
            .set_port(Some(port))
            .map_err(|()| request_error("The local preview port is invalid.", 500))?;
    }
    preview.set_path("/");
    preview.set_query(None);
    preview.set_fragment(None);
    Ok(preview)
}

fn validate_preview_origin(value: &str) -> Result<Url, ServerFnError> {
    let mut url = Url::parse(value)
        .map_err(|_| request_error("SYNTAXIS_PREVIEW_ORIGIN is not a valid URL.", 500))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.username() != ""
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || url
            .host()
            .is_some_and(|host| matches!(host, url::Host::Ipv4(_) | url::Host::Ipv6(_)))
    {
        return Err(request_error(
            "SYNTAXIS_PREVIEW_ORIGIN must be an HTTP(S) hostname origin without credentials, path, query, or fragment.",
            500,
        ));
    }
    url.set_path("/");
    Ok(url)
}

fn preview_access(headers: &HeaderMap) -> Option<PreviewAccess> {
    let host = request_host(headers)?;
    let label = host.split('.').next()?;
    let (kind, token) = label.split_once('-')?;
    if token.len() != 32 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let token = token.to_ascii_lowercase();
    match kind {
        "p" => Some(PreviewAccess::Owner { lease_id: token }),
        "s" => Some(PreviewAccess::Share { token }),
        _ => None,
    }
}

fn request_host(headers: &HeaderMap) -> Option<String> {
    let authority = headers
        .get(HOST)?
        .to_str()
        .ok()?
        .parse::<Authority>()
        .ok()?;
    Some(authority.host().to_owned())
}

fn is_preview_hostname(headers: &HeaderMap) -> bool {
    let Some(host) = request_host(headers) else {
        return false;
    };
    let label = host.split('.').next().unwrap_or_default();
    if label.starts_with("p-") || label.starts_with("s-") {
        return true;
    }
    let Ok(configured) = std::env::var("SYNTAXIS_PREVIEW_ORIGIN") else {
        return false;
    };
    let Some(base_host) = Url::parse(configured.trim())
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
    else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    let base_host = base_host.to_ascii_lowercase();
    host.strip_suffix(&base_host)
        .is_some_and(|prefix| prefix.ends_with('.') && prefix.len() > 1)
}

fn is_websocket_request(request: &Request) -> bool {
    request
        .headers()
        .get(UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
}

fn preview_origin_is_allowed(headers: &HeaderMap, lease: &Lease) -> bool {
    headers
        .get(ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_none_or(|origin| origin.eq_ignore_ascii_case(&lease.public_origin))
}

fn upstream_url(lease: &Lease, uri: &dioxus::server::axum::http::Uri, websocket: bool) -> Url {
    let mut upstream = lease.upstream.clone();
    if websocket {
        let scheme = if upstream.scheme() == "https" {
            "wss"
        } else {
            "ws"
        };
        upstream
            .set_scheme(scheme)
            .expect("HTTP schemes always have WebSocket equivalents");
    }
    upstream.set_path(uri.path());
    upstream.set_query(uri.query());
    upstream
}

fn is_hop_by_hop(name: &HeaderName) -> bool {
    matches!(
        *name,
        CONNECTION
            | PROXY_AUTHENTICATE
            | PROXY_AUTHORIZATION
            | TE
            | TRAILER
            | TRANSFER_ENCODING
            | UPGRADE
    )
}

fn connection_header_names(headers: &HeaderMap) -> HashSet<HeaderName> {
    headers
        .get_all(CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|name| HeaderName::from_bytes(name.trim().as_bytes()).ok())
        .collect()
}

fn is_forwarding_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "forwarded"
            | "x-forwarded-for"
            | "x-forwarded-host"
            | "x-forwarded-port"
            | "x-forwarded-proto"
            | "x-real-ip"
    )
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    use subtle::ConstantTimeEq;

    left.len() == right.len() && bool::from(left.as_bytes().ct_eq(right.as_bytes()))
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        "This preview session is no longer active.",
    )
        .into_response()
}

fn bad_gateway(lease: &Lease) -> Response {
    gateway_error(
        StatusCode::BAD_GATEWAY,
        "The preview service is no longer reachable. Restart it and reconnect from Syntaxis.",
        lease,
    )
}

fn gateway_error(status: StatusCode, message: &'static str, lease: &Lease) -> Response {
    let mut response = (status, message).into_response();
    harden_gateway_response(&mut response, lease);
    response
}

fn error_response(error: ServerFnError) -> Response {
    let (code, message) = match error {
        ServerFnError::ServerError { code, message, .. } => (code, message),
        other => (500, other.to_string()),
    };
    (
        StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        message,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use syntaxis_workspace::WorkspaceId;

    const OWNER_ID: &str = "0123456789abcdef0123456789abcdef";
    const SHARE_ID: &str = "fedcba9876543210fedcba9876543210";

    fn test_lease(upstream: &str) -> Lease {
        Lease {
            workspace_id: WorkspaceId::new("workspace"),
            upstream: Url::parse(upstream).unwrap(),
            target_label: upstream.into(),
            share_token: None,
            gateway_base: Url::parse("https://preview.example.test/").unwrap(),
            public_authority: format!("p-{OWNER_ID}.preview.example.test"),
            public_origin: format!("https://p-{OWNER_ID}.preview.example.test"),
            parent_origin: "https://syntaxis.example.test".into(),
            secure: true,
        }
    }

    #[test]
    fn preview_hosts_require_exact_owner_or_share_labels() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HOST,
            HeaderValue::from_static("p-0123456789abcdef0123456789abcdef.preview.localhost:8080"),
        );

        assert_eq!(
            preview_access(&headers),
            Some(PreviewAccess::Owner {
                lease_id: OWNER_ID.into(),
            })
        );
        headers.insert(
            HOST,
            HeaderValue::from_static("s-fedcba9876543210fedcba9876543210.preview.localhost:8080"),
        );
        assert_eq!(
            preview_access(&headers),
            Some(PreviewAccess::Share {
                token: SHARE_ID.into(),
            })
        );
        headers.insert(HOST, HeaderValue::from_static("p-too-short.localhost"));
        assert_eq!(preview_access(&headers), None);
    }

    #[test]
    fn configured_origin_requires_a_hostname() {
        validate_preview_origin("https://preview.example.test").unwrap();
        validate_preview_origin("https://127.0.0.1").unwrap_err();
        validate_preview_origin("https://preview.example.test/base").unwrap_err();
        validate_preview_origin("file:///tmp/preview").unwrap_err();
    }

    #[test]
    fn local_loopback_origins_use_localhost_preview_subdomains() {
        for origin in [
            "http://localhost:8080",
            "http://127.0.0.1:8080",
            "http://[::1]:8080",
        ] {
            assert_eq!(
                local_preview_base_url(origin, None).unwrap().as_str(),
                "http://localhost:8080/"
            );
        }
        assert_eq!(
            local_preview_base_url("http://127.0.0.1:8080", Some(33_575))
                .unwrap()
                .as_str(),
            "http://localhost:33575/"
        );
        local_preview_base_url("http://192.0.2.1:8080", None).unwrap_err();
    }

    #[test]
    fn owner_and_share_credentials_are_separate_and_revocable() {
        let mut lease = test_lease("http://127.0.0.1:5173/");
        lease.share_token = Some(SHARE_ID.into());
        let mut leases = HashMap::from([(OWNER_ID.into(), lease)]);

        assert_eq!(
            resolve_access(
                &leases,
                &PreviewAccess::Owner {
                    lease_id: OWNER_ID.into(),
                },
            )
            .map(|(lease_id, _, label)| (lease_id.to_owned(), label)),
            Some((OWNER_ID.into(), format!("p-{OWNER_ID}")))
        );
        assert_eq!(
            resolve_access(
                &leases,
                &PreviewAccess::Share {
                    token: SHARE_ID.into(),
                },
            )
            .map(|(lease_id, _, label)| (lease_id.to_owned(), label)),
            Some((OWNER_ID.into(), format!("s-{SHARE_ID}")))
        );

        leases.get_mut(OWNER_ID).unwrap().share_token = None;
        assert!(
            resolve_access(
                &leases,
                &PreviewAccess::Share {
                    token: SHARE_ID.into(),
                }
            )
            .is_none()
        );
        assert!(
            resolve_access(
                &leases,
                &PreviewAccess::Owner {
                    lease_id: OWNER_ID.into(),
                }
            )
            .is_some()
        );
    }

    #[test]
    fn upstream_requests_preserve_paths_and_upgrade_the_scheme() {
        let lease = test_lease("https://app.example.test/");
        let uri = "/assets/app.js?v=1".parse().unwrap();

        assert_eq!(
            upstream_url(&lease, &uri, false).as_str(),
            "https://app.example.test/assets/app.js?v=1"
        );
        assert_eq!(
            upstream_url(&lease, &uri, true).as_str(),
            "wss://app.example.test/assets/app.js?v=1"
        );
    }

    #[test]
    fn redirects_are_rewritten_only_for_the_configured_upstream() {
        let lease = test_lease("https://app.example.test/");

        assert_eq!(
            rewrite_location(
                &HeaderValue::from_static("https://app.example.test/dashboard"),
                &lease,
            )
            .and_then(|value| value.to_str().ok().map(str::to_owned)),
            Some(format!(
                "https://p-{OWNER_ID}.preview.example.test/dashboard"
            ))
        );
        assert_eq!(
            rewrite_location(
                &HeaderValue::from_static("//app.example.test/settings"),
                &lease,
            )
            .and_then(|value| value.to_str().ok().map(str::to_owned)),
            Some(format!(
                "https://p-{OWNER_ID}.preview.example.test/settings"
            ))
        );
        assert!(
            rewrite_location(
                &HeaderValue::from_static("https://accounts.example.test/login"),
                &lease,
            )
            .is_none()
        );
    }

    #[test]
    fn preview_referers_are_rewritten_without_leaking_the_owner_token() {
        let lease = test_lease("https://app.example.test/");

        assert_eq!(
            rewrite_referer(
                &HeaderValue::from_static(
                    "https://p-0123456789abcdef0123456789abcdef.preview.example.test/dashboard?q=1"
                ),
                &lease,
            )
            .and_then(|value| value.to_str().ok().map(str::to_owned))
            .as_deref(),
            Some("https://app.example.test/dashboard?q=1")
        );
        assert!(
            rewrite_referer(
                &HeaderValue::from_static("https://unrelated.example.test/"),
                &lease,
            )
            .is_none()
        );
    }

    #[test]
    fn proxy_headers_remove_spoofed_forwarding_and_connection_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONNECTION,
            HeaderValue::from_static("keep-alive, x-remove-me"),
        );
        headers.insert("x-remove-me", HeaderValue::from_static("private"));
        headers.insert(
            "x-forwarded-host",
            HeaderValue::from_static("spoofed.example.test"),
        );
        headers.insert(
            ORIGIN,
            HeaderValue::from_static("https://preview.example.test"),
        );
        headers.insert("cookie", HeaderValue::from_static("session=secret"));
        headers.insert(
            "sec-websocket-key",
            HeaderValue::from_static("generated-by-the-browser"),
        );

        let http = filtered_request_headers(&headers);
        assert_eq!(
            http.get("cookie").and_then(|value| value.to_str().ok()),
            Some("session=secret")
        );
        assert!(!http.contains_key("x-remove-me"));
        assert!(!http.contains_key("x-forwarded-host"));
        assert!(!http.contains_key(ORIGIN));

        let websocket = filtered_websocket_request_headers(&headers);
        assert_eq!(
            websocket
                .get("cookie")
                .and_then(|value| value.to_str().ok()),
            Some("session=secret")
        );
        assert!(!websocket.contains_key("sec-websocket-key"));
    }

    #[test]
    fn gateway_replaces_upstream_frame_embedding_rules() {
        let lease = test_lease("https://app.example.test/");
        let mut source = HeaderMap::new();
        source.insert("x-frame-options", HeaderValue::from_static("DENY"));
        source.insert(
            "content-security-policy",
            HeaderValue::from_static(
                "default-src 'self'; frame-ancestors 'none'; script-src 'self'",
            ),
        );
        let mut target = HeaderMap::new();

        copy_response_headers(&source, &mut target, &lease);

        assert_eq!(
            target
                .get("content-security-policy")
                .and_then(|value| value.to_str().ok()),
            Some(
                "default-src 'self'; script-src 'self'; frame-ancestors https://syntaxis.example.test"
            )
        );
        assert!(!target.contains_key("x-frame-options"));
    }

    #[test]
    fn upstream_cookies_are_scoped_to_one_preview_hostname() {
        assert_eq!(
            host_only_cookie(&HeaderValue::from_static(
                "session=secret; Path=/; Domain=.preview.example.test; HttpOnly; Secure"
            ))
            .and_then(|value| value.to_str().ok().map(str::to_owned))
            .as_deref(),
            Some("session=secret; Path=/; HttpOnly; Secure")
        );
    }
}
