use dioxus::prelude::*;
use serde_json::{json, Value};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, DialogActions, DialogForm, IconButton, Modal, Tone,
};

use super::{
    api::{self, PiAuthFlow, PiAuthPrompt, PiAuthType, PiSettingsSnapshot},
    generated_settings::{PiSettingDefinition, PiSettingKind, PI_SETTING_DEFINITIONS},
};

pub(super) const ACCOUNTS_SECTION: &str = "Provider accounts";
pub(super) const EXTENSIONS_SECTION: &str = "Extensions";
pub(super) const GENERAL_SECTION: &str = "General";
pub(super) const PROMPT_TEMPLATES_SECTION: &str = "Prompt templates";
pub(super) const SKILLS_SECTION: &str = "Skills";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum AiPanel {
    #[default]
    Chat,
    Settings,
}

#[component]
pub(super) fn AiSidebarTabs(
    mut panel: Signal<AiPanel>,
    on_change: EventHandler<AiPanel>,
) -> Element {
    rsx! {
        div { class: "grid h-12 min-h-12 grid-cols-2 items-center gap-1 border-b border-border p-1.25",
            SidebarTab {
                label: "Chat",
                active: panel() == AiPanel::Chat,
                onclick: move |()| {
                    panel.set(AiPanel::Chat);
                    on_change.call(AiPanel::Chat);
                },
            }
            SidebarTab {
                label: "Settings",
                active: panel() == AiPanel::Settings,
                onclick: move |()| {
                    panel.set(AiPanel::Settings);
                    on_change.call(AiPanel::Settings);
                },
            }
        }
    }
}

#[component]
fn SidebarTab(label: &'static str, active: bool, onclick: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: if active { "file-tree-tab h-8.5 rounded-md bg-muted text-[11px] font-medium text-foreground" } else { "file-tree-tab h-8.5 rounded-md bg-transparent text-[11px] text-muted-foreground hover:bg-muted/60 hover:text-foreground" },
            onclick: move |_| onclick.call(()),
            "{label}"
        }
    }
}

pub(super) fn default_settings_section() -> String {
    GENERAL_SECTION.to_owned()
}

#[component]
pub(super) fn SettingsSidebar(
    mut selected: Signal<String>,
    on_selected: EventHandler<()>,
) -> Element {
    rsx! {
        nav {
            class: "min-h-0 flex-1 overflow-y-auto p-2",
            aria_label: "Pi settings sections",
            for section in setting_sections() {
                button {
                    class: if selected() == section { "mb-1 w-full rounded-lg bg-primary/10 px-3 py-2 text-left text-[11px] font-medium text-primary" } else { "mb-1 w-full rounded-lg px-3 py-2 text-left text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground" },
                    onclick: move |_| {
                        selected.set(section.to_owned());
                        on_selected.call(());
                    },
                    "{section}"
                }
            }
        }
    }
}

#[component]
pub(super) fn SettingsPanel(
    workspace_id: String,
    mut revision: Signal<u64>,
    mut toast: Signal<Option<(String, Tone)>>,
    selected_section: ReadSignal<String>,
    sidebar_open: bool,
    on_toggle_sidebar: EventHandler<()>,
    on_open_sidebar: EventHandler<()>,
) -> Element {
    let settings_workspace_id = workspace_id.clone();
    let settings = use_resource(move || {
        let workspace_id = settings_workspace_id.clone();
        let _ = revision();
        async move { api::pi_settings(workspace_id).await }
    });
    rsx! {
        section { class: "flex h-full min-h-0 flex-col bg-card",
            header { class: "flex min-h-12 items-center gap-3 border-b border-border bg-background px-4",
                ManagementSidebarButton {
                    sidebar_open,
                    on_toggle_sidebar,
                    on_open_sidebar,
                }
                div { class: "min-w-0 flex-1",
                    strong { class: "block text-xs", "{selected_section()}" }
                    small { class: "text-[9px] text-muted-foreground", "Pi settings" }
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto p-5",
                match settings() {
                    None => rsx! {
                        p { class: "text-xs text-muted-foreground", "Loading settings…" }
                    },
                    Some(Err(error)) => rsx! {
                        p { class: "text-xs text-destructive", "{error}" }
                    },
                    Some(Ok(snapshot)) => rsx! {
                        SettingsForm {
                            workspace_id: workspace_id.clone(),
                            snapshot,
                            selected_section,
                            revision,
                            toast,
                        }
                    },
                }
            }
        }
    }
}

#[component]
pub(super) fn ManagementSidebarButton(
    sidebar_open: bool,
    on_toggle_sidebar: EventHandler<()>,
    on_open_sidebar: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "shrink-0 max-md:hidden",
            IconButton {
                label: if sidebar_open { "Hide AI sidebar" } else { "Show AI sidebar" },
                icon: AppIcon::Explorer,
                pressed: sidebar_open,
                onclick: move |_| on_toggle_sidebar.call(()),
            }
        }
        div { class: "hidden shrink-0 max-md:block",
            IconButton {
                label: "Open AI sidebar",
                icon: AppIcon::Explorer,
                onclick: move |_| on_open_sidebar.call(()),
            }
        }
    }
}

#[component]
fn SettingsForm(
    workspace_id: String,
    snapshot: PiSettingsSnapshot,
    selected_section: ReadSignal<String>,
    revision: Signal<u64>,
    toast: Signal<Option<(String, Tone)>>,
) -> Element {
    let saving = use_signal(|| None::<String>);
    let error = use_signal(|| None::<String>);
    rsx! {
        div { class: "mx-auto max-w-3xl",
            if let Some(message) = snapshot.compatibility_message.clone() {
                p { class: "mb-5 rounded-lg bg-warning/10 p-3 text-xs text-warning",
                    "{message}"
                }
            }
            if let Some(message) = error() {
                p { class: "mb-5 rounded-lg bg-destructive/10 p-3 text-xs text-destructive",
                    "{message}"
                }
            }
            if selected_section() == GENERAL_SECTION {
                div { class: "space-y-5",
                    PiUpdate {
                        workspace_id: workspace_id.clone(),
                        revision,
                        toast,
                    }
                    for section in definition_sections() {
                        section {
                            h3 { class: "mb-2 px-1 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground",
                                "{section}"
                            }
                            div { class: "divide-y divide-border overflow-hidden rounded-xl border border-border bg-background",
                                for definition in PI_SETTING_DEFINITIONS
                                    .iter()
                                    .copied()
                                    .filter(|definition| definition.section == section)
                                {
                                    SettingRow {
                                        key: "{definition.path}",
                                        definition,
                                        current: setting_value(&snapshot.values, definition),
                                        disabled: !snapshot.compatible || saving().is_some(),
                                        saving: saving().as_deref() == Some(definition.path),
                                        workspace_id: workspace_id.clone(),
                                        saving_state: saving,
                                        error,
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if selected_section() == ACCOUNTS_SECTION {
                ProviderAccounts { workspace_id: workspace_id.clone() }
            }
        }
    }
}

#[component]
fn PiUpdate(
    workspace_id: String,
    mut revision: Signal<u64>,
    mut toast: Signal<Option<(String, Tone)>>,
) -> Element {
    let mut pending = use_signal(|| false);
    rsx! {
        section {
            h3 { class: "mb-2 px-1 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground",
                "Updates"
            }
            div { class: "flex items-center gap-4 rounded-xl border border-border bg-background px-4 py-3",
                div { class: "min-w-0 flex-1",
                    strong { class: "block text-xs font-medium", "Pi and tracked skills" }
                    p { class: "mt-0.5 text-[10px] leading-relaxed text-muted-foreground",
                        "Update Pi packages and refresh tracked skills."
                    }
                }
                Button {
                    label: if pending() { "Updating…" } else { "Update" },
                    kind: ButtonKind::Primary,
                    disabled: pending(),
                    onclick: move |_| {
                        pending.set(true);
                        let workspace_id = workspace_id.clone();
                        spawn(async move {
                            match api::update_pi(workspace_id).await {
                                Ok(result) => toast.set(Some((result.message, Tone::Success))),
                                Err(error) => {
                                    toast.set(Some((error.to_string(), Tone::Destructive)));
                                }
                            }
                            pending.set(false);
                            revision.with_mut(|revision| *revision += 1);
                        });
                    },
                }
            }
        }
    }
}

#[component]
fn ProviderAccounts(workspace_id: String) -> Element {
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
            match api::start_pi_provider_login(workspace_id, provider_id, auth_type).await {
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
                                flow.set(Some(snapshot));
                                if finished {
                                    revision.with_mut(|revision| *revision += 1);
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
                                                        match api::logout_pi_provider(workspace_id, provider_id).await {
                                                            Ok(()) => revision.with_mut(|revision| *revision += 1),
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

#[component]
fn SettingRow(
    definition: PiSettingDefinition,
    current: String,
    disabled: bool,
    saving: bool,
    workspace_id: String,
    mut saving_state: Signal<Option<String>>,
    mut error: Signal<Option<String>>,
) -> Element {
    let mut draft = use_signal(|| current.clone());
    use_effect(use_reactive((&current,), move |(current,)| {
        draft.set(current);
    }));
    let previous_value = current.clone();
    let save = EventHandler::new(move |value: Value| {
        saving_state.set(Some(definition.path.into()));
        error.set(None);
        let workspace_id = workspace_id.clone();
        let rollback_value = previous_value.clone();
        spawn(async move {
            match api::update_pi_setting(workspace_id, definition.path.into(), value).await {
                Ok(snapshot) => draft.set(setting_value(&snapshot.values, definition)),
                Err(update_error) => {
                    draft.set(rollback_value);
                    error.set(Some(update_error.to_string()));
                }
            }
            saving_state.set(None);
        });
    });
    rsx! {
        div { class: "grid grid-cols-[minmax(0,1fr)_minmax(9rem,14rem)] items-center gap-4 px-4 py-3 max-sm:grid-cols-1",
            div { class: "min-w-0",
                strong { class: "block text-xs font-medium", "{definition.label}" }
                p { class: "mt-0.5 text-[10px] leading-relaxed text-muted-foreground",
                    "{definition.description}"
                }
            }
            div { class: "min-w-0",
                match definition.kind {
                    PiSettingKind::Toggle => rsx! {
                        select {
                            class: "h-8 w-full rounded-lg border border-input bg-background px-2 text-xs",
                            disabled,
                            value: draft(),
                            onchange: move |event| {
                                let value = event.value();
                                draft.set(value.clone());
                                save.call(json!(value == "true"));
                            },
                            option { value: "true", "On" }
                            option { value: "false", "Off" }
                        }
                    },
                    PiSettingKind::Select(options) => rsx! {
                        select {
                            class: "h-8 w-full rounded-lg border border-input bg-background px-2 text-xs",
                            disabled,
                            value: draft(),
                            onchange: move |event| {
                                let value = event.value();
                                draft.set(value.clone());
                                save.call(json!(value));
                            },
                            if definition.default_value.is_empty() {
                                option { value: "", "Not set" }
                            }
                            for option in options {
                                option { value: option, "{option}" }
                            }
                        }
                    },
                    PiSettingKind::Number | PiSettingKind::Text | PiSettingKind::StringArray => {
                        rsx! {
                            input {
                                class: "h-8 w-full rounded-lg border border-input bg-background px-2 text-xs",
                                r#type: if definition.kind == PiSettingKind::Number { "number" } else { "text" },
                                placeholder: if definition.kind == PiSettingKind::StringArray { "Comma-separated values" } else { "" },
                                disabled,
                                value: draft(),
                                oninput: move |event| draft.set(event.value()),
                                onblur: move |_| {
                                    let value = draft();
                                    if value != current {
                                        if let Some(value) = draft_setting_value(definition.kind, &value) {
                                            save.call(value);
                                        }
                                    }
                                },
                            }
                        }
                    }
                }
                if saving {
                    small { class: "text-[9px] text-muted-foreground", "Saving…" }
                }
            }
        }
    }
}

fn draft_setting_value(kind: PiSettingKind, value: &str) -> Option<Value> {
    match kind {
        PiSettingKind::Number => value.parse::<u64>().ok().map(|number| json!(number)),
        PiSettingKind::StringArray => Some(json!(value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>())),
        PiSettingKind::Text => Some(json!(value)),
        PiSettingKind::Toggle | PiSettingKind::Select(_) => None,
    }
}

fn setting_value(values: &Value, definition: PiSettingDefinition) -> String {
    let value = definition
        .path
        .split('.')
        .try_fold(values, |value, segment| value.get(segment));
    match value {
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::String(value)) => value.clone(),
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        _ => definition.default_value.into(),
    }
}

fn setting_sections() -> Vec<&'static str> {
    vec![
        GENERAL_SECTION,
        ACCOUNTS_SECTION,
        PROMPT_TEMPLATES_SECTION,
        SKILLS_SECTION,
        EXTENSIONS_SECTION,
    ]
}

fn definition_sections() -> Vec<&'static str> {
    let mut sections = Vec::new();
    for definition in PI_SETTING_DEFINITIONS {
        if !sections.contains(&definition.section) {
            sections.push(definition.section);
        }
    }
    sections
}
