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
        .expect("matching-origin request should be valid");
    let foreign = Request::builder()
        .method(Method::POST)
        .header(HOST, "code.example.test")
        .header(ORIGIN, "https://evil.example.test")
        .body(axum::body::Body::empty())
        .expect("foreign-origin request should be valid");

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
