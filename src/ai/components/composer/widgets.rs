use dioxus::prelude::*;
use syntaxis_agent::ExtensionWidget;

#[component]
pub(super) fn ExtensionWidgets(widgets: Vec<ExtensionWidget>, placement: String) -> Element {
    rsx! {
        for widget in widgets.into_iter().filter(|widget| widget.placement == placement) {
            div {
                key: "{widget.key}",
                class: "mb-1 max-h-36 overflow-auto rounded-lg border border-border bg-secondary/35 px-3 py-2 font-mono text-[10px] leading-relaxed text-muted-foreground",
                for line in widget.lines {
                    div { "{line}" }
                }
            }
        }
    }
}

#[component]
pub(super) fn QueuePreview(steering: Vec<String>, follow_up: Vec<String>) -> Element {
    if steering.is_empty() && follow_up.is_empty() {
        return rsx! {};
    }
    rsx! {
        div {
            class: "grid max-h-28 gap-1 overflow-y-auto border-b border-border/70 bg-secondary/25 px-3 py-2 text-[10px]",
            role: "status",
            "aria-live": "polite",
            for message in steering {
                div { class: "flex min-w-0 items-center gap-2",
                    span { class: "shrink-0 rounded bg-primary/10 px-1.5 py-0.5 font-medium text-primary",
                        "Next turn"
                    }
                    span { class: "truncate text-muted-foreground", "{message}" }
                }
            }
            for message in follow_up {
                div { class: "flex min-w-0 items-center gap-2",
                    span { class: "shrink-0 rounded bg-secondary px-1.5 py-0.5 font-medium text-foreground",
                        "After task"
                    }
                    span { class: "truncate text-muted-foreground", "{message}" }
                }
            }
        }
    }
}
