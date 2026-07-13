use dioxus::prelude::*;

use crate::{
    files::Files,
    git::Git,
    terminal::Terminal,
    workspace::{Ai, Home, Preview, WorkspaceShell},
};

const APP_CSS: Asset = asset!("/assets/app.css");
const FAVICON: Asset = asset!("/assets/favicon.ico");
const GEIST_FONT: Asset = asset!("/assets/geist-latin-wght-normal.woff2");
const UI_SCRIPT: Asset = asset!("/assets/ui.js");

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[route("/")]
    Home {},
    #[layout(WorkspaceShell)]
        #[route("/workspaces/:slug/files")]
        Files { slug: String },
        #[route("/workspaces/:slug/terminal")]
        Terminal { slug: String },
        #[route("/workspaces/:slug/git")]
        Git { slug: String },
        #[route("/workspaces/:slug/preview")]
        Preview { slug: String },
        #[route("/workspaces/:slug/ai")]
        Ai { slug: String },
}

#[component]
pub fn App() -> Element {
    let geist_font_face = format!(
        "@font-face {{ font-family: 'Geist Variable'; src: url('{GEIST_FONT}') format('woff2'); font-style: normal; font-weight: 100 900; font-display: swap; }}"
    );
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link {
            rel: "preload",
            href: GEIST_FONT,
            r#as: "font",
            r#type: "font/woff2",
            crossorigin: "anonymous",
        }
        document::Style { {geist_font_face} }
        document::Stylesheet { href: APP_CSS }
        document::Script { src: UI_SCRIPT }
        Router::<Route> {}
    }
}
