use super::*;

#[component]
pub(super) fn ProviderAccounts(
    workspace_id: String,
    on_accounts_changed: EventHandler<()>,
) -> Element {
    let providers_workspace_id = workspace_id.clone();
    let mut revision = use_signal(|| 0_u64);
    let providers = use_resource(move || {
        let workspace_id = providers_workspace_id.clone();
        let _ = revision();
        async move { api::pi_providers(workspace_id).await }
    });
    let mut flow = use_signal(|| None::<PiAuthFlow>);
    let mut pending = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let login_workspace_id = workspace_id.clone();
    let start_login = EventHandler::new(move |(provider_id, auth_type): (String, PiAuthType)| {
        pending.set(Some(provider_id.clone()));
        error.set(None);
        let workspace_id = login_workspace_id.clone();
        spawn(async move {
            match api::start_pi_provider_login(workspace_id.clone(), provider_id, auth_type).await {
                Ok(started) => {
                    let flow_id = started.id.clone();
                    flow.set(Some(started));
                    pending.set(None);
                    loop {
                        dioxus_sdk_time::sleep(std::time::Duration::from_millis(350)).await;
                        if flow().as_ref().map(|flow| flow.id.as_str()) != Some(flow_id.as_str()) {
                            break;
                        }
                        match api::pi_provider_login_status(flow_id.clone()).await {
                            Ok(snapshot) => {
                                let finished = snapshot.complete || snapshot.error.is_some();
                                let complete = snapshot.complete;
                                flow.set(Some(snapshot));
                                if finished {
                                    revision.with_mut(|revision| *revision += 1);
                                    if complete {
                                        match api::reload_pi_agent_runtime(workspace_id.clone())
                                            .await
                                        {
                                            Ok(()) => on_accounts_changed.call(()),
                                            Err(reload_error) => {
                                                error.set(Some(reload_error.to_string()));
                                            }
                                        }
                                    }
                                    break;
                                }
                            }
                            Err(status_error) => {
                                error.set(Some(status_error.to_string()));
                                break;
                            }
                        }
                    }
                }
                Err(start_error) => {
                    pending.set(None);
                    error.set(Some(start_error.to_string()));
                }
            }
        });
    });
    rsx! {
        div { class: "@container space-y-4",
            div { class: "px-1",
                h3 { class: "text-sm font-semibold", "Provider accounts" }
                p { class: "mt-1 text-xs leading-relaxed text-muted-foreground",
                    "Connect subscriptions or store API keys using Pi's authentication flows. Credentials are kept by Pi on the Syntaxis host."
                }
            }
            if let Some(message) = error() {
                p { class: "rounded-lg bg-destructive/10 p-3 text-xs text-destructive",
                    "{message}"
                }
            }
            match providers() {
                None => rsx! {
                    p { class: "px-1 text-xs text-muted-foreground", "Loading providers…" }
                },
                Some(Err(load_error)) => rsx! {
                    p { class: "rounded-lg bg-destructive/10 p-3 text-xs text-destructive", "{load_error}" }
                },
                Some(Ok(items)) => rsx! {
                    div { class: "divide-y divide-border overflow-hidden rounded-xl border border-border bg-background",
                        for provider in items {
                            div {
                                key: "{provider.id}",
                                class: "grid grid-cols-[minmax(0,1fr)_auto] items-center gap-4 px-4 py-3 @max-[520px]:grid-cols-1 @max-[520px]:gap-2",
                                div { class: "min-w-0 flex-1",
                                    strong { class: "block truncate text-xs font-medium", "{provider.name}" }
                                    small { class: if provider.configured { "mt-0.5 block break-words text-[10px] text-success" } else { "mt-0.5 block break-words text-[10px] text-muted-foreground" },
                                        if provider.configured {
                                            "Connected · "
                                        }
                                        "{provider.status}"
                                    }
                                }
                                div { class: "flex shrink-0 flex-wrap justify-end gap-1.5 @max-[520px]:w-full @max-[520px]:justify-start",
                                    for method in provider.methods.clone() {
                                        Button {
                                            label: match method.auth_type {
                                                PiAuthType::ApiKey => "API key",
                                                PiAuthType::Oauth => "Subscription",
                                            },
                                            kind: ButtonKind::Secondary,
                                            disabled: pending().is_some(),
                                            onclick: {
                                                let provider_id = provider.id.clone();
                                                move |_| start_login.call((provider_id.clone(), method.auth_type))
                                            },
                                        }
                                    }
                                    if provider.can_logout {
                                        Button {
                                            label: "Log out",
                                            kind: ButtonKind::Ghost,
                                            disabled: pending().is_some(),
                                            onclick: {
                                                let provider_id = provider.id.clone();
                                                let workspace_id = workspace_id.clone();
                                                move |_| {
                                                    let provider_id = provider_id.clone();
                                                    let workspace_id = workspace_id.clone();
                                                    pending.set(Some(provider_id.clone()));
                                                    error.set(None);
                                                    spawn(async move {
                                                        match api::logout_pi_provider(workspace_id.clone(), provider_id).await {
                                                            Ok(()) => {
                                                                revision.with_mut(|revision| *revision += 1);
                                                                match api::reload_pi_agent_runtime(workspace_id).await {
                                                                    Ok(()) => on_accounts_changed.call(()),
                                                                    Err(reload_error) => {
                                                                        error.set(Some(reload_error.to_string()));
                                                                    }
                                                                }
                                                            }
                                                            Err(logout_error) => {
                                                                error.set(Some(logout_error.to_string()));
                                                            }
                                                        }
                                                        pending.set(None);
                                                    });
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
        if let Some(active_flow) = flow() {
            ProviderLoginDialog {
                flow: active_flow,
                on_close: move |flow_id: String| {
                    flow.set(None);
                    spawn(async move {
                        let _ = api::cancel_pi_provider_login(flow_id).await;
                    });
                },
            }
        }
    }
}

#[component]
fn ProviderLoginDialog(flow: PiAuthFlow, on_close: EventHandler<String>) -> Element {
    let close_id = flow.id.clone();
    rsx! {
        Modal {
            title: format!("Connect {}", flow.provider_id),
            description: "Follow Pi's authentication steps. On a remote host, choose device-code login when offered. If the provider redirects to localhost, copy that final URL from the browser and paste it here.",
            on_close: move |()| on_close.call(close_id.clone()),
            DialogForm {
                if let Some(message) = flow.error.clone() {
                    p { class: "rounded-lg bg-destructive/10 p-3 text-xs text-destructive",
                        "{message}"
                    }
                } else if flow.complete {
                    p { class: "rounded-lg bg-success/10 p-3 text-xs text-success",
                        "Provider connected successfully."
                    }
                } else {
                    for event in flow.events.clone() {
                        div { class: "rounded-lg border border-border bg-secondary/25 p-3 text-xs",
                            if !event.message.is_empty() {
                                p { class: "leading-relaxed", "{event.message}" }
                            }
                            if !event.url.is_empty() {
                                a {
                                    class: "mt-2 block break-all text-primary underline underline-offset-2",
                                    href: event.url,
                                    target: "_blank",
                                    rel: "noreferrer",
                                    "Open authentication page"
                                }
                            }
                            if !event.user_code.is_empty() {
                                code { class: "mt-2 block select-all text-base font-semibold tracking-widest",
                                    "{event.user_code}"
                                }
                            }
                        }
                    }
                    if let Some(prompt) = flow.prompt.clone() {
                        ProviderAuthPrompt {
                            key: "{prompt.id}",
                            flow_id: flow.id.clone(),
                            prompt,
                        }
                    } else {
                        p { class: "text-xs text-muted-foreground", "Waiting for Pi…" }
                    }
                }
                DialogActions {
                    Button {
                        label: if flow.complete || flow.error.is_some() { "Close" } else { "Cancel" },
                        kind: if flow.complete { ButtonKind::Primary } else { ButtonKind::Ghost },
                        onclick: move |_| on_close.call(flow.id.clone()),
                    }
                }
            }
        }
    }
}

#[component]
fn ProviderAuthPrompt(flow_id: String, prompt: PiAuthPrompt) -> Element {
    let mut value = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let submit = EventHandler::new(move |answer: String| {
        submitting.set(true);
        error.set(None);
        let flow_id = flow_id.clone();
        spawn(async move {
            if let Err(submit_error) =
                api::respond_to_pi_provider_login(flow_id, prompt.id, answer).await
            {
                error.set(Some(submit_error.to_string()));
                submitting.set(false);
            }
        });
    });
    rsx! {
        div { class: "space-y-3 rounded-lg border border-border p-3",
            p { class: "text-xs font-medium", "{prompt.message}" }
            if prompt.kind == "select" {
                div { class: "grid gap-2",
                    for option in prompt.options.clone() {
                        button {
                            class: "rounded-lg border border-input bg-background px-3 py-2 text-left text-xs hover:bg-accent disabled:opacity-50",
                            disabled: submitting(),
                            onclick: move |_| submit.call(option.id.clone()),
                            strong { class: "block font-medium", "{option.label}" }
                            if !option.description.is_empty() {
                                small { class: "mt-0.5 block text-[10px] text-muted-foreground",
                                    "{option.description}"
                                }
                            }
                        }
                    }
                }
            } else {
                input {
                    class: "h-9 w-full rounded-lg border border-input bg-background px-3 text-xs",
                    r#type: if prompt.kind == "secret" { "password" } else { "text" },
                    value: value(),
                    placeholder: prompt.placeholder,
                    autofocus: true,
                    disabled: submitting(),
                    oninput: move |event| value.set(event.value()),
                    onkeydown: move |event| {
                        if event.key() == Key::Enter && !value().trim().is_empty() {
                            submit.call(value());
                        }
                    },
                }
                Button {
                    label: if submitting() { "Submitting…" } else { "Continue" },
                    kind: ButtonKind::Primary,
                    disabled: submitting() || value().trim().is_empty(),
                    onclick: move |_| submit.call(value()),
                }
            }
            if let Some(message) = error() {
                p { class: "text-xs text-destructive", "{message}" }
            }
        }
    }
}
