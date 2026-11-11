use super::*;

#[test]
fn password_hash_upgrade_preserves_existing_argon2id_passwords() {
    // Argon2 reference vector, also supported by the previous 0.5 release.
    let encoded =
        "$argon2id$v=19$m=65536,t=2,p=1$c29tZXNhbHQ$CTFhFdXPJO1aFaMaO6Mm5c8y7cJHAph8ArZWb2GRPPc";
    let parsed = PasswordHash::new(encoded).unwrap();
    assert!(
        Argon2::default()
            .verify_password(b"password", &parsed)
            .is_ok()
    );
    assert!(
        Argon2::default()
            .verify_password(b"incorrect", &parsed)
            .is_err()
    );

    let fresh = Argon2::default().hash_password(b"new password").unwrap();
    let fresh_encoded = fresh.to_string();
    let reparsed = PasswordHash::new(&fresh_encoded).unwrap();
    assert!(
        Argon2::default()
            .verify_password(b"new password", &reparsed)
            .is_ok()
    );
}

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
fn android_pairing_never_accepts_an_unpaired_client() {
    let state = AuthState {
        inner: Arc::new(AuthStateInner {
            disabled: false,
            password_hash: String::new(),
            api_token: Some("0123456789abcdef0123456789abcdef".into()),
            sessions: Mutex::new(HashMap::new()),
            login_failures: Mutex::new(HashMap::new()),
            secure_cookie: false,
        }),
    };
    let unpaired = android_session(&state, &HeaderMap::new());
    assert_eq!(unpaired.status(), StatusCode::NOT_FOUND);
    assert!(!unpaired.headers().contains_key(SET_COOKIE));
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer incorrect"));
    let incorrect = android_session(&state, &headers);
    assert_eq!(incorrect.status(), StatusCode::NOT_FOUND);
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_static("Bearer 0123456789abcdef0123456789abcdef"),
    );
    let paired = android_session(&state, &headers);
    if cfg!(target_os = "android") {
        assert_eq!(paired.status(), StatusCode::NO_CONTENT);
        assert!(paired.headers().contains_key(SET_COOKIE));
    } else {
        assert_eq!(paired.status(), StatusCode::NOT_FOUND);
        assert!(!paired.headers().contains_key(SET_COOKIE));
    }
}
