use super::*;

#[component]
pub(crate) fn CommitDialog(
    workspace_slug: String,
    initial_message: String,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_submit: EventHandler<CommitRequest>,
) -> Element {
    let mut message = use_signal(|| initial_message);
    let mut amend = use_signal(|| false);
    let mut skip_hooks = use_signal(|| false);
    rsx! {
        Modal {
            title: if amend() { "Amend previous commit" } else { "Commit staged changes" },
            description: "Git will use the configured identity and signing settings.",
            on_close,
            DialogForm {
                Field { control_id: "commit-message", label: "Commit message",
                    TextArea {
                        rows: 4,
                        value: message(),
                        placeholder: "Describe your changes",
                        autofocus: true,
                        disabled: pending,
                        oninput: move |event: FormEvent| message.set(event.value()),
                    }
                }
                label { class: "compact flex items-center gap-2.5 py-1.75",
                    Checkbox {
                        checked: amend(),
                        aria_label: "Amend previous commit",
                        disabled: pending,
                        on_checked_change: move |checked| {
                            amend.set(checked);
                            if checked && message().trim().is_empty() {
                                let slug = workspace_slug.clone();
                                spawn(async move {
                                    let previous_message =
                                        api::commit_message(slug, "HEAD".into()).await;
                                    if let Ok(previous_message) = previous_message
                                        && amend()
                                        && message().trim().is_empty()
                                    {
                                        message.set(previous_message);
                                    }
                                });
                            }
                        },
                    }
                    span { "Amend previous commit" }
                }
                label { class: "compact flex items-center gap-2.5 py-1.75",
                    Checkbox {
                        checked: skip_hooks(),
                        aria_label: "Skip Git commit validation hooks",
                        disabled: pending,
                        on_checked_change: move |checked| skip_hooks.set(checked),
                    }
                    span { "Skip validations (--no-verify)" }
                }
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Committing…" } else { "Commit" },
                        kind: ButtonKind::Primary,
                        disabled: pending || message().trim().is_empty(),
                        onclick: move |_| {
                            on_submit
                                .call(CommitRequest {
                                    message: message(),
                                    amend: amend(),
                                    skip_hooks: skip_hooks(),
                                    signing_passphrase: None,
                                });
                        },
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn SigningDialog(
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_submit: EventHandler<String>,
) -> Element {
    let mut passphrase = use_signal(String::new);
    let submit = EventHandler::new(move |()| {
        if pending || passphrase().is_empty() {
            return;
        }
        let secret = std::mem::take(&mut *passphrase.write());
        on_submit.call(secret);
    });
    rsx! {
        Modal {
            title: "Signing passphrase required",
            description: "The passphrase is sent only for this commit retry and is not stored.",
            on_close,
            DialogForm {
                Field {
                    control_id: "signing-passphrase",
                    label: "Signing passphrase",
                    TextInput {
                        input_type: TextInputType::Password,
                        value: passphrase(),
                        autofocus: true,
                        disabled: pending,
                        oninput: move |event: FormEvent| passphrase.set(event.value()),
                        onkeydown: move |event: KeyboardEvent| {
                            if event.key() == Key::Enter {
                                event.prevent_default();
                                submit.call(());
                            }
                        },
                    }
                }
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Signing…" } else { "Retry signed commit" },
                        kind: ButtonKind::Primary,
                        disabled: pending || passphrase().is_empty(),
                        onclick: move |_| submit.call(()),
                    }
                }
            }
        }
    }
}
