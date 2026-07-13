use dioxus::prelude::*;

use crate::{AppIcon, ControlSize, Icon};

#[component]
pub fn IconButton(
    label: String,
    icon: AppIcon,
    #[props(default)] size: ControlSize,
    #[props(default = false)] pressed: bool,
    #[props(default = false)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            class: if pressed { "inline-flex items-center justify-center bg-accent text-foreground transition-colors hover:bg-accent hover:text-foreground {size.icon_button_class()}" } else { "inline-flex items-center justify-center bg-transparent text-muted-foreground transition-colors hover:bg-accent hover:text-foreground {size.icon_button_class()}" },
            "aria-label": label.clone(),
            title: label,
            "aria-pressed": pressed,
            disabled,
            onclick: move |event| onclick.call(event),
            Icon { icon, size: size.icon_size() }
        }
    }
}
