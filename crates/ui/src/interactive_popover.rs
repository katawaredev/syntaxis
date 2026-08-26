use dioxus::prelude::*;

/// A controlled popover for forms and other interactive content.
///
/// Outside clicks are handled by a backdrop instead of a document capture
/// listener. This keeps pointer and focus events inside the content from being
/// mistaken for outside interactions when the content rerenders.
#[component]
pub fn InteractivePopover(
    id: String,
    label: String,
    #[props(default)] title: String,
    #[props(default)] class: String,
    trigger_class: String,
    content_class: String,
    #[props(default = false)] open: bool,
    #[props(default = false)] disabled: bool,
    on_open_change: EventHandler<bool>,
    trigger: Element,
    children: Element,
) -> Element {
    let trigger_id = format!("{id}-trigger");
    let content_id = format!("{id}-content");
    let title = if title.is_empty() {
        label.clone()
    } else {
        title
    };

    let trigger_change = on_open_change;
    let outside_change = on_open_change;
    let keyboard_change = on_open_change;

    rsx! {
        div {
            class: "relative shrink-0 {class}",
            "data-state": if open { "open" } else { "closed" },
            onkeydown: move |event| {
                if open && event.key() == Key::Escape {
                    event.stop_propagation();
                    keyboard_change.call(false);
                }
            },
            button {
                id: trigger_id.clone(),
                class: trigger_class,
                r#type: "button",
                title,
                "aria-label": label,
                "aria-haspopup": "dialog",
                "aria-expanded": open,
                "aria-controls": content_id.clone(),
                disabled,
                onclick: move |event| {
                    event.stop_propagation();
                    trigger_change.call(!open);
                },
                {trigger}
            }
            if open {
                div {
                    class: "fixed inset-0 z-70 cursor-default",
                    "aria-hidden": "true",
                    onclick: move |event| {
                        event.stop_propagation();
                        outside_change.call(false);
                    },
                }
                div {
                    id: content_id.clone(),
                    class: "touch-popover {content_class}",
                    role: "dialog",
                    "aria-labelledby": trigger_id.clone(),
                    tabindex: "-1",
                    onclick: move |event| event.stop_propagation(),
                    onpointerdown: move |event| event.stop_propagation(),
                    {children}
                }
            }
        }
    }
}
