use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::DropdownMenu;

use crate::{AppIcon, Icon, MenuButtonTrigger, MenuContent};

#[component]
pub fn ComboButton(
    label: String,
    title: String,
    icon: AppIcon,
    #[props(default)] count: Option<String>,
    #[props(default = false)] danger: bool,
    #[props(default = false)] disabled: bool,
    #[props(default = true)] primary_action: bool,
    #[props(default = false)] open: bool,
    menu_label: String,
    #[props(default = "w-56".to_owned())] menu_class: String,
    on_click: EventHandler<()>,
    on_open_change: EventHandler<bool>,
    children: Element,
) -> Element {
    rsx! {
        DropdownMenu {
            class: "relative shrink-0",
            open,
            disabled,
            on_open_change: move |next: bool| on_open_change.call(next),
            div { class: "flex items-stretch",
                button {
                    class: if danger { "touch-target inline-flex h-7 items-center gap-1.5 rounded-l-md border border-destructive/30 bg-destructive/10 px-2 text-[11px] font-medium text-destructive hover:bg-destructive/15 disabled:opacity-50" } else { "touch-target inline-flex h-7 items-center gap-1.5 rounded-l-md border border-border bg-secondary px-2 text-[11px] font-medium text-secondary-foreground hover:bg-accent disabled:opacity-50" },
                    r#type: "button",
                    title: title.clone(),
                    "aria-label": title,
                    disabled,
                    onclick: move |_| {
                        if primary_action {
                            on_click.call(());
                        } else {
                            on_open_change.call(true);
                        }
                    },
                    Icon { icon, size: 14 }
                    "{label}"
                    if let Some(count) = count.as_deref() {
                        span { class: "max-w-20 truncate rounded-sm bg-background/70 px-1 text-[9px] font-normal text-muted-foreground",
                            "{count}"
                        }
                    }
                }
                MenuButtonTrigger {
                    class: if danger { "touch-target inline-flex h-7 items-center justify-center rounded-r-md border border-l-0 border-destructive/30 bg-destructive/10 px-1 text-destructive hover:bg-destructive/15 @max-[520px]:px-3" } else { "touch-target inline-flex h-7 items-center justify-center rounded-r-md border border-l-0 border-border bg-secondary px-1 text-muted-foreground hover:bg-accent hover:text-foreground @max-[520px]:px-3" },
                    label: menu_label.clone(),
                    title: menu_label,
                    on_toggle: move |()| on_open_change.call(!open),
                    Icon { icon: AppIcon::ChevronDown, size: 12 }
                }
            }
            MenuContent { class: "right-0 {menu_class}", {children} }
        }
    }
}
