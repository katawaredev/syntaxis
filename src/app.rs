use crate::{
    ai::{Ai, AiQuery},
    files::{Files, FilesQuery},
    git::Git,
    terminal::{Terminal, TerminalQuery},
    workspace::{Home, Preview, WorkspaceShell},
};
use dioxus::prelude::*;
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const FAVICON: Asset = asset!("/assets/favicon.ico");
const GEIST_FONT: Asset = asset!("/assets/geist-latin-wght-normal.woff2");
const UI_SCRIPT: Asset = asset!("/assets/ui.js");
const AI_CHAT_SCRIPT: Asset = asset!("/assets/ai-chat.js");

// TODO(route-splitting): Enable Dioxus WASM splitting for these routes once the
// upstream fix ships. The 0.7 splitter discovers all six route modules, but
// currently panics in Walrus while emitting the main module with
// `assertion failed: !self.dead.contains(&id)`, so one application bundle is
// still shipped. Track https://github.com/DioxusLabs/dioxus/issues/4769 and
// https://github.com/DioxusLabs/dioxus/pull/5668.
#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[route("/")]
    Home {},
    #[layout(WorkspaceShell)]
    #[route("/workspaces/:slug/files?:..query")]
    Files { slug: String, query: FilesQuery },
    #[route("/workspaces/:slug/terminal?:..query")]
    Terminal { slug: String, query: TerminalQuery },
    #[route("/workspaces/:slug/git")]
    Git { slug: String },
    #[route("/workspaces/:slug/preview")]
    Preview { slug: String },
    #[route("/workspaces/:slug/ai?:..query")]
    Ai { slug: String, query: AiQuery },
}
#[component]
pub fn App() -> Element {
    let notification_center = crate::ai::notifications::use_notification_center();
    use_context_provider(|| notification_center);
    let geist_font_face = format!(
        "@font-face {{ font-family: 'Geist Variable'; src: url('{GEIST_FONT}') format('woff2'); font-style: normal; font-weight: 100 900; font-display: swap; }}",
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
        document::Stylesheet { href: TAILWIND_CSS }
        document::Script { src: UI_SCRIPT }
        document::Script { src: AI_CHAT_SCRIPT }
        Router::<Route> {}
    }
}
