use dioxus::prelude::*;
use dioxus_primitives::alert_dialog::{
    AlertDialogContent as DialogContent, AlertDialogDescription as DialogDescription,
    AlertDialogRoot as DialogRoot, AlertDialogTitle as DialogTitle,
};

use crate::{AppIcon, Icon};

#[component]
pub fn Modal(
    title: String,
    #[props(default)] description: String,
    #[props(default)] content_class: String,
    on_close: EventHandler<()>,
    children: Element,
) -> Element {
    // Use the non-light-dismiss primitive for focus trapping and Escape handling.
    // A form rerender can move focus; that is not a request to discard the dialog.
    // Only the backdrop, Escape, and explicit close actions dismiss a modal.
    rsx! {
        DialogRoot {
            open: true,
            on_open_change: move |open: bool| {
                if !open {
                    on_close.call(());
                }
            },
            class: "mobile-modal-root fixed inset-0 z-100 grid touch-pan-y place-items-center overflow-y-auto overscroll-contain bg-background/75 p-4.5 backdrop-blur-sm",
            div {
                class: "absolute inset-0",
                aria_hidden: "true",
                onclick: move |_| on_close.call(()),
            }
            DialogContent {
                role: "dialog",
                class: "relative max-h-[calc(var(--app-height,100dvh)-1.5rem)] w-full max-w-115 touch-pan-y overflow-y-auto overscroll-contain rounded-xl border border-border bg-popover text-popover-foreground shadow-2xl {content_class}",
                header { class: "flex justify-between gap-4.5 px-5 pt-5 pb-2",
                    div {
                        DialogTitle { class: "text-lg font-semibold text-foreground", {title} }
                        if !description.is_empty() {
                            DialogDescription { class: "mt-1 text-[13px] leading-snug text-muted-foreground",
                                {description}
                            }
                        }
                    }
                    button {
                        class: "inline-flex size-8.5 min-w-8.5 items-center justify-center rounded-lg bg-transparent text-muted-foreground hover:bg-accent hover:text-foreground",
                        "aria-label": "Close dialog",
                        r#type: "button",
                        onclick: move |_| on_close.call(()),
                        Icon { icon: AppIcon::Close }
                    }
                }
                {children}
            }
        }
    }
}
