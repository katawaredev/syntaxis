mod app;
#[cfg(feature = "server")]
mod auth;

#[cfg(not(feature = "server"))]
fn main() {
    dioxus::launch(app::App);
}

#[cfg(feature = "server")]
fn main() {
    if std::env::args().nth(1).as_deref() == Some("hash-password") {
        auth::print_password_hash()
            .unwrap_or_else(|message| panic!("Could not generate password hash: {message}"));
        return;
    }
    auth::serve();
}
