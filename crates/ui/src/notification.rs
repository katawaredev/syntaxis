use dioxus::prelude::*;

use crate::{AppIcon, Icon, InteractivePopover};

/// Canonical notification popover shell. Applications provide notification rows while the
/// shared shell owns the trigger, badge, heading, and empty state.
#[component]
pub fn NotificationPopover(
    count: usize,
    open: bool,
    on_open_change: EventHandler<bool>,
    on_clear_all: EventHandler<()>,
    children: Element,
) -> Element {
    let badge_count = count.min(99).to_string();
    rsx! {
        InteractivePopover {
            id: "notifications",
            label: if count == 0 { "Notifications".to_owned() } else { format!("Notifications, {count} unread") },
            title: "Notifications",
            open,
            on_open_change,
            trigger_class: if open { "relative grid size-8 place-items-center rounded-lg bg-accent text-foreground" } else { "relative grid size-8 place-items-center rounded-lg text-muted-foreground hover:bg-accent hover:text-foreground" },
            content_class: "absolute top-[calc(100%+6px)] right-0 z-90 w-[min(360px,calc(100vw-1rem))] overflow-hidden rounded-xl border border-border bg-popover shadow-2xl",
            trigger: rsx! {
                Icon { icon: AppIcon::Bell, size: 15 }
                if count > 0 {
                    span { class: "absolute -top-0.5 -right-0.5 grid h-4 min-w-4 place-items-center rounded-full bg-primary px-1 text-[8px] font-semibold leading-none text-primary-foreground ring-2 ring-background",
                        "{badge_count}"
                    }
                }
            },
            div { class: "flex items-center justify-between border-b border-border px-3 py-2.5",
                strong { class: "text-xs", "Notifications" }
                if count > 0 {
                    button {
                        class: "rounded-md px-2 py-1 text-[9px] font-medium text-muted-foreground hover:bg-accent hover:text-foreground",
                        r#type: "button",
                        aria_label: "Clear all notifications",
                        onclick: move |_| on_clear_all.call(()),
                        "Clear all"
                    }
                }
            }
            div { class: "max-h-[min(420px,70vh)] overflow-y-auto p-1.5",
                if count == 0 {
                    div { class: "px-4 py-8 text-center",
                        div { class: "mx-auto grid size-8 place-items-center rounded-full bg-secondary text-muted-foreground",
                            Icon { icon: AppIcon::Bell, size: 14 }
                        }
                        p { class: "mt-2 text-xs font-medium", "Nothing needs attention" }
                    }
                } else {
                    {children}
                }
            }
        }
    }
}
