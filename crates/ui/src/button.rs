use dioxus::prelude::*;

use crate::ControlSize;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    #[default]
    Secondary,
    Ghost,
    Danger,
}

impl ButtonKind {
    const fn class(self) -> &'static str {
        match self {
            Self::Primary => "bg-primary text-primary-foreground hover:bg-primary/90",
            Self::Secondary => {
                "border border-border bg-secondary text-secondary-foreground hover:bg-accent"
            }
            Self::Ghost => "bg-transparent hover:bg-accent hover:text-accent-foreground",
            Self::Danger => "bg-destructive text-white hover:bg-destructive/90",
        }
    }
}

#[component]
pub fn Button(
    label: String,
    #[props(default)] kind: ButtonKind,
    #[props(default)] size: ControlSize,
    #[props(default = false)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            class: "font-semibold transition-colors {size.button_class()} {kind.class()}",
            disabled,
            onclick: move |event| onclick.call(event),
            {label}
        }
    }
}
