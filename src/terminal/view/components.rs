use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::DropdownMenuItem;

use super::TerminalAction;

#[component]
pub(super) fn TerminalMenuItem(
    action: TerminalAction,
    index: usize,
    label: String,
    #[props(default)] destructive: bool,
    #[props(default)] disabled: bool,
    on_select: EventHandler<TerminalAction>,
) -> Element {
    rsx! {
        DropdownMenuItem::<TerminalAction> {
            value: action,
            index,
            class: if destructive { "!text-destructive" },
            disabled,
            on_select,
            "{label}"
        }
    }
}
