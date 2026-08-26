use super::*;

#[component]
pub(crate) fn RemoteDialog(
    workspace_slug: String,
    remote: Option<RemoteInfo>,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_submit: EventHandler<RemoteRequest>,
) -> Element {
    let editing = remote.is_some();
    let initial_name = remote
        .as_ref()
        .map_or_else(|| "origin".into(), |remote| remote.name.clone());
    let initial_fetch_url = remote
        .as_ref()
        .map(|remote| remote.fetch_url.clone())
        .unwrap_or_default();
    let initial_push_url = remote.as_ref().map_or_else(String::new, |remote| {
        if remote.push_url == remote.fetch_url {
            String::new()
        } else {
            remote.push_url.clone()
        }
    });
    let unchanged_fetch_url = initial_fetch_url.clone();
    let mut name = use_signal(|| initial_name);
    let mut fetch_url = use_signal(|| initial_fetch_url);
    let mut push_url = use_signal(|| initial_push_url);
    let mut checking = use_signal(|| false);
    let mut unverified_url = use_signal(|| None::<String>);
    rsx! {
        Modal {
            title: if editing { "Edit remote" } else { "Add remote" },
            description: if editing { "Rename the remote or update its fetch and push URLs." } else { "Add a named Git remote. The push URL defaults to the fetch URL." },
            on_close,
            DialogForm {
                Field { control_id: "remote-name", label: "Name",
                    TextInput {
                        value: name(),
                        autofocus: true,
                        disabled: pending || checking(),
                        placeholder: "origin",
                        oninput: move |event: FormEvent| name.set(event.value()),
                    }
                }
                Field { control_id: "remote-fetch-url", label: "Fetch URL",
                    TextInput {
                        value: fetch_url(),
                        disabled: pending || checking(),
                        placeholder: "https://example.com/owner/repository.git",
                        oninput: move |event: FormEvent| {
                            fetch_url.set(event.value());
                            unverified_url.set(None);
                        },
                    }
                }
                Field {
                    control_id: "remote-push-url",
                    label: "Push URL (optional)",
                    TextInput {
                        value: push_url(),
                        disabled: pending || checking(),
                        placeholder: "Uses the fetch URL when empty",
                        oninput: move |event: FormEvent| push_url.set(event.value()),
                    }
                }
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                if unverified_url().is_some() {
                    p {
                        class: "rounded-md border border-warning/30 bg-warning/8 p-3 text-xs leading-relaxed text-warning",
                        role: "alert",
                        "This repository could not be verified. It may not exist, require authentication, or be temporarily unreachable. You can still save the remote."
                    }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending || checking(),
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Saving…" } else if checking() { "Checking…" } else if unverified_url().is_some() { "Save anyway" } else if editing { "Save remote" } else { "Add remote" },
                        kind: ButtonKind::Primary,
                        disabled: pending || checking() || name().trim().is_empty() || fetch_url().trim().is_empty(),
                        onclick: move |_| {
                            let push = push_url();
                            let request = remote_request(
                                name().trim().to_owned(),
                                fetch_url().trim().to_owned(),
                                (!push.trim().is_empty()).then(|| push.trim().to_owned()),
                            );
                            let url = request.fetch_url.clone();
                            let already_verified_or_approved = (editing && url == unchanged_fetch_url)
                                || unverified_url().as_deref() == Some(url.as_str());
                            if already_verified_or_approved {
                                on_submit.call(request);
                                return;
                            }
                            checking.set(true);
                            let workspace_slug = workspace_slug.clone();
                            spawn(async move {
                                match api::check_remote(workspace_slug, url.clone()).await {
                                    Ok(true) => on_submit.call(request),
                                    Ok(false) | Err(_) => unverified_url.set(Some(url)),
                                }
                                checking.set(false);
                            });
                        },
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn RemoveRemoteDialog(
    remote: RemoteInfo,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Remove remote?",
            description: "This removes the remote configuration and its remote-tracking branches. It does not delete the remote repository.",
            on_close,
            DialogForm {
                div { class: "rounded-md border border-border bg-secondary/50 p-3",
                    strong { class: "block text-xs", "{remote.name}" }
                    small { class: "mt-1 block truncate text-[10px] text-muted-foreground",
                        {display_remote_url(&remote.fetch_url)}
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
                        label: if pending { "Removing…" } else { "Remove remote" },
                        kind: ButtonKind::Danger,
                        disabled: pending,
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}
