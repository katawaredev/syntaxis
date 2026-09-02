use dioxus::prelude::*;

use crate::{AppIcon, Icon};

/// Canonical action card used to enter or create a workspace.
#[component]
pub fn WorkspaceSourceAction(
    icon: AppIcon,
    title: String,
    description: String,
    #[props(default = false)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let accessible_title = title.clone();
    rsx! {
        button {
            class: "grid min-w-0 grid-cols-[auto_minmax(0,1fr)] items-center gap-3 overflow-hidden rounded-xl border border-border bg-card p-4 text-left shadow-sm outline-none transition-colors hover:border-primary/60 hover:bg-accent/80 focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35 max-[420px]:p-3.5",
            disabled,
            aria_label: accessible_title,
            onclick: move |event| onclick.call(event),
            span { class: "grid size-9 place-items-center rounded-lg bg-primary/10 text-primary",
                Icon { icon, size: 22 }
            }
            span { class: "min-w-0",
                strong { class: "mb-1 block text-foreground", {title.clone()} }
                small { class: "block leading-snug text-muted-foreground", {description} }
            }
        }
    }
}

/// File-picker variant of the canonical workspace source card.
#[component]
pub fn WorkspaceSourceFileAction(
    icon: AppIcon,
    title: String,
    description: String,
    accept: String,
    #[props(default = false)] disabled: bool,
    on_select: EventHandler<Vec<dioxus::html::FileData>>,
) -> Element {
    rsx! {
        label {
            class: if disabled { "relative grid min-w-0 cursor-not-allowed grid-cols-[auto_minmax(0,1fr)] items-center gap-3 overflow-hidden rounded-xl border border-border bg-card p-4 text-left opacity-50 shadow-sm max-[420px]:p-3.5" } else { "relative grid min-w-0 cursor-pointer grid-cols-[auto_minmax(0,1fr)] items-center gap-3 overflow-hidden rounded-xl border border-border bg-card p-4 text-left shadow-sm transition-colors hover:border-primary/60 hover:bg-accent/80 focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/35 max-[420px]:p-3.5" },
            aria_label: title.clone(),
            input {
                class: "absolute inset-0 z-10 cursor-pointer opacity-0",
                r#type: "file",
                name: "workspace-archive",
                accept,
                disabled,
                onchange: move |event: FormEvent| on_select.call(event.files()),
            }
            span { class: "grid size-9 place-items-center rounded-lg bg-primary/10 text-primary",
                Icon { icon, size: 22 }
            }
            span { class: "min-w-0",
                strong { class: "mb-1 block text-foreground", {title.clone()} }
                small { class: "block leading-snug text-muted-foreground", {description} }
            }
        }
    }
}
