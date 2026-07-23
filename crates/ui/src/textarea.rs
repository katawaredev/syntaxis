use dioxus::prelude::*;

use crate::{field::FieldContext, ControlSize};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum TextAreaResize {
    None,
    #[default]
    Vertical,
}

impl TextAreaResize {
    const fn class(self) -> &'static str {
        match self {
            Self::None => "resize-none",
            Self::Vertical => "resize-y",
        }
    }
}

#[component]
pub fn TextArea(
    #[props(default)] size: ControlSize,
    #[props(default)] resize: TextAreaResize,
    #[props(default)] id: Option<String>,
    #[props(default)] class: String,
    #[props(default)] name: Option<String>,
    #[props(default)] value: Option<String>,
    #[props(default)] placeholder: Option<String>,
    #[props(default = 3)] rows: u32,
    #[props(default)] aria_label: Option<String>,
    #[props(default)] aria_describedby: Option<String>,
    #[props(default)] aria_invalid: bool,
    #[props(default)] autofocus: bool,
    #[props(default)] disabled: bool,
    #[props(default)] required: bool,
    #[props(default)] oninput: EventHandler<FormEvent>,
    #[props(default)] onchange: EventHandler<FormEvent>,
) -> Element {
    let field = try_consume_context::<FieldContext>();
    let id = id.or_else(|| field.as_ref().map(|field| field.control_id.clone()));
    let aria_describedby = aria_describedby.or_else(|| {
        let field = field.as_ref()?;
        field.describedby.clone()
    });
    let aria_invalid = aria_invalid || field.as_ref().is_some_and(|field| field.invalid);
    let required = required || field.as_ref().is_some_and(|field| field.required);
    let class = format!(
        "touch-input w-full border border-input bg-background/95 text-foreground shadow-xs outline-none transition-[color,box-shadow] placeholder:text-muted-foreground/70 focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-2 aria-invalid:ring-destructive/20 {} {} {class}",
        size.text_area_class(),
        resize.class()
    );

    rsx! {
        textarea {
            id,
            class,
            name,
            value,
            placeholder,
            rows,
            aria_label,
            aria_describedby,
            aria_invalid,
            autofocus,
            disabled,
            required,
            oninput: move |event| oninput.call(event),
            onchange: move |event| onchange.call(event),
        }
    }
}
