use dioxus::prelude::*;

use crate::{AppIcon, Icon, Tone};

#[component]
pub fn Toast(
    message: String,
    #[props(default = Tone::Success)] tone: Tone,
    on_close: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            class: "fixed right-4.5 bottom-20 z-200 flex max-w-[calc(100vw-2.25rem)] items-center gap-2 rounded-lg border border-border bg-popover px-3 py-2.5 text-xs shadow-xl",
            role: if tone == Tone::Destructive { "alert" } else { "status" },
            span { class: "size-2 rounded-full {tone.dot_class()}" }
            span { {message} }
            button {
                class: "ml-1.5 text-muted-foreground hover:text-foreground",
                "aria-label": "Dismiss notification",
                onclick: move |_| on_close.call(()),
                Icon { icon: AppIcon::Close, size: 14 }
            }
        }
    }
}
