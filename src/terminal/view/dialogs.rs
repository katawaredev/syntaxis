use dioxus::prelude::*;
use syntaxis_ui::prelude::{
    Button, ButtonKind, DialogActions, DialogForm, Field, Modal, TextInput,
};

#[component]
pub(super) fn NewTerminalDialog(
    mut open: Signal<bool>,
    mut name: Signal<String>,
    mut server_error: Signal<Option<String>>,
    creating: Signal<bool>,
    name_error: Option<String>,
    create_disabled: bool,
    on_submit: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "New terminal",
            description: "Start a new shell session in this workspace.",
            on_close: move |()| {
                if !creating() {
                    open.set(false);
                }
            },
            DialogForm {
                Field {
                    control_id: "terminal-name",
                    label: "Name",
                    description: "Optional. Names make terminal tabs easier to identify.",
                    error: name_error,
                    TextInput {
                        placeholder: "shell",
                        value: "{name}",
                        autofocus: true,
                        disabled: creating(),
                        oninput: move |event: FormEvent| {
                            name.set(event.value());
                            server_error.set(None);
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
                    TextInput { value: "Server default shell", disabled: true }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: creating(),
                        onclick: move |_| open.set(false),
                    }
                    Button {
                        label: if creating() { "Creating…" } else { "Create terminal" },
                        kind: ButtonKind::Primary,
                        disabled: create_disabled,
                        onclick: move |_| on_submit.call(()),
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn AddCommandDialog(
    mut open: Signal<bool>,
    mut label: Signal<String>,
    mut command: Signal<String>,
    mut error: Signal<Option<String>>,
    saving: Signal<bool>,
    on_submit: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Add command",
            description: "Save a command for this project. It will remain available after the server restarts.",
            on_close: move |()| {
                if !saving() {
                    open.set(false);
                }
            },
            DialogForm {
                Field {
                    control_id: "run-command-label",
                    label: "Label",
                    description: "Optional. The command itself is used when left blank.",
                    TextInput {
                        placeholder: "Development server",
                        value: "{label}",
                        disabled: saving(),
                        oninput: move |event: FormEvent| {
                            label.set(event.value());
                            error.set(None);
                        },
                    }
                }
                Field {
                    control_id: "run-command-text",
                    label: "Command",
                    required: true,
                    error: error(),
                    TextInput {
                        placeholder: "npm run dev",
                        value: "{command}",
                        autofocus: true,
                        disabled: saving(),
                        oninput: move |event: FormEvent| {
                            command.set(event.value());
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
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: saving(),
                        onclick: move |_| open.set(false),
                    }
                    Button {
                        label: if saving() { "Saving…" } else { "Add command" },
                        kind: ButtonKind::Primary,
                        disabled: saving() || command().trim().is_empty(),
                        onclick: move |_| on_submit.call(()),
                    }
                }
            }
        }
    }
}
