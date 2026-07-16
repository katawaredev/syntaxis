use dioxus::prelude::*;
use dioxus_primitives::dialog::{DialogContent, DialogDescription, DialogRoot, DialogTitle};

use crate::{AppIcon, Icon};

#[component]
pub fn Modal(
    title: String,
    description: String,
    #[props(default)] content_class: String,
    on_close: EventHandler<()>,
    children: Element,
) -> Element {
    rsx! {
        DialogRoot {
            open: true,
            on_open_change: move |open: bool| {
                if !open {
                    on_close.call(());
                }
            },
            class: "fixed inset-0 z-100 grid place-items-center bg-background/75 p-4.5 backdrop-blur-sm",
            DialogContent { class: "max-h-[calc(100svh-1.5rem)] w-full max-w-115 overflow-y-auto rounded-xl border border-border bg-popover text-popover-foreground shadow-2xl {content_class}",
                header { class: "flex justify-between gap-4.5 px-5 pt-5 pb-2",
                    div {
                        DialogTitle { class: "text-lg font-semibold text-foreground", {title} }
                        DialogDescription { class: "mt-1 text-[13px] leading-snug text-muted-foreground",
                            {description}
                        }
                    }
                    button {
                        class: "inline-flex size-8.5 min-w-8.5 items-center justify-center rounded-lg bg-transparent text-muted-foreground hover:bg-accent hover:text-foreground",
                        "aria-label": "Close dialog",
                        onclick: move |_| on_close.call(()),
                        Icon { icon: AppIcon::Close }
                    }
                }
                {children}
            }
        }
    }
}
