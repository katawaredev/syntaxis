use dioxus::prelude::*;
use dioxus_primitives::dialog::{DialogContent, DialogRoot, DialogTitle};

use crate::{AppIcon, Icon};

#[component]
pub fn Drawer(
    title: String,
    label: String,
    content_class: String,
    restore_focus: String,
    on_close: EventHandler<()>,
    children: Element,
) -> Element {
    let restore_after_dismiss = restore_focus.clone();
    let restore_after_button = restore_focus;
    rsx! {
        DialogRoot {
            open: true,
            on_open_change: move |open: bool| {
                if !open {
                    on_close.call(());
                    restore_focus_after_drawer(&restore_after_dismiss);
                }
            },
            class: "mobile-drawer-root fixed inset-0 z-100 grid touch-pan-y place-items-stretch overscroll-contain bg-background/75 backdrop-blur-sm",
            DialogContent {
                class: "mobile-drawer-content {content_class} max-w-[86vw] overscroll-contain shadow-2xl",
                "aria-label": label,
                div { class: "flex h-12 items-center justify-between border-b border-border px-2.5",
                    DialogTitle { {title} }
                    button {
                        class: "inline-flex size-8.5 items-center justify-center rounded-lg text-muted-foreground hover:bg-accent hover:text-foreground",
                        "aria-label": "Close drawer",
                        onclick: move |_| {
                            on_close.call(());
                            restore_focus_after_drawer(&restore_after_button);
                        },
                        Icon { icon: AppIcon::Close }
                    }
                }
                {children}
            }
        }
    }
}

fn restore_focus_after_drawer(selector: &str) {
    document::eval(&format!(
        "requestAnimationFrame(() => document.querySelector({selector:?})?.focus())"
    ));
}
