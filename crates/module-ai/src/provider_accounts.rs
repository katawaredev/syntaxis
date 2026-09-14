//! Provider login, polling, and interactive authentication prompts.

#![allow(
    clippy::clone_on_ref_ptr,
    reason = "PortHandle is Rc in WASM and Arc on native targets"
)]

use dioxus::prelude::*;
use syntaxis_ui::prelude::{Button, ButtonKind, DialogActions, DialogForm, Modal};
use syntaxis_workspace::WorkspaceRecord;

use crate::{AiAuthFlow, AiAuthPrompt, AiPorts, AiProviderAuthKind};

#[component]
pub(crate) fn ProviderAccountsPanel(workspace: WorkspaceRecord) -> Element {
    let ports = use_context::<AiPorts>();
    let Some(port) = ports.provider_auth().cloned() else {
        return rsx! {};
    };
    let mut revision = use_signal(|| 0_u64);
    let mut pending = use_signal(|| None::<String>);
    let mut flow = use_signal(|| None::<AiAuthFlow>);
    let mut error = use_signal(|| None::<String>);
    let list_port = port.clone();
    let list_workspace = workspace.clone();
    let providers = use_resource(move || {
        let port = list_port.clone();
        let workspace = list_workspace.clone();
        let _ = revision();
        async move { port.list(&workspace).await }
    });
    let start_port = port.clone();
    let start_workspace = workspace.clone();
    let start = EventHandler::new(move |(provider_id, kind): (String, AiProviderAuthKind)| {
        pending.set(Some(provider_id.clone()));
        error.set(None);
        let port = start_port.clone();
        let workspace = start_workspace.clone();
        spawn(async move {
            match port.start(&workspace, &provider_id, kind).await {
                Ok(started) => {
                    let flow_id = started.id.clone();
                    flow.set(Some(started));
                    pending.set(None);
                    loop {
                        dioxus_sdk_time::sleep(std::time::Duration::from_millis(350)).await;
                        if flow.peek().as_ref().map(|flow| flow.id.as_str())
                            != Some(flow_id.as_str())
                        {
                            break;
                        }
                        let result = port.status(&workspace, &flow_id).await;
                        // A response to an old poll must not reopen a cancelled dialog.
                        if flow.peek().as_ref().map(|flow| flow.id.as_str())
                            != Some(flow_id.as_str())
                        {
                            break;
                        }
                        match result {
                            Ok(snapshot) => {
                                let finished = snapshot.complete || snapshot.error.is_some();
                                flow.set(Some(snapshot));
                                if finished {
                                    *revision.write() += 1;
                                    break;
                                }
                            }
                            Err(problem) => {
                                error.set(Some(problem.message));
                                break;
                            }
                        }
                    }
                }
                Err(problem) => {
                    pending.set(None);
                    error.set(Some(problem.message));
                }
            }
        });
    });
    rsx! {
        div { class: "mt-5 max-w-3xl space-y-4",
            p { class: "text-xs leading-5 text-muted-foreground", "Connect subscriptions or API keys through the runtime's Pi authentication flow." }
            if let Some(message) = error() {
                p { class: "rounded-lg bg-destructive/10 p-3 text-xs text-destructive", "{message}" }
            }
            match providers() {
                None => rsx! { p { class: "text-xs text-muted-foreground", "Loading providers…" } },
                Some(Err(problem)) => rsx! { p { class: "text-xs text-destructive", "{problem.message}" } },
                Some(Ok(items)) => rsx! {
                    div { class: "divide-y divide-border overflow-hidden rounded-xl border border-border bg-background",
                        for provider in items {
                            div { key: "{provider.id}", class: "flex items-center gap-4 px-4 py-3 max-sm:flex-col max-sm:items-stretch",
                                div { class: "min-w-0 flex-1",
                                    strong { class: "block truncate text-xs font-semibold", "{provider.name}" }
                                    small { class: if provider.configured { "text-[10px] text-success" } else { "text-[10px] text-muted-foreground" }, "{provider.status}" }
                                }
                                div { class: "flex flex-wrap gap-1.5",
                                    for method in provider.methods.clone() {
                                        Button {
                                            label: method.label,
                                            kind: ButtonKind::Secondary,
                                            disabled: pending().is_some(),
                                            onclick: {
                                                let provider_id = provider.id.clone();
                                                move |_| start.call((provider_id.clone(), method.kind))
                                            },
                                        }
                                    }
                                    if provider.can_logout {
                                        Button { label: "Log out", kind: ButtonKind::Ghost, disabled: pending().is_some(), onclick: {
                                            let provider_id = provider.id.clone();
                                            let port = port.clone();
                                            let workspace = workspace.clone();
                                            move |_| {
                                                pending.set(Some(provider_id.clone()));
                                                let provider_id = provider_id.clone();
                                                let port = port.clone();
                                                let workspace = workspace.clone();
                                                spawn(async move {
                                                    match port.logout(&workspace, &provider_id).await {
                                                        Ok(()) => *revision.write() += 1,
                                                        Err(problem) => error.set(Some(problem.message)),
                                                    }
                                                    pending.set(None);
                                                });
                                            }
                                        } }
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
        if let Some(active_flow) = flow() {
            ProviderLoginDialog { workspace: workspace.clone(), flow: active_flow, on_close: move |flow_id: String| {
                flow.set(None);
                let port = port.clone();
                let workspace = workspace.clone();
                spawn(async move { let _ = port.cancel(&workspace, &flow_id).await; });
            } }
        }
    }
}

#[component]
fn ProviderLoginDialog(
    workspace: WorkspaceRecord,
    flow: AiAuthFlow,
    on_close: EventHandler<String>,
) -> Element {
    let close_id = flow.id.clone();
    rsx! {
        Modal {
            title: format!("Connect {}", flow.provider_id),
            description: "Follow the provider authentication steps. Credentials are handled by Pi on the application host.",
            on_close: move |()| on_close.call(close_id.clone()),
            DialogForm {
                if let Some(message) = flow.error.clone() {
                    p { class: "rounded-lg bg-destructive/10 p-3 text-xs text-destructive", "{message}" }
                } else if flow.complete {
                    p { class: "rounded-lg bg-success/10 p-3 text-xs text-success", "Provider connected successfully." }
                } else {
                    for event in flow.events.clone() {
                        div { class: "rounded-lg border border-border bg-secondary/25 p-3 text-xs",
                            if !event.message.is_empty() { p { "{event.message}" } }
                            if !event.url.is_empty() { a { class: "mt-2 block break-all text-primary underline", href: event.url, target: "_blank", rel: "noreferrer", "Open authentication page" } }
                            if !event.user_code.is_empty() { code { class: "mt-2 block select-all text-base font-semibold tracking-widest", "{event.user_code}" } }
                        }
                    }
                    if let Some(prompt) = flow.prompt.clone() {
                        ProviderAuthPrompt {
                            key: "{flow.id}:{prompt.id}",
                            workspace: workspace.clone(),
                            flow_id: flow.id.clone(),
                            prompt,
                        }
                    } else {
                        p { class: "text-xs text-muted-foreground", "Waiting for Pi…" }
                    }
                }
                DialogActions {
                    Button { label: if flow.complete || flow.error.is_some() { "Close" } else { "Cancel" }, kind: if flow.complete { ButtonKind::Primary } else { ButtonKind::Ghost }, onclick: move |_| on_close.call(flow.id.clone()) }
                }
            }
        }
    }
}

#[component]
fn ProviderAuthPrompt(
    workspace: WorkspaceRecord,
    flow_id: String,
    prompt: AiAuthPrompt,
) -> Element {
    let ports = use_context::<AiPorts>();
    let Some(port) = ports.provider_auth().cloned() else {
        return rsx! {};
    };
    let mut value = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let prompt_id = prompt.id;
    let submit = EventHandler::new(move |answer: String| {
        submitting.set(true);
        error.set(None);
        let port = port.clone();
        let workspace = workspace.clone();
        let flow_id = flow_id.clone();
        spawn(async move {
            if let Err(problem) = port.respond(&workspace, &flow_id, prompt_id, &answer).await {
                error.set(Some(problem.message));
                submitting.set(false);
            }
        });
    });
    rsx! {
        div { class: "space-y-3 rounded-lg border border-border p-3",
            p { class: "text-xs font-medium", "{prompt.message}" }
            if prompt.kind == "select" {
                for option in prompt.options.clone() {
                    button { r#type: "button", class: "block w-full rounded-lg border border-input px-3 py-2 text-left text-xs hover:bg-accent", disabled: submitting(), onclick: move |_| submit.call(option.id.clone()),
                        strong { class: "block", "{option.label}" }
                        if !option.description.is_empty() { small { class: "text-muted-foreground", "{option.description}" } }
                    }
                }
            } else {
                input { class: "h-9 w-full rounded-lg border border-input bg-background px-3 text-xs", r#type: if prompt.kind == "secret" { "password" } else { "text" }, value: value(), placeholder: prompt.placeholder, disabled: submitting(), oninput: move |event| value.set(event.value()) }
                Button { label: if submitting() { "Submitting…" } else { "Continue" }, kind: ButtonKind::Primary, disabled: submitting() || value().trim().is_empty(), onclick: move |_| submit.call(value()) }
            }
            if let Some(message) = error() { p { class: "text-xs text-destructive", "{message}" } }
        }
    }
}
