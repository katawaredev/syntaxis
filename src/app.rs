use crate::{
    ai::{Ai, AiQuery, AiSettings, AiSettingsSection},
    files::{Files, FilesQuery},
    git::Git,
    preview::Preview,
    terminal::{Terminal, TerminalQuery},
    workspace::{Home, WorkspaceShell},
};
use dioxus::prelude::*;
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
pub(crate) const FAVICON: Asset = asset!("/assets/favicon.ico");
const FAVICON_SVG: Asset = asset!("/assets/favicon.svg");
const FAVICON_96: Asset = asset!("/assets/favicon-96x96.png");
const APPLE_TOUCH_ICON: Asset = asset!("/assets/apple-touch-icon.png");
const SITE_MANIFEST: Asset = asset!("/assets/site.webmanifest");
// Manifest icon URLs cannot use Dioxus's default content hashes because they
// are referenced from JSON instead of Rust-generated markup.
#[used]
static WEB_APP_ICON_192: Asset = asset!(
    "/assets/web-app-manifest-192x192.png",
    AssetOptions::builder().with_hash_suffix(false)
);
#[used]
static WEB_APP_ICON_512: Asset = asset!(
    "/assets/web-app-manifest-512x512.png",
    AssetOptions::builder().with_hash_suffix(false)
);
pub(crate) const GEIST_FONT: Asset = asset!("/assets/geist-latin-wght-normal.woff2");
const UI_SCRIPT: Asset = asset!("/assets/ui.js");
const AI_CHAT_SCRIPT: Asset = asset!("/assets/ai-chat.js");
const THEME_COLOR: &str = "#1f2021";

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
    #[redirect("/workspaces/:slug/ai/settings", |slug: String| Route::AiSettings {
        slug,
        section: AiSettingsSection::General,
    })]
    #[route("/workspaces/:slug/ai/settings/:section")]
    AiSettings { slug: String, section: AiSettingsSection },
}
#[component]
pub fn App() -> Element {
    let notification_center = crate::ai::notifications::use_notification_center();
    use_context_provider(|| notification_center);
    let geist_font_face = format!(
        "@font-face {{ font-family: 'Geist Variable'; src: url('{GEIST_FONT}') format('woff2'); font-style: normal; font-weight: 100 900; font-display: swap; }}",
    );
    rsx! {
        document::Link { rel: "icon", r#type: "image/svg+xml", href: FAVICON_SVG }
        document::Link {
            rel: "icon",
            r#type: "image/png",
            sizes: "96x96",
            href: FAVICON_96,
        }
        document::Link { rel: "shortcut icon", href: FAVICON }
        document::Link {
            rel: "apple-touch-icon",
            sizes: "180x180",
            href: APPLE_TOUCH_ICON,
        }
        document::Link { rel: "manifest", href: SITE_MANIFEST }
        document::Meta { name: "theme-color", content: THEME_COLOR }
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

#[component]
pub(crate) fn LogoutButton() -> Element {
    rsx! {
        form { action: "/auth/logout", method: "post",
            button {
                r#type: "submit",
                class: "inline-flex h-8.5 items-center justify-center rounded-lg px-2.5 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground",
                title: "Sign out",
                "Sign out"
            }
        }
    }
}
