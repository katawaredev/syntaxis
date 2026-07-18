use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenuContent, DropdownMenuTrigger};

use crate::{AppIcon, ControlSize, Icon};

#[component]
pub fn MenuButtonTrigger(
    class: String,
    label: String,
    #[props(default)] title: String,
    on_toggle: EventHandler<()>,
    children: Element,
) -> Element {
    let title = if title.is_empty() {
        label.clone()
    } else {
        title
    };
    rsx! {
        DropdownMenuTrigger {
            class,
            "aria-label": label,
            title,
            r#as: move |attributes: Vec<Attribute>| {
                let attributes = attributes
                    .into_iter()
                    .filter(|attribute| attribute.name != "onclick")
                    .collect::<Vec<_>>();
                let children = children.clone();
                rsx! {
                    button { onclick: move |_| on_toggle.call(()), ..attributes, {children} }
                }
            },
        }
    }
}

#[component]
pub fn MenuTrigger(
    label: String,
    icon: AppIcon,
    #[props(default)] class: String,
    #[props(default)] size: ControlSize,
    #[props(default = false)] open: bool,
    on_toggle: EventHandler<()>,
) -> Element {
    rsx! {
        MenuButtonTrigger {
            class: if open { "touch-target inline-flex items-center justify-center bg-accent text-foreground transition-colors {size.icon_button_class()} {class}" } else { "touch-target inline-flex items-center justify-center bg-transparent text-muted-foreground transition-colors hover:bg-accent hover:text-foreground {size.icon_button_class()} {class}" },
            label: label.clone(),
            title: label,
            on_toggle,
            Icon { icon, size: size.icon_size() }
        }
    }
}

#[component]
pub fn MenuContent(class: String, children: Element) -> Element {
    rsx! {
        DropdownMenuContent { class: "syntaxis-menu-content absolute top-[calc(100%+5px)] z-80 max-h-[min(32rem,calc(var(--app-height,100dvh)-1rem))] max-w-[calc(100vw-1rem)] touch-pan-y overflow-y-auto overscroll-contain rounded-lg border border-border bg-popover p-1.25 text-popover-foreground shadow-2xl [&_[role=option]]:flex [&_[role=option]]:min-h-8 [&_[role=option]]:w-full [&_[role=option]]:items-center [&_[role=option]]:justify-between [&_[role=option]]:gap-3 [&_[role=option]]:rounded-sm [&_[role=option]]:px-2 [&_[role=option]]:py-1.5 [&_[role=option]]:text-left [&_[role=option]]:text-xs [&_[role=option]]:outline-none [&_[role=option]]:hover:bg-accent [&_[role=option]]:focus-visible:bg-accent [&_[role=option][data-disabled=true]]:cursor-not-allowed [&_[role=option][data-disabled=true]]:opacity-40 [&_hr]:-mx-1.25 [&_hr]:my-1 [&_hr]:h-px [&_hr]:border-0 [&_hr]:bg-border [&_kbd]:ml-auto [&_kbd]:shrink-0 [&_kbd]:whitespace-nowrap [&_kbd]:font-mono [&_kbd]:text-[9px] [&_kbd]:text-muted-foreground {class}",
            {children}
        }
    }
}
