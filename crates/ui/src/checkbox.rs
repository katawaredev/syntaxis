use dioxus::prelude::*;
use dioxus_primitives::checkbox::{
    Checkbox as PrimitiveCheckbox, CheckboxIndicator, CheckboxState,
};

use crate::{field::FieldContext, AppIcon, Icon};

#[component]
pub fn Checkbox(
    checked: bool,
    #[props(default)] indeterminate: bool,
    #[props(default)] disabled: bool,
    #[props(default)] required: bool,
    #[props(default)] id: Option<String>,
    #[props(default)] class: String,
    #[props(default)] aria_label: Option<String>,
    #[props(default)] aria_describedby: Option<String>,
    #[props(default)] aria_invalid: bool,
    #[props(default)] name: String,
    #[props(default = "on".to_owned())] value: String,
    on_checked_change: EventHandler<bool>,
) -> Element {
    let field = try_consume_context::<FieldContext>();
    let id = id.or_else(|| field.as_ref().map(|field| field.control_id.clone()));
    let aria_describedby = aria_describedby.or_else(|| {
        let field = field.as_ref()?;
        field.describedby.clone()
    });
    let aria_invalid = aria_invalid || field.as_ref().is_some_and(|field| field.invalid);
    let required = required || field.as_ref().is_some_and(|field| field.required);
    let state = if indeterminate {
        CheckboxState::Indeterminate
    } else if checked {
        CheckboxState::Checked
    } else {
        CheckboxState::Unchecked
    };

    let class = format!(
        "inline-flex size-4 shrink-0 items-center justify-center rounded-[4px] border border-input bg-background text-primary-foreground shadow-xs outline-none transition-colors hover:border-ring focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35 disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:border-primary data-[state=checked]:bg-primary data-[state=indeterminate]:border-primary data-[state=indeterminate]:bg-primary {class}"
    );

    rsx! {
        PrimitiveCheckbox {
            id,
            class,
            aria_label,
            aria_describedby,
            aria_invalid,
            checked: state,
            disabled,
            required,
            name,
            value,
            on_checked_change: move |next: CheckboxState| {
                on_checked_change.call(next != CheckboxState::Unchecked);
            },
            CheckboxIndicator { class: "flex items-center justify-center",
                if state == CheckboxState::Indeterminate {
                    span { class: "h-0.5 w-2 bg-current" }
                } else {
                    Icon { icon: AppIcon::Check, size: 12 }
                }
            }
        }
    }
}
