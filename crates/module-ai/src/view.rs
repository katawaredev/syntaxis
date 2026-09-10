#![allow(
    clippy::clone_on_ref_ptr,
    reason = "PortHandle is Rc in the browser and Arc in native runtimes"
)]

use dioxus::prelude::*;
use syntaxis_app_contracts::{AiSettingsSection, NavigationIntent};
use syntaxis_git::{WorktreeCreateRequest, WorktreeInfo};
use syntaxis_module_files::FilesUiState;
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, DialogActions, DialogForm, Field, Icon, Modal, TextInput,
    TextInputType, Toast, Tone,
};
use syntaxis_workspace::WorkspaceRecord;

use crate::{
    AiAuthFlow, AiAuthPrompt, AiConversation, AiEvent, AiExtension, AiExtensionAction,
    AiManagedFeature, AiPorts, AiPrompt, AiPromptTemplate, AiProviderAuthKind, AiProviderSettings,
    AiResourceScope, AiRole, AiSkill,
};

#[component]
pub fn AiView(
    workspace: WorkspaceRecord,
    base_workspace: Option<WorkspaceRecord>,
    current_head: Option<String>,
    requested_conversation_id: Option<String>,
    on_navigate: EventHandler<NavigationIntent>,
    on_view_conversation: EventHandler<Option<String>>,
    on_stop_viewing: EventHandler<()>,
    on_activate_worktree: EventHandler<WorktreeInfo>,
) -> Element {
    let ports = use_context::<AiPorts>();
    let files = use_context::<FilesUiState>();
    let conversation_port = ports.conversation().cloned();
    let mut conversation = use_signal(AiConversation::default);
    let mut prompt = use_signal(String::new);
    let mut pending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let activity = use_signal(Vec::<AiEvent>::new);
    let mut isolated_open = use_signal(|| false);
    let mut isolated_branch = use_signal(default_isolated_branch);
    let mut isolated_creating = use_signal(|| false);
    let mut isolated_error = use_signal(|| None::<String>);
    let mut loaded_request = use_signal(|| None::<String>);
    let mut list_refresh = use_signal(|| 0_u64);
    let list_workspace = workspace.clone();
    let list_port = conversation_port.clone();
    let conversations = use_resource(move || {
        let workspace = list_workspace.clone();
        let port = list_port.clone();
        let _ = list_refresh();
        async move {
            match port {
                Some(port) => port.list(&workspace).await,
                None => Ok(Vec::new()),
            }
        }
    });
    let model_workspace = workspace.clone();
    let model_port = ports.models().cloned();
    let models = use_resource(move || {
        let workspace = model_workspace.clone();
        let port = model_port.clone();
        let conversation_id = conversation().id;
        async move {
            if conversation_id.is_empty() {
                return Ok(Vec::new());
            }
            match port {
                Some(port) => port.list_models(&workspace, &conversation_id).await,
                None => Ok(Vec::new()),
            }
        }
    });
    let load_workspace = workspace.clone();
    let requested = requested_conversation_id.clone();

    use_effect(move || {
        let conversation_id = conversation().id;
        on_view_conversation.call((!conversation_id.is_empty()).then_some(conversation_id));
    });
    use_drop(move || on_stop_viewing.call(()));

    use_effect(move || {
        let request_key = format!(
            "{}:{}",
            load_workspace.id.0,
            requested.clone().unwrap_or_default(),
        );
        if loaded_request().as_ref() == Some(&request_key) {
            return;
        }
        loaded_request.set(Some(request_key));
        let workspace = load_workspace.clone();
        let conversation_port = conversation_port.clone();
        let requested = requested.clone();
        spawn(async move {
            let Some(port) = conversation_port else {
                error.set(Some(
                    "AI conversations are unavailable in this runtime.".into(),
                ));
                return;
            };
            let result = match requested {
                Some(id) => port.open(&workspace, &id).await,
                None => port.create(&workspace).await,
            };
            match result {
                Ok(value) => {
                    conversation.set(value);
                    *list_refresh.write() += 1;
                }
                Err(problem) => error.set(Some(problem.message)),
            }
        });
    });

    let submit_workspace = workspace.clone();
    let submit_port = ports.conversation().cloned();
    let cancel_workspace = workspace.clone();
    let cancel_port = ports.conversation().cloned();
    let mut submit = move || {
        let text = prompt().trim().to_owned();
        if text.is_empty() || pending() || conversation().id.is_empty() {
            return;
        }
        let Some(port) = submit_port.clone() else {
            error.set(Some(
                "AI conversations are unavailable in this runtime.".into(),
            ));
            return;
        };
        pending.set(true);
        error.set(None);
        prompt.set(String::new());
        let workspace = submit_workspace.clone();
        let conversation_id = conversation().id;
        let reference = files.active_reference();
        spawn(async move {
            match port
                .send(
                    &workspace,
                    &conversation_id,
                    AiPrompt {
                        text,
                        active_file_reference: reference,
                        delivery: crate::AiPromptDelivery::Prompt,
                    },
                )
                .await
            {
                Ok(mut events) => {
                    loop {
                        match events.receive().await {
                            Ok(Some(event)) => {
                                let delta = matches!(&event, AiEvent::AssistantDelta { .. });
                                apply_event(conversation, &event);
                                if !matches!(
                                    &event,
                                    AiEvent::UserMessage(_)
                                        | AiEvent::AssistantCompleted(_)
                                        | AiEvent::AssistantDelta { .. }
                                ) {
                                    push_activity(activity, event);
                                }
                                if delta {
                                    // Apply at most one streamed text update per animation-frame
                                    // interval. Backpressure lets the adapter/socket coalesce the
                                    // remaining transport chunks without flooding Dioxus signals.
                                    dioxus_sdk_time::sleep(std::time::Duration::from_millis(16))
                                        .await;
                                }
                            }
                            Ok(None) => {
                                *list_refresh.write() += 1;
                                break;
                            }
                            Err(problem) => {
                                error.set(Some(problem.message));
                                break;
                            }
                        }
                    }
                }
                Err(problem) => error.set(Some(problem.message)),
            }
            pending.set(false);
        });
    };

    rsx! {
        section { class: "flex size-full min-h-0 flex-col bg-card", "aria-label": "AI assistant",
            header { class: "flex min-h-12 items-center gap-2 border-b border-border bg-background px-3 py-2",
                span { class: "grid size-7 place-items-center rounded-md bg-primary/10 text-primary",
                    Icon { icon: AppIcon::Bot, size: 14 }
                }
                div { class: "min-w-0 flex-1",
                    strong { class: "block truncate text-xs font-semibold", if conversation().title.is_empty() { "New chat" } else { "{conversation().title}" } }
                    small { class: "block truncate text-[9px] text-muted-foreground", "{workspace.name}" }
                }
                if let Some(Ok(items)) = models()
                    && !items.is_empty()
                {
                    select {
                        class: "max-w-52 rounded-md border border-input bg-card px-2 py-1.5 text-xs text-foreground",
                        "aria-label": "AI model",
                        disabled: pending(),
                        value: conversation().selected_model_id.unwrap_or_default(),
                        onchange: {
                            let port = ports.models().cloned();
                            let workspace = workspace.clone();
                            move |event: FormEvent| {
                                let Some(port) = port.clone() else { return; };
                                let model_id = event.value();
                                let conversation_id = conversation().id;
                                if model_id.is_empty() || conversation_id.is_empty() { return; }
                                let workspace = workspace.clone();
                                spawn(async move {
                                    match port.select_model(&workspace, &conversation_id, &model_id).await {
                                        Ok(()) => conversation.write().selected_model_id = Some(model_id),
                                        Err(problem) => error.set(Some(problem.message)),
                                    }
                                });
                            }
                        },
                        for model in items {
                            option { value: model.id.clone(), "{model.label}" }
                        }
                    }
                }
                select {
                    class: "max-w-52 rounded-md border border-input bg-card px-2 py-1.5 text-xs text-foreground",
                    "aria-label": "AI conversation",
                    value: conversation().id,
                    onchange: {
                        let port = ports.conversation().cloned();
                        let workspace = workspace.clone();
                        move |event: FormEvent| {
                            let Some(port) = port.clone() else { return; };
                            let conversation_id = event.value();
                            if conversation_id.is_empty() { return; }
                            let workspace = workspace.clone();
                            spawn(async move {
                                match port.open(&workspace, &conversation_id).await {
                                    Ok(opened) => conversation.set(opened),
                                    Err(problem) => error.set(Some(problem.message)),
                                }
                            });
                        }
                    },
                    option { value: "", "Conversations" }
                    if let Some(Ok(items)) = conversations() {
                        for item in items {
                            option { value: item.id.clone(), "{item.title}" }
                        }
                    }
                }
                Button {
                    label: "New",
                    kind: ButtonKind::Ghost,
                    disabled: pending(),
                    onclick: {
                        let port = ports.conversation().cloned();
                        let workspace = workspace.clone();
                        move |_| {
                            let Some(port) = port.clone() else { return; };
                            let workspace = workspace.clone();
                            spawn(async move {
                                match port.create(&workspace).await {
                                    Ok(created) => {
                                        conversation.set(created);
                                        *list_refresh.write() += 1;
                                    }
                                    Err(problem) => error.set(Some(problem.message)),
                                }
                            });
                        }
                    },
                }
                if ports.worktrees().is_some() {
                    button {
                        class: "touch-target rounded-md bg-transparent px-3 text-sm font-semibold hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50",
                        r#type: "button",
                        disabled: pending() || files.has_dirty() || base_workspace.is_none(),
                        title: if files.has_dirty() { "Save or close modified files before creating an isolated worktree" } else { "Create an isolated worktree and conversation" },
                        onclick: move |_| {
                            isolated_branch.set(default_isolated_branch());
                            isolated_error.set(None);
                            isolated_open.set(true);
                        },
                        "Isolated"
                    }
                }
                Button {
                    label: "Settings",
                    kind: ButtonKind::Ghost,
                    onclick: {
                        let workspace_id = workspace.id.clone();
                        move |_| on_navigate.call(NavigationIntent::AiSettings {
                            workspace: workspace_id.clone(),
                            section: AiSettingsSection::ProviderAccounts,
                        })
                    },
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto p-4", role: "log", "aria-live": "polite",
                if conversation().messages.is_empty() {
                    div { class: "grid min-h-full place-items-center text-center text-sm text-muted-foreground",
                        div {
                            h2 { class: "text-lg font-semibold text-foreground", "Ask about your project" }
                            p { class: "mt-2 max-w-md", "Open a file to include its active editor reference, then send a request." }
                        }
                    }
                }
                for message in conversation().messages {
                    article {
                        key: "{message.id}",
                        class: if message.role == AiRole::User { "ml-auto mb-3 max-w-[80%] rounded-xl bg-primary px-3 py-2 text-sm text-primary-foreground" } else { "mb-3 max-w-[85%] rounded-xl border border-border bg-background px-3 py-2 text-sm text-foreground" },
                        p { class: "whitespace-pre-wrap", "{message.content}" }
                    }
                }
                for event in activity() {
                    match event {
                        AiEvent::ToolStarted { name, .. } => rsx! { p { class: "mb-2 rounded-lg border border-border bg-muted/30 px-3 py-2 text-xs text-muted-foreground", "Running {name}…" } },
                        AiEvent::ToolUpdated { output, .. } | AiEvent::ToolCompleted { output, .. } => rsx! { pre { class: "mb-2 max-h-40 overflow-auto rounded-lg border border-border bg-muted/30 px-3 py-2 text-[11px] text-muted-foreground", "{output}" } },
                        AiEvent::UsageUpdated { input_tokens, output_tokens } => rsx! { p { class: "mb-2 text-[10px] text-muted-foreground", "Usage: {input_tokens} input · {output_tokens} output tokens" } },
                        AiEvent::Failed { message } => rsx! { p { class: "mb-2 text-xs text-destructive", "{message}" } },
                        AiEvent::UserMessage(_) | AiEvent::AssistantDelta { .. } | AiEvent::AssistantCompleted(_) => rsx! {},
                    }
                }
                if pending() { p { class: "text-xs text-muted-foreground", "Waiting for the provider…" } }
            }
            form {
                class: "border-t border-border bg-background p-3",
                onsubmit: move |event| { event.prevent_default(); submit(); },
                textarea {
                    class: "min-h-22 w-full resize-y rounded-lg border border-input bg-card p-3 text-sm text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring",
                    value: prompt,
                    maxlength: 0x0001_0000,
                    placeholder: "Ask the assistant…",
                    oninput: move |event| prompt.set(event.value()),
                }
                div { class: "mt-2 flex items-center gap-2",
                    span { class: "min-w-0 flex-1 truncate text-[10px] text-muted-foreground",
                        if let Some(reference) = files.active_reference() { "Context: {reference}" } else { "No active editor reference" }
                    }
                    if pending() {
                        button {
                            class: "touch-target rounded-md border border-destructive/40 px-3 text-sm font-semibold text-destructive",
                            r#type: "button",
                            onclick: move |_| {
                                let Some(port) = cancel_port.clone() else { return; };
                                let workspace = cancel_workspace.clone();
                                let conversation_id = conversation().id;
                                spawn(async move {
                                    match port.cancel(&workspace, &conversation_id).await {
                                        Ok(()) => {}
                                        Err(problem) => error.set(Some(problem.message)),
                                    }
                                });
                            },
                            "Cancel"
                        }
                    } else {
                        button {
                            class: "touch-target rounded-md bg-primary px-3 text-sm font-semibold text-primary-foreground disabled:opacity-50",
                            r#type: "submit",
                            disabled: prompt().trim().is_empty(),
                            "Send"
                        }
                    }
                }
            }
        }
        if isolated_open() {
            Modal {
                title: "Create an isolated worktree",
                description: "Create a branch and checkout for an independent chat. Files, Terminal, and Git switch with it.",
                on_close: move |()| if !isolated_creating() { isolated_open.set(false); },
                DialogForm {
                    Field { control_id: "ai-isolated-branch", label: "New branch", error: isolated_error(),
                        TextInput { value: isolated_branch(), placeholder: "agent/chat-1234", disabled: isolated_creating(), oninput: move |event: FormEvent| { isolated_branch.set(event.value()); isolated_error.set(None); } }
                    }
                    if files.has_dirty() { p { class: "text-xs text-warning", "Save or close modified files before starting an isolated chat." } }
                    DialogActions {
                        Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: isolated_creating(), onclick: move |_| isolated_open.set(false) }
                        Button { label: if isolated_creating() { "Creating…" } else { "Create worktree" }, kind: ButtonKind::Primary, disabled: isolated_creating() || files.has_dirty() || isolated_branch().trim().is_empty() || base_workspace.is_none(), onclick: {
                            let port = ports.worktrees().cloned();
                            let base_workspace = base_workspace.clone();
                            let start_point = current_head.clone();
                            move |_| {
                                let (Some(port), Some(base)) = (port.clone(), base_workspace.clone()) else { return; };
                                isolated_creating.set(true);
                                isolated_error.set(None);
                                let request = WorktreeCreateRequest { branch: isolated_branch().trim().to_owned(), start_point: start_point.clone(), create_branch: true };
                                spawn(async move {
                                    match port.create(&base, request).await {
                                        Ok(worktree) => {
                                            isolated_open.set(false);
                                            isolated_creating.set(false);
                                            on_activate_worktree.call(worktree);
                                        }
                                        Err(problem) => {
                                            isolated_error.set(Some(problem.message));
                                            isolated_creating.set(false);
                                        }
                                    }
                                });
                            }
                        } }
                    }
                }
            }
        }
        if let Some(message) = error() {
            Toast { message, tone: Tone::Destructive, on_close: move |()| error.set(None) }
        }
    }
}

fn default_isolated_branch() -> String {
    let milliseconds = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    format!("agent/chat-{milliseconds}")
}

#[component]
pub fn AiSettingsView(
    workspace: WorkspaceRecord,
    section: AiSettingsSection,
    on_navigate: EventHandler<NavigationIntent>,
) -> Element {
    let ports = use_context::<AiPorts>();
    let settings_port = ports.settings().cloned();
    let mut settings = use_signal(AiProviderSettings::default);
    let mut loaded = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut notice = use_signal(|| None::<(String, Tone)>);
    let managed = managed_feature(&ports, section);
    let managed_workspace = workspace.clone();
    let managed_summary = use_resource(move || {
        let managed = managed.clone();
        let workspace = managed_workspace.clone();
        async move {
            match managed {
                Some((port, feature)) => Some(port.summary(&workspace, feature).await),
                None => None,
            }
        }
    });
    let load_workspace = workspace.clone();
    use_effect(move || {
        if loaded() {
            return;
        }
        let settings_port = settings_port.clone();
        let Some(settings_port) = settings_port else {
            return;
        };
        loaded.set(true);
        let workspace = load_workspace.clone();
        spawn(async move {
            match settings_port.load(&workspace).await {
                Ok(value) => settings.set(value),
                Err(problem) => notice.set(Some((problem.message, Tone::Destructive))),
            }
        });
    });
    let save_workspace = workspace.clone();
    let save_port = ports.settings().cloned();
    let mut save = move || {
        let Some(port) = save_port.clone() else {
            return;
        };
        saving.set(true);
        let workspace = save_workspace.clone();
        let value = settings();
        spawn(async move {
            match port.save(&workspace, value).await {
                Ok(()) => notice.set(Some(("AI provider settings saved".into(), Tone::Success))),
                Err(problem) => notice.set(Some((problem.message, Tone::Destructive))),
            }
            saving.set(false);
        });
    };
    rsx! {
        section { class: "flex size-full min-h-0 bg-card", "aria-label": "AI settings",
            nav { class: "w-56 shrink-0 border-r border-border bg-background p-3 max-md:w-40",
                Button {
                    label: "Back to chat",
                    kind: ButtonKind::Ghost,
                    onclick: {
                        let workspace_id = workspace.id.clone();
                        move |_| on_navigate.call(NavigationIntent::Ai { workspace: workspace_id.clone(), conversation_id: None })
                    },
                }
                div { class: "mt-3 grid gap-1",
                    for candidate in AiSettingsSection::ALL {
                        {
                            let supported = supports_section(&ports, candidate);
                            let workspace_id = workspace.id.clone();
                            rsx! {
                        button {
                            class: if candidate == section { "rounded-md bg-accent px-2 py-2 text-left text-xs font-semibold text-foreground" } else { "rounded-md px-2 py-2 text-left text-xs text-muted-foreground" },
                            disabled: !supported,
                            title: if supported { "" } else { "Unavailable in this runtime" },
                            onclick: move |_| on_navigate.call(NavigationIntent::AiSettings {
                                workspace: workspace_id.clone(),
                                section: candidate,
                            }),
                            "{candidate.label()}"
                        }
                            }
                        }
                    }
                }
            }
            main { class: "min-w-0 flex-1 overflow-y-auto p-5",
                h1 { class: "text-lg font-semibold", "{section.label()}" }
                if section == AiSettingsSection::ProviderAccounts && ports.provider_auth().is_some() {
                    ProviderAccountsPanel { workspace: workspace.clone() }
                } else if section == AiSettingsSection::GlobalInstructions && ports.resources().is_some() {
                    InstructionsPanel { workspace: workspace.clone() }
                } else if section == AiSettingsSection::PromptTemplates && ports.resources().is_some() {
                    PromptTemplatesPanel { workspace: workspace.clone() }
                } else if section == AiSettingsSection::Skills && ports.resources().is_some() {
                    SkillsPanel { workspace: workspace.clone() }
                } else if section == AiSettingsSection::Extensions && ports.extensions().is_some() {
                    ExtensionsPanel { workspace: workspace.clone() }
                } else if let Some(Some(result)) = managed_summary() {
                    match result {
                        Ok(summary) => rsx! {
                            div { class: "mt-5 max-w-xl rounded-xl border border-border bg-background p-4",
                                h2 { class: "text-sm font-semibold", "{summary.title}" }
                                p { class: "mt-2 text-xs leading-5 text-muted-foreground", "{summary.detail}" }
                                if section == AiSettingsSection::General {
                                    ul { class: "mt-4 grid gap-2 text-xs text-muted-foreground",
                                        if ports.worktrees().is_some() { li { "Isolated worktrees available" } }
                                        if ports.notifications().is_some() { li { "Background notifications available" } }
                                    }
                                }
                            }
                        },
                        Err(problem) => rsx! { p { class: "mt-4 text-sm text-destructive", "{problem.message}" } },
                    }
                } else if section == AiSettingsSection::ProviderAccounts && ports.settings().is_some() {
                    p { class: "mt-1 text-xs text-muted-foreground", if settings().volatile_credential { "The credential remains in memory and is cleared when this tab closes." } else { "Credentials are managed by the connected runtime." } }
                    div { class: "mt-5 grid max-w-xl gap-4 rounded-xl border border-border bg-background p-4",
                        Field { control_id: "ai-endpoint", label: "Endpoint",
                            TextInput { value: settings().endpoint, oninput: move |event: FormEvent| settings.write().endpoint = event.value() }
                        }
                        Field { control_id: "ai-model", label: "Model",
                            TextInput { value: settings().model, oninput: move |event: FormEvent| settings.write().model = event.value() }
                        }
                        Field { control_id: "ai-credential", label: "API key",
                            TextInput { input_type: TextInputType::Password, value: settings().credential, autocomplete: "off", placeholder: if settings().credential_is_set { "Credential configured" } else { "sk-…" }, oninput: move |event: FormEvent| settings.write().credential = event.value() }
                        }
                        Button { label: if saving() { "Saving…" } else { "Save settings" }, kind: ButtonKind::Primary, disabled: saving(), onclick: move |_| save() }
                    }
                } else {
                    div { class: "mt-5 max-w-xl rounded-xl border border-dashed border-border p-5 text-sm text-muted-foreground",
                        "This settings surface is not available in the current runtime."
                    }
                }
            }
        }
        if let Some((message, tone)) = notice() { Toast { message, tone, on_close: move |()| notice.set(None) } }
    }
}

fn supports_section(ports: &AiPorts, section: AiSettingsSection) -> bool {
    match section {
        AiSettingsSection::General => true,
        AiSettingsSection::ProviderAccounts => {
            ports.settings().is_some() || ports.provider_auth().is_some()
        }
        AiSettingsSection::GlobalInstructions
        | AiSettingsSection::PromptTemplates
        | AiSettingsSection::Skills => ports.resources().is_some(),
        AiSettingsSection::Extensions => ports.extensions().is_some(),
    }
}

fn managed_feature(
    ports: &AiPorts,
    section: AiSettingsSection,
) -> Option<(
    syntaxis_app_contracts::PortHandle<dyn crate::AiManagedFeaturePort>,
    AiManagedFeature,
)> {
    match section {
        AiSettingsSection::General => ports
            .usage()
            .cloned()
            .map(|port| (port, AiManagedFeature::Usage)),
        AiSettingsSection::ProviderAccounts
        | AiSettingsSection::GlobalInstructions
        | AiSettingsSection::PromptTemplates
        | AiSettingsSection::Skills
        | AiSettingsSection::Extensions => None,
    }
}

#[component]
fn InstructionsPanel(workspace: WorkspaceRecord) -> Element {
    let ports = use_context::<AiPorts>();
    let Some(port) = ports.resources().cloned() else {
        return rsx! {};
    };
    let mut content = use_signal(String::new);
    let mut applied = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut notice = use_signal(|| None::<(String, Tone)>);
    let load_port = port.clone();
    let load_workspace = workspace.clone();
    let loaded = use_resource(move || {
        let port = load_port.clone();
        let workspace = load_workspace.clone();
        async move { port.load_instructions(&workspace).await }
    });
    use_effect(move || {
        if applied() {
            return;
        }
        if let Some(Ok(value)) = loaded() {
            content.set(value);
            applied.set(true);
        }
    });
    rsx! {
        div { class: "mt-5 max-w-3xl",
            p { class: "mb-3 text-xs leading-5 text-muted-foreground", "Instructions are applied to Pi conversations in this workspace." }
            match loaded() {
                None => rsx! { p { class: "text-xs text-muted-foreground", "Loading instructions…" } },
                Some(Err(problem)) => rsx! { p { class: "text-xs text-destructive", "{problem.message}" } },
                Some(Ok(_)) => rsx! {
                    textarea { class: "min-h-96 w-full resize-y rounded-xl border border-input bg-background p-4 font-mono text-xs leading-5", value: content(), disabled: saving(), oninput: move |event| content.set(event.value()) }
                    div { class: "mt-3 flex justify-end",
                        Button { label: if saving() { "Saving…" } else { "Save instructions" }, kind: ButtonKind::Primary, disabled: saving(), onclick: {
                            let port = port.clone();
                            let workspace = workspace.clone();
                            move |_| {
                                saving.set(true);
                                let port = port.clone();
                                let workspace = workspace.clone();
                                let value = content();
                                spawn(async move {
                                    match port.save_instructions(&workspace, &value).await {
                                        Ok(()) => notice.set(Some(("Instructions saved".into(), Tone::Success))),
                                        Err(problem) => notice.set(Some((problem.message, Tone::Destructive))),
                                    }
                                    saving.set(false);
                                });
                            }
                        } }
                    }
                },
            }
        }
        if let Some((message, tone)) = notice() { Toast { message, tone, on_close: move |()| notice.set(None) } }
    }
}

#[component]
fn PromptTemplatesPanel(workspace: WorkspaceRecord) -> Element {
    let ports = use_context::<AiPorts>();
    let Some(port) = ports.resources().cloned() else {
        return rsx! {};
    };
    let mut revision = use_signal(|| 0_u64);
    let mut editor = use_signal(|| None::<(Option<String>, AiPromptTemplate)>);
    let mut confirm_delete = use_signal(|| None::<AiPromptTemplate>);
    let mut notice = use_signal(|| None::<(String, Tone)>);
    let load_port = port.clone();
    let load_workspace = workspace.clone();
    let templates = use_resource(move || {
        let port = load_port.clone();
        let workspace = load_workspace.clone();
        let _ = revision();
        async move { port.prompt_templates(&workspace).await }
    });
    rsx! {
        div { class: "mt-5 max-w-3xl",
            div { class: "mb-3 flex items-center justify-between gap-3",
                p { class: "text-xs text-muted-foreground", "Reusable slash-command prompt templates." }
                Button { label: "New template", kind: ButtonKind::Primary, onclick: move |_| editor.set(Some((None, empty_prompt_template()))) }
            }
            match templates() {
                None => rsx! { p { class: "text-xs text-muted-foreground", "Loading templates…" } },
                Some(Err(problem)) => rsx! { p { class: "text-xs text-destructive", "{problem.message}" } },
                Some(Ok(items)) if items.is_empty() => rsx! { p { class: "rounded-xl border border-dashed border-border p-6 text-center text-xs text-muted-foreground", "No prompt templates yet." } },
                Some(Ok(items)) => rsx! {
                    div { class: "grid gap-2",
                        for template in items {
                            article { key: "{template.scope:?}:{template.name}", class: "flex items-center gap-3 rounded-xl border border-border bg-background p-3",
                                div { class: "min-w-0 flex-1",
                                    strong { class: "block truncate text-xs", "{template.name}" }
                                    p { class: "mt-0.5 line-clamp-2 text-[10px] text-muted-foreground", "{template.description}" }
                                    small { class: "text-[9px] text-primary", "{scope_label(template.scope)}" }
                                }
                                Button { label: "Edit", kind: ButtonKind::Ghost, onclick: {
                                    let original_name = template.name.clone();
                                    let value = template.clone();
                                    move |_| editor.set(Some((Some(original_name.clone()), value.clone())))
                                } }
                                Button { label: "Delete", kind: ButtonKind::Danger, onclick: {
                                    let value = template.clone();
                                    move |_| confirm_delete.set(Some(value.clone()))
                                } }
                            }
                        }
                    }
                },
            }
        }
        if let Some((original_name, template)) = editor() {
            PromptTemplateEditor { workspace: workspace.clone(), original_name, template, on_close: move |()| editor.set(None), on_saved: move |()| { editor.set(None); *revision.write() += 1; notice.set(Some(("Prompt template saved".into(), Tone::Success))); } }
        }
        if let Some(template) = confirm_delete() {
            Modal { title: format!("Delete {}?", template.name), description: "This removes the prompt template from its Pi resource scope.", on_close: move |()| confirm_delete.set(None),
                DialogForm { DialogActions {
                    Button { label: "Cancel", kind: ButtonKind::Ghost, onclick: move |_| confirm_delete.set(None) }
                    Button { label: "Delete", kind: ButtonKind::Danger, onclick: {
                        let port = port.clone();
                        let workspace = workspace.clone();
                        move |_| {
                            let port = port.clone();
                            let workspace = workspace.clone();
                            let template = template.clone();
                            spawn(async move {
                                match port.delete_prompt_template(&workspace, &template).await {
                                    Ok(()) => { confirm_delete.set(None); *revision.write() += 1; notice.set(Some(("Prompt template deleted".into(), Tone::Success))); }
                                    Err(problem) => notice.set(Some((problem.message, Tone::Destructive))),
                                }
                            });
                        }
                    } }
                } }
            }
        }
        if let Some((message, tone)) = notice() { Toast { message, tone, on_close: move |()| notice.set(None) } }
    }
}

#[component]
fn PromptTemplateEditor(
    workspace: WorkspaceRecord,
    original_name: Option<String>,
    template: AiPromptTemplate,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
) -> Element {
    let ports = use_context::<AiPorts>();
    let Some(port) = ports.resources().cloned() else {
        return rsx! {};
    };
    let mut name = use_signal(|| template.name.clone());
    let mut description = use_signal(|| template.description.clone());
    let mut argument_hint = use_signal(|| template.argument_hint.clone());
    let mut content = use_signal(|| template.content.clone());
    let scope = use_signal(|| template.scope);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    rsx! {
        Modal { title: if original_name.is_some() { "Edit prompt template" } else { "New prompt template" }, description: "Prompt templates are stored in Pi's project or global resource directory.", on_close: move |()| if !saving() { on_close.call(()) },
            DialogForm {
                Field { control_id: "prompt-template-name", label: "Name", error: error(), TextInput { value: name(), autofocus: true, disabled: saving(), oninput: move |event: FormEvent| { name.set(event.value()); error.set(None); } } }
                Field { control_id: "prompt-template-description", label: "Description", TextInput { value: description(), disabled: saving(), oninput: move |event: FormEvent| description.set(event.value()) } }
                Field { control_id: "prompt-template-arguments", label: "Argument hint", TextInput { value: argument_hint(), disabled: saving(), oninput: move |event: FormEvent| argument_hint.set(event.value()) } }
                ResourceScopeSelect { scope, disabled: saving() }
                Field { control_id: "prompt-template-content", label: "Content",
                    textarea { id: "prompt-template-content", class: "min-h-52 w-full resize-y rounded-lg border border-input bg-background p-3 font-mono text-xs", value: content(), disabled: saving(), oninput: move |event| content.set(event.value()) }
                }
                DialogActions {
                    Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: saving(), onclick: move |_| on_close.call(()) }
                    Button { label: if saving() { "Saving…" } else { "Save template" }, kind: ButtonKind::Primary, disabled: saving() || name().trim().is_empty() || content().trim().is_empty(), onclick: {
                        let port = port.clone();
                        let workspace = workspace.clone();
                        let original_name = original_name.clone();
                        move |_| {
                            saving.set(true);
                            let port = port.clone();
                            let workspace = workspace.clone();
                            let original_name = original_name.clone();
                            let template = AiPromptTemplate { name: name().trim().into(), description: description().trim().into(), argument_hint: argument_hint().trim().into(), content: content(), scope: scope() };
                            spawn(async move {
                                match port.save_prompt_template(&workspace, original_name.as_deref(), template).await {
                                    Ok(()) => on_saved.call(()),
                                    Err(problem) => { error.set(Some(problem.message)); saving.set(false); }
                                }
                            });
                        }
                    } }
                }
            }
        }
    }
}

fn empty_prompt_template() -> AiPromptTemplate {
    AiPromptTemplate {
        name: String::new(),
        description: String::new(),
        argument_hint: String::new(),
        content: String::new(),
        scope: AiResourceScope::Project,
    }
}

#[component]
fn SkillsPanel(workspace: WorkspaceRecord) -> Element {
    let ports = use_context::<AiPorts>();
    let Some(port) = ports.resources().cloned() else {
        return rsx! {};
    };
    let mut revision = use_signal(|| 0_u64);
    let mut editor = use_signal(|| None::<(Option<String>, AiSkill)>);
    let mut confirm_delete = use_signal(|| None::<AiSkill>);
    let mut notice = use_signal(|| None::<(String, Tone)>);
    let load_port = port.clone();
    let load_workspace = workspace.clone();
    let skills = use_resource(move || {
        let port = load_port.clone();
        let workspace = load_workspace.clone();
        let _ = revision();
        async move { port.skills(&workspace).await }
    });
    rsx! {
        div { class: "mt-5 max-w-3xl",
            div { class: "mb-3 flex items-center justify-between gap-3",
                p { class: "text-xs text-muted-foreground", "Project and global Pi skills." }
                Button { label: "New skill", kind: ButtonKind::Primary, onclick: move |_| editor.set(Some((None, empty_skill()))) }
            }
            match skills() {
                None => rsx! { p { class: "text-xs text-muted-foreground", "Loading skills…" } },
                Some(Err(problem)) => rsx! { p { class: "text-xs text-destructive", "{problem.message}" } },
                Some(Ok(items)) if items.is_empty() => rsx! { p { class: "rounded-xl border border-dashed border-border p-6 text-center text-xs text-muted-foreground", "No skills yet." } },
                Some(Ok(items)) => rsx! {
                    div { class: "grid gap-2",
                        for skill in items {
                            article { key: "{skill.scope:?}:{skill.storage_name}", class: "flex items-center gap-3 rounded-xl border border-border bg-background p-3",
                                div { class: "min-w-0 flex-1",
                                    strong { class: "block truncate text-xs", "{skill.name}" }
                                    p { class: "mt-0.5 line-clamp-2 text-[10px] text-muted-foreground", "{skill.description}" }
                                    small { class: "text-[9px] text-primary", "{scope_label(skill.scope)}" }
                                }
                                Button { label: "Edit", kind: ButtonKind::Ghost, onclick: {
                                    let original = skill.storage_name.clone();
                                    let value = skill.clone();
                                    move |_| editor.set(Some((Some(original.clone()), value.clone())))
                                } }
                                Button { label: "Delete", kind: ButtonKind::Danger, onclick: {
                                    let value = skill.clone();
                                    move |_| confirm_delete.set(Some(value.clone()))
                                } }
                            }
                        }
                    }
                },
            }
        }
        if let Some((original, skill)) = editor() {
            SkillEditor { workspace: workspace.clone(), original_storage_name: original, skill, on_close: move |()| editor.set(None), on_saved: move |()| { editor.set(None); *revision.write() += 1; notice.set(Some(("Skill saved".into(), Tone::Success))); } }
        }
        if let Some(skill) = confirm_delete() {
            Modal { title: format!("Delete {}?", skill.name), description: "This removes the skill from its Pi resource scope.", on_close: move |()| confirm_delete.set(None),
                DialogForm { DialogActions {
                    Button { label: "Cancel", kind: ButtonKind::Ghost, onclick: move |_| confirm_delete.set(None) }
                    Button { label: "Delete", kind: ButtonKind::Danger, onclick: {
                        let port = port.clone();
                        let workspace = workspace.clone();
                        move |_| {
                            let port = port.clone();
                            let workspace = workspace.clone();
                            let skill = skill.clone();
                            spawn(async move {
                                match port.delete_skill(&workspace, &skill).await {
                                    Ok(()) => { confirm_delete.set(None); *revision.write() += 1; notice.set(Some(("Skill deleted".into(), Tone::Success))); }
                                    Err(problem) => notice.set(Some((problem.message, Tone::Destructive))),
                                }
                            });
                        }
                    } }
                } }
            }
        }
        if let Some((message, tone)) = notice() { Toast { message, tone, on_close: move |()| notice.set(None) } }
    }
}

#[component]
fn SkillEditor(
    workspace: WorkspaceRecord,
    original_storage_name: Option<String>,
    skill: AiSkill,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
) -> Element {
    let ports = use_context::<AiPorts>();
    let Some(port) = ports.resources().cloned() else {
        return rsx! {};
    };
    let mut name = use_signal(|| skill.name.clone());
    let mut description = use_signal(|| skill.description.clone());
    let mut content = use_signal(|| skill.content.clone());
    let scope = use_signal(|| skill.scope);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    rsx! {
        Modal { title: if original_storage_name.is_some() { "Edit skill" } else { "New skill" }, description: "Skills provide reusable instructions and workflows to Pi.", on_close: move |()| if !saving() { on_close.call(()) },
            DialogForm {
                Field { control_id: "skill-name", label: "Name", error: error(), TextInput { value: name(), autofocus: true, disabled: saving(), oninput: move |event: FormEvent| { name.set(event.value()); error.set(None); } } }
                Field { control_id: "skill-description", label: "Description", TextInput { value: description(), disabled: saving(), oninput: move |event: FormEvent| description.set(event.value()) } }
                ResourceScopeSelect { scope, disabled: saving() }
                Field { control_id: "skill-content", label: "Instructions",
                    textarea { id: "skill-content", class: "min-h-64 w-full resize-y rounded-lg border border-input bg-background p-3 font-mono text-xs", value: content(), disabled: saving(), oninput: move |event| content.set(event.value()) }
                }
                DialogActions {
                    Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: saving(), onclick: move |_| on_close.call(()) }
                    Button { label: if saving() { "Saving…" } else { "Save skill" }, kind: ButtonKind::Primary, disabled: saving() || name().trim().is_empty() || content().trim().is_empty(), onclick: {
                        let port = port.clone();
                        let workspace = workspace.clone();
                        let original = original_storage_name.clone();
                        move |_| {
                            saving.set(true);
                            let port = port.clone();
                            let workspace = workspace.clone();
                            let original = original.clone();
                            let normalized_name = name().trim().to_owned();
                            let skill = AiSkill { name: normalized_name.clone(), description: description().trim().into(), content: content(), scope: scope(), storage_name: if skill.storage_name.is_empty() { normalized_name } else { skill.storage_name.clone() }, single_file: skill.single_file, extra_frontmatter: skill.extra_frontmatter.clone() };
                            spawn(async move {
                                match port.save_skill(&workspace, original.as_deref(), skill).await {
                                    Ok(()) => on_saved.call(()),
                                    Err(problem) => { error.set(Some(problem.message)); saving.set(false); }
                                }
                            });
                        }
                    } }
                }
            }
        }
    }
}

#[component]
fn ResourceScopeSelect(mut scope: Signal<AiResourceScope>, disabled: bool) -> Element {
    rsx! {
        Field { control_id: "resource-scope", label: "Scope",
            select { id: "resource-scope", class: "h-9 w-full rounded-lg border border-input bg-background px-3 text-xs", disabled, value: match scope() { AiResourceScope::Global => "global", AiResourceScope::Project => "project" }, onchange: move |event| scope.set(if event.value() == "global" { AiResourceScope::Global } else { AiResourceScope::Project }),
                option { value: "project", "Project (.pi)" }
                option { value: "global", "Global (~/.pi/agent)" }
            }
        }
    }
}

fn scope_label(scope: AiResourceScope) -> &'static str {
    match scope {
        AiResourceScope::Global => "Global",
        AiResourceScope::Project => "Project",
    }
}

fn empty_skill() -> AiSkill {
    AiSkill {
        name: String::new(),
        description: String::new(),
        content: String::new(),
        scope: AiResourceScope::Project,
        storage_name: String::new(),
        single_file: true,
        extra_frontmatter: String::new(),
    }
}

#[component]
fn ExtensionsPanel(workspace: WorkspaceRecord) -> Element {
    let ports = use_context::<AiPorts>();
    let Some(port) = ports.extensions().cloned() else {
        return rsx! {};
    };
    let mut query = use_signal(String::new);
    let mut offset = use_signal(|| 0_usize);
    let mut revision = use_signal(|| 0_u64);
    let mut loaded = use_signal(Vec::<AiExtension>::new);
    let mut total = use_signal(|| 0_usize);
    let mut next_offset = use_signal(|| 0_usize);
    let mut has_more = use_signal(|| false);
    let mut confirm = use_signal(|| None::<(AiExtension, AiExtensionAction)>);
    let mut pending = use_signal(|| None::<String>);
    let mut notice = use_signal(|| None::<(String, Tone)>);
    let search_port = port.clone();
    let search_workspace = workspace.clone();
    let results = use_resource(move || {
        let port = search_port.clone();
        let workspace = search_workspace.clone();
        let query = query();
        let offset = offset();
        let _ = revision();
        async move {
            if !query.is_empty() {
                dioxus_sdk_time::sleep(std::time::Duration::from_millis(250)).await;
            }
            (
                query.clone(),
                offset,
                port.search(&workspace, &query, offset).await,
            )
        }
    });
    use_effect(move || {
        let Some((resource_query, resource_offset, Ok(page))) = results() else {
            return;
        };
        if resource_query != query() || resource_offset != offset() {
            return;
        }
        let mut merged = if resource_offset == 0 {
            Vec::new()
        } else {
            loaded()
        };
        for package in page.items {
            if let Some(existing) = merged.iter_mut().find(|item| item.name == package.name) {
                *existing = package;
            } else {
                merged.push(package);
            }
        }
        merged.sort_by(|left, right| left.name.cmp(&right.name));
        loaded.set(merged);
        total.set(page.total);
        next_offset.set(page.next_offset);
        has_more.set(page.has_more);
    });
    rsx! {
        div { class: "mt-5 max-w-4xl",
            TextInput { value: query(), placeholder: "Search Pi packages…", oninput: move |event: FormEvent| { query.set(event.value()); offset.set(0); loaded.set(Vec::new()); } }
            p { class: "py-3 text-[10px] text-muted-foreground", "Showing {loaded().len()} of {total()} packages" }
            if let Some((_, _, Err(problem))) = results() {
                p { class: "mb-3 rounded-lg bg-destructive/10 p-3 text-xs text-destructive", "{problem.message}" }
            }
            if loaded().is_empty() && results().is_none() {
                p { class: "p-6 text-center text-xs text-muted-foreground", "Loading extensions…" }
            } else if loaded().is_empty() {
                p { class: "rounded-xl border border-dashed border-border p-6 text-center text-xs text-muted-foreground", "No matching extensions." }
            } else {
                div { class: "grid grid-cols-2 gap-3 max-lg:grid-cols-1",
                    for package in loaded() {
                        article { key: "{package.name}", class: "rounded-xl border border-border bg-background p-4",
                            div { class: "flex items-start gap-3",
                                div { class: "min-w-0 flex-1",
                                    strong { class: "block truncate text-xs", "{package.name}" }
                                    small { class: "text-[9px] text-muted-foreground", "v{package.version} · {package.publisher}" }
                                }
                                Button { label: if package.installed { "Uninstall" } else { "Install" }, kind: if package.installed { ButtonKind::Danger } else { ButtonKind::Primary }, disabled: pending().is_some(), onclick: {
                                    let value = package.clone();
                                    move |_| confirm.set(Some((value.clone(), if value.installed { AiExtensionAction::Uninstall } else { AiExtensionAction::Install })))
                                } }
                            }
                            p { class: "mt-2 line-clamp-3 text-[10px] leading-4 text-muted-foreground", "{package.description}" }
                            p { class: "mt-2 text-[9px] text-muted-foreground", "{package.monthly_downloads} monthly downloads" }
                        }
                    }
                }
                if has_more() {
                    div { class: "mt-4 flex justify-center",
                        Button { label: "Load more", kind: ButtonKind::Ghost, disabled: results().is_none(), onclick: move |_| offset.set(next_offset()) }
                    }
                }
            }
        }
        if let Some((package, action)) = confirm() {
            Modal { title: match action { AiExtensionAction::Install => format!("Install {}?", package.name), AiExtensionAction::Uninstall => format!("Uninstall {}?", package.name) }, description: if action == AiExtensionAction::Install { "Pi packages can execute arbitrary code with the server user's permissions. Review the source before installing." } else { "This removes the user-scoped Pi package." }, on_close: move |()| if pending().is_none() { confirm.set(None) },
                DialogForm { DialogActions {
                    Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: pending().is_some(), onclick: move |_| confirm.set(None) }
                    Button { label: if pending().is_some() { "Working…" } else if action == AiExtensionAction::Install { "Install" } else { "Uninstall" }, kind: if action == AiExtensionAction::Install { ButtonKind::Primary } else { ButtonKind::Danger }, disabled: pending().is_some(), onclick: {
                        let port = port.clone();
                        let workspace = workspace.clone();
                        let package_name = package.name.clone();
                        move |_| {
                            pending.set(Some(package_name.clone()));
                            let port = port.clone();
                            let workspace = workspace.clone();
                            let package_name = package_name.clone();
                            spawn(async move {
                                match port.manage(&workspace, &package_name, action).await {
                                    Ok(message) => { notice.set(Some((message, Tone::Success))); confirm.set(None); offset.set(0); loaded.set(Vec::new()); *revision.write() += 1; }
                                    Err(problem) => notice.set(Some((problem.message, Tone::Destructive))),
                                }
                                pending.set(None);
                            });
                        }
                    } }
                } }
            }
        }
        if let Some((message, tone)) = notice() { Toast { message, tone, on_close: move |()| notice.set(None) } }
    }
}

#[component]
fn ProviderAccountsPanel(workspace: WorkspaceRecord) -> Element {
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
                        if flow().as_ref().map(|flow| flow.id.as_str()) != Some(flow_id.as_str()) {
                            break;
                        }
                        match port.status(&workspace, &flow_id).await {
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
                        ProviderAuthPrompt { workspace: workspace.clone(), flow_id: flow.id.clone(), prompt }
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

fn apply_event(mut conversation: Signal<AiConversation>, event: &AiEvent) {
    apply_event_to_conversation(&mut conversation.write(), event);
}

fn apply_event_to_conversation(conversation: &mut AiConversation, event: &AiEvent) {
    match event {
        AiEvent::UserMessage(message) => {
            if !conversation
                .messages
                .iter()
                .any(|item| item.id == message.id)
            {
                conversation.messages.push(message.clone());
            }
        }
        AiEvent::AssistantDelta { message_id, text } => {
            if let Some(message) = conversation
                .messages
                .iter_mut()
                .find(|item| item.id == *message_id)
            {
                message.content.push_str(text);
            } else {
                conversation.messages.push(crate::AiMessage {
                    id: message_id.clone(),
                    role: AiRole::Assistant,
                    content: text.clone(),
                });
            }
        }
        AiEvent::AssistantCompleted(message) => {
            if let Some(existing) = conversation
                .messages
                .iter_mut()
                .find(|item| item.id == message.id)
            {
                *existing = message.clone();
            } else {
                conversation.messages.push(message.clone());
            }
        }
        AiEvent::Failed { .. }
        | AiEvent::ToolStarted { .. }
        | AiEvent::ToolUpdated { .. }
        | AiEvent::ToolCompleted { .. }
        | AiEvent::UsageUpdated { .. } => {}
    }
}

fn push_activity(mut activity: Signal<Vec<AiEvent>>, event: AiEvent) {
    const MAX_VISIBLE_ACTIVITY: usize = 200;
    let mut activity = activity.write();
    activity.push(event);
    if activity.len() > MAX_VISIBLE_ACTIVITY {
        let excess = activity.len() - MAX_VISIBLE_ACTIVITY;
        activity.drain(..excess);
    }
}

#[cfg(test)]
mod event_tests {
    use super::apply_event_to_conversation;
    use crate::{AiConversation, AiEvent, AiMessage, AiRole};

    #[test]
    fn streamed_deltas_are_reconciled_with_the_completed_message() {
        let mut conversation = AiConversation::default();
        apply_event_to_conversation(
            &mut conversation,
            &AiEvent::AssistantDelta {
                message_id: "assistant-1".into(),
                text: "hel".into(),
            },
        );
        apply_event_to_conversation(
            &mut conversation,
            &AiEvent::AssistantDelta {
                message_id: "assistant-1".into(),
                text: "lo".into(),
            },
        );
        apply_event_to_conversation(
            &mut conversation,
            &AiEvent::AssistantCompleted(AiMessage {
                id: "assistant-1".into(),
                role: AiRole::Assistant,
                content: "hello!".into(),
            }),
        );

        assert_eq!(conversation.messages.len(), 1);
        assert_eq!(conversation.messages[0].content, "hello!");
    }

    #[test]
    fn repeated_user_events_are_idempotent() {
        let mut conversation = AiConversation::default();
        let event = AiEvent::UserMessage(AiMessage {
            id: "user-1".into(),
            role: AiRole::User,
            content: "question".into(),
        });
        apply_event_to_conversation(&mut conversation, &event);
        apply_event_to_conversation(&mut conversation, &event);
        assert_eq!(conversation.messages.len(), 1);
    }
}
