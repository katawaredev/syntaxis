mod app;
mod files;
mod git;
mod mock;
mod terminal;
mod ui;
mod workspace;

fn main() {
    dioxus::launch(app::App);
}
