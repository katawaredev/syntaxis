mod ai;
mod app;
mod files;
mod git;
mod mock;
mod terminal;
mod workspace;

fn main() {
    dioxus::launch(app::App);
}
