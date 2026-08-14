mod ai;
mod app;
#[cfg(feature = "server")]
mod auth;
mod clipboard;
mod files;
mod git;
mod lsp;
mod mock;
mod preview;
mod terminal;
mod workspace;

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
