use dioxus::prelude::*;
use syntaxis_agent::ExtensionUiRequest;
use syntaxis_ui::prelude::{Button, ButtonKind, DialogActions, DialogForm, Modal};

#[component]
pub(crate) fn ExtensionRequestDialog(
    request: ExtensionUiRequest,
    on_respond: EventHandler<(Option<String>, Option<bool>, bool)>,
) -> Element {
    let mut value = use_signal(|| request.prefill.clone().unwrap_or_default());
    let description = if request.message.is_empty() {
        "A Pi extension needs your input.".to_owned()
    } else {
        request.message.clone()
    };
    rsx! {
        Modal {
            title: request.title.clone(),
            description,
            on_close: move |()| on_respond.call((None, None, true)),
            DialogForm {
                if request.method == "select" {
                    div { class: "grid gap-2",
                        for option in request.options.clone() {
                            Button {
                                label: option.clone(),
                                kind: ButtonKind::Secondary,
                                onclick: move |_| on_respond.call((Some(option.clone()), None, false)),
                            }
                        }
                    }
                } else if request.method == "confirm" {
                    DialogActions {
                        Button {
                            label: "No",
                            kind: ButtonKind::Ghost,
                            onclick: move |_| on_respond.call((None, Some(false), false)),
                        }
                        Button {
                            label: "Yes",
                            kind: ButtonKind::Primary,
                            onclick: move |_| on_respond.call((None, Some(true), false)),
                        }
                    }
                } else {
                    textarea {
                        class: "min-h-28 w-full resize-y rounded-md border border-input bg-background p-3 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/20",
                        value: value(),
                        autofocus: true,
                        placeholder: request.placeholder.clone(),
                        oninput: move |event| value.set(event.value()),
                    }
                    DialogActions {
                        Button {
                            label: "Cancel",
                            kind: ButtonKind::Ghost,
                            onclick: move |_| on_respond.call((None, None, true)),
                        }
                        Button {
                            label: "Submit",
                            kind: ButtonKind::Primary,
                            disabled: value().trim().is_empty(),
                            onclick: move |_| on_respond.call((Some(value()), None, false)),
                        }
                    }
                }
            }
        }
    }
}
