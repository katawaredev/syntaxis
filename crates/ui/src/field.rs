use dioxus::prelude::*;
use dioxus_primitives::label::Label;

#[derive(Clone)]
pub(crate) struct FieldContext {
    pub control_id: String,
    pub describedby: Option<String>,
    pub invalid: bool,
    pub required: bool,
}

#[component]
pub fn Field(
    control_id: String,
    label: String,
    #[props(default)] description: Option<String>,
    #[props(default)] error: Option<String>,
    #[props(default)] required: bool,
    children: Element,
) -> Element {
    let description_id = format!("{control_id}-description");
    let error_id = format!("{control_id}-error");
    let describedby = match (description.is_some(), error.is_some()) {
        (true, true) => Some(format!("{description_id} {error_id}")),
        (true, false) => Some(description_id.clone()),
        (false, true) => Some(error_id.clone()),
        (false, false) => None,
    };
    use_context_provider(|| FieldContext {
        control_id: control_id.clone(),
        describedby,
        invalid: error.is_some(),
        required,
    });

    rsx! {
        div { class: "flex flex-col gap-1.5",
            Label {
                class: "text-xs font-semibold text-foreground/80",
                html_for: control_id,
                "{label}"
                if required {
                    span { class: "ml-0.5 text-destructive", aria_hidden: "true", "*" }
                }
            }
            {children}
            if let Some(description) = description {
                p {
                    id: description_id,
                    class: "text-[11px] leading-relaxed text-muted-foreground",
                    {description}
                }
            }
            if let Some(error) = error {
                p {
                    id: error_id,
                    class: "text-[11px] leading-relaxed text-destructive",
                    role: "alert",
                    {error}
                }
            }
        }
    }
}
