use dioxus::prelude::*;

/// Keyboard-only shortcut to bypass persistent application chrome.
#[component]
pub fn SkipLink(target_id: String) -> Element {
    rsx! {
        a {
            class: "fixed top-2 left-2 z-300 -translate-y-16 rounded-md bg-primary px-3 py-2 text-xs font-semibold text-primary-foreground shadow-lg outline-none transition-transform focus-visible:translate-y-0 focus-visible:ring-2 focus-visible:ring-ring",
            href: "#{target_id}",
            "Skip to main content"
        }
    }
}
