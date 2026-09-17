use dioxus::prelude::*;

const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const UI_SCRIPT: Asset = asset!("/assets/ui.js");
const GEIST_FONT: Asset = asset!("/assets/geist-latin-wght-normal.woff2");
const FAVICON: Asset = asset!("/assets/favicon.ico");
const FAVICON_SVG: Asset = asset!("/assets/favicon.svg");
const FAVICON_96: Asset = asset!("/assets/favicon-96x96.png");
const APPLE_TOUCH_ICON: Asset = asset!("/assets/apple-touch-icon.png");
const SITE_MANIFEST: Asset = asset!("/assets/site.webmanifest");
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
#[component]
pub fn App() -> Element {
    let services = use_hook(|| {
        let services = syntaxis_runtime_browser::services();
        services
            .validate()
            .unwrap_or_else(|problem| panic!("invalid browser service graph: {problem}"));
        services
    });
    use_context_provider(|| services.clone());
    let geist_font_face = format!(
        "@font-face {{ font-family: 'Geist Variable'; src: url('{GEIST_FONT}') format('woff2'); font-style: normal; font-weight: 100 900; font-display: swap; }}",
    );
    rsx! {
        document::Link { rel: "icon", r#type: "image/svg+xml", href: FAVICON_SVG }
        document::Link { rel: "icon", r#type: "image/png", sizes: "96x96", href: FAVICON_96 }
        document::Link { rel: "shortcut icon", href: FAVICON }
        document::Link { rel: "apple-touch-icon", sizes: "180x180", href: APPLE_TOUCH_ICON }
        document::Link { rel: "manifest", href: SITE_MANIFEST }
        document::Meta { name: "theme-color", content: "#1f2021" }
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
        syntaxis_app_shell::SyntaxisApp {}
    }
}
