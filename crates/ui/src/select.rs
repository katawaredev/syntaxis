use dioxus::prelude::*;

use crate::{field::FieldContext, ControlSize};

#[component]
pub fn Select(
    #[props(default)] size: ControlSize,
    #[props(default)] id: Option<String>,
    #[props(default)] class: String,
    #[props(default)] name: Option<String>,
    #[props(default)] value: Option<String>,
    #[props(default)] aria_label: Option<String>,
    #[props(default)] aria_describedby: Option<String>,
    #[props(default)] aria_invalid: bool,
    #[props(default)] disabled: bool,
    #[props(default)] required: bool,
    #[props(default)] onchange: EventHandler<FormEvent>,
    children: Element,
) -> Element {
    let field = try_consume_context::<FieldContext>();
    let id = id.or_else(|| field.as_ref().map(|field| field.control_id.clone()));
    let aria_describedby =
        aria_describedby.or_else(|| field.as_ref().and_then(|field| field.describedby.clone()));
    let aria_invalid = aria_invalid || field.as_ref().is_some_and(|field| field.invalid);
    let required = required || field.as_ref().is_some_and(|field| field.required);
    let class = format!(
        "w-full border border-input bg-background/95 text-foreground shadow-xs outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-2 aria-invalid:ring-destructive/20 {} {class}",
        size.input_class()
    );

    rsx! {
        select {
            id,
            class,
            name,
            value,
            aria_label,
            aria_describedby,
            aria_invalid,
            disabled,
            required,
            onchange: move |event| onchange.call(event),
            {children}
        }
    }
}
