use dioxus::prelude::*;

use crate::{field::FieldContext, ControlSize};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum TextInputType {
    #[default]
    Text,
    Email,
    Password,
    Search,
    Url,
}

impl TextInputType {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Email => "email",
            Self::Password => "password",
            Self::Search => "search",
            Self::Url => "url",
        }
    }
}

#[component]
pub fn TextInput(
    #[props(default)] size: ControlSize,
    #[props(default)] input_type: TextInputType,
    #[props(default)] id: Option<String>,
    #[props(default)] class: String,
    #[props(default)] name: Option<String>,
    #[props(default)] value: Option<String>,
    #[props(default)] placeholder: Option<String>,
    #[props(default)] autocomplete: Option<String>,
    #[props(default)] aria_label: Option<String>,
    #[props(default)] aria_describedby: Option<String>,
    #[props(default)] aria_invalid: bool,
    #[props(default)] autofocus: bool,
    #[props(default)] disabled: bool,
    #[props(default)] required: bool,
    #[props(default)] oninput: EventHandler<FormEvent>,
    #[props(default)] onchange: EventHandler<FormEvent>,
    #[props(default)] onkeydown: EventHandler<KeyboardEvent>,
) -> Element {
    let field = try_consume_context::<FieldContext>();
    let id = id.or_else(|| field.as_ref().map(|field| field.control_id.clone()));
    let aria_describedby =
        aria_describedby.or_else(|| field.as_ref().and_then(|field| field.describedby.clone()));
    let aria_invalid = aria_invalid || field.as_ref().is_some_and(|field| field.invalid);
    let required = required || field.as_ref().is_some_and(|field| field.required);
    let class = format!(
        "w-full border border-input bg-background/95 text-foreground shadow-xs outline-none transition-[color,box-shadow] placeholder:text-muted-foreground/70 focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-2 aria-invalid:ring-destructive/20 {} {class}",
        size.input_class()
    );

    rsx! {
        input {
            id,
            class,
            r#type: input_type.as_str(),
            name,
            value,
            placeholder,
            autocomplete,
            aria_label,
            aria_describedby,
            aria_invalid,
            autofocus,
            disabled,
            required,
            oninput: move |event| oninput.call(event),
            onchange: move |event| onchange.call(event),
            onkeydown: move |event| onkeydown.call(event),
        }
    }
}
