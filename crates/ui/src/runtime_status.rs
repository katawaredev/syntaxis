use dioxus::prelude::*;

use crate::{InteractivePopover, Tone};

/// Canonical workspace runtime status indicator and detail popover.
#[component]
pub fn RuntimeStatusPopover(message: String, tone: Tone) -> Element {
    let mut open = use_signal(|| false);
    let dot_class = match tone {
        Tone::Success => {
            "bg-success shadow-[0_0_0.5rem_color-mix(in_oklch,var(--success),transparent_20%)]"
        }
        Tone::Warning => "bg-warning",
        Tone::Destructive => "bg-destructive",
        Tone::Neutral => "bg-muted-foreground",
    };
    rsx! {
        InteractivePopover {
            id: "runtime-status",
            label: message.clone(),
            title: message.clone(),
            open: open(),
            on_open_change: move |next| open.set(next),
            trigger_class: if open() { "grid size-8 place-items-center rounded-lg bg-accent" } else { "grid size-8 place-items-center rounded-lg hover:bg-accent" },
            content_class: "absolute top-[calc(100%+6px)] right-0 z-90 w-[min(280px,calc(100vw-1rem))] rounded-xl border border-border bg-popover p-3 shadow-2xl",
            trigger: rsx! {
                span { class: "size-2 rounded-full {dot_class}", "aria-hidden": "true" }
            },
            strong { class: "block text-xs text-foreground", "Runtime status" }
            p { class: "mt-1 break-words text-[10px] leading-relaxed text-muted-foreground",
                "{message}"
            }
        }
    }
}
