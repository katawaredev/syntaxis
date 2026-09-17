use dioxus::prelude::*;

use crate::{Button, ButtonKind, DialogActions, DialogForm, Field, Modal, TextInput};

/// Canonical new-terminal dialog shared by native and browser shells.
#[component]
pub fn NewTerminalDialog(
    mut open: Signal<bool>,
    mut name: Signal<String>,
    mut error: Signal<Option<String>>,
    busy: bool,
    name_error: Option<String>,
    create_disabled: bool,
    shell_label: String,
    on_submit: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "New terminal",
            description: "Start a new shell session in this workspace.",
            on_close: move |()| if !busy { open.set(false) },
            DialogForm {
                Field {
                    control_id: "terminal-name",
                    label: "Name",
                    description: "Optional. Names make terminal tabs easier to identify.",
                    error: name_error,
                    TextInput {
                        placeholder: "shell",
                        value: name(),
                        autofocus: true,
                        disabled: busy,
                        oninput: move |event: FormEvent| {
                            name.set(event.value());
                            error.set(None);
                        },
                        onkeydown: move |event: KeyboardEvent| {
                            if event.key() == Key::Enter {
                                event.prevent_default();
                                on_submit.call(());
                            }
                        },
                    }
                }
                Field { control_id: "terminal-command", label: "Shell",
                    TextInput { value: shell_label, disabled: true }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: busy,
                        onclick: move |_| open.set(false),
                    }
                    Button {
                        label: if busy { "Creating…" } else { "Create terminal" },
                        kind: ButtonKind::Primary,
                        disabled: create_disabled,
                        onclick: move |_| on_submit.call(()),
                    }
                }
            }
        }
    }
}
