#![allow(
    clippy::clone_on_ref_ptr,
    reason = "PortHandle is Rc in the browser and Arc in native runtimes"
)]

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use dioxus::core::Task;
use dioxus::html::HasFileData;
use dioxus::prelude::*;
use syntaxis_app_contracts::{AiSettingsSection, NavigationIntent};
use syntaxis_git::{WorktreeCreateRequest, WorktreeInfo};
use syntaxis_module_files::{
    FilesPorts, FilesUiState, SearchOptions, SearchRequest, SearchScope, render_markdown,
};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, ChatAction, ChatActionsMenu, DialogActions, DialogForm, Field,
    Icon, IconButton, Modal, TextArea, TextAreaResize, TextInput, TextInputType, Toast, Tone,
};
use syntaxis_workspace::{EntryKind, RelativePath, WorkspaceRecord};

use crate::conversation::{activity_id, consume_ai_events};
use crate::message::ConversationMessage;
use crate::provider_accounts::ProviderAccountsPanel;
use crate::session::{restore_conversation, selection_after_delete, selection_key};
use crate::{
    AiActivity, AiAdvancedSettings, AiClientEvent, AiCommand, AiConversation, AiConversationMatch,
    AiConversationSummary, AiExtension, AiExtensionAction, AiExtensionRequest, AiExtensionWidget,
    AiGeneralSetting, AiGeneralSettingKind, AiImageAttachment, AiManagedFeature, AiMessage,
    AiMessageStatus, AiModel, AiModelPreferences, AiPorts, AiPrompt, AiPromptTemplate,
    AiProviderSettings, AiResourceScope, AiRole, AiSkill, AiSkillCatalogView, AiSkillSearchResult,
};

const AI_CHAT_CSS: Asset = asset!("/assets/chat.css");
const AI_CHAT_SCRIPT: Asset = asset!("/assets/ai-chat.js");
const MAX_PROMPT_IMAGES: usize = 5;
const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_TOTAL_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_INSTRUCTIONS_BYTES: usize = 512 * 1024;

#[derive(Clone)]
struct PendingMessageEdit {
    entry_id: String,
    previous_prompt: String,
    previous_attachments: Vec<AiImageAttachment>,
}

#[component]
pub fn AiView(
    workspace: WorkspaceRecord,
    base_workspace: Option<WorkspaceRecord>,
    current_head: Option<String>,
    requested_conversation_id: ReadSignal<Option<String>>,
    #[props(default)] start_new_conversation: bool,
    on_navigate: EventHandler<NavigationIntent>,
    on_view_conversation: EventHandler<Option<String>>,
    on_stop_viewing: EventHandler<()>,
    on_activate_worktree: EventHandler<WorktreeInfo>,
) -> Element {
    let ports = use_context::<AiPorts>();
    let ai_ui = use_context::<crate::AiUiState>();
    let files = use_context::<FilesUiState>();
    let file_ports = use_context::<FilesPorts>();
    let conversation_port = ports.conversation().cloned();
    let mut conversation = use_signal(AiConversation::default);
    let active_conversation_id = use_memo(move || conversation.read().id.clone());
    let mut conversation_loading = use_signal(|| true);
    let mut conversation_load_error = use_signal(|| None::<String>);
    let mut prompt = use_signal(String::new);
    let mut pending = use_signal(|| false);
    let mut conversation_task = use_signal(|| None::<Task>);
    let mut error = use_signal(|| None::<String>);
    let mut notice = use_signal(|| None::<String>);
    let mut isolated_open = use_signal(|| false);
    let mut isolated_branch = use_signal(default_isolated_branch);
    let mut isolated_creating = use_signal(|| false);
    let mut isolated_error = use_signal(|| None::<String>);
    let mut loaded_request = use_signal(|| None::<String>);
    let mut list_refresh = use_signal(|| 0_u64);
    let mut sidebar_open = use_signal(|| true);
    let mut mobile_sidebar_open = use_signal(|| false);
    let mut search_open = use_signal(|| false);
    let mut conversation_query = use_signal(String::new);
    let mut attachments = use_signal(Vec::<AiImageAttachment>::new);
    let mut rename_target = use_signal(|| None::<AiConversationSummary>);
    let mut rename_value = use_signal(String::new);
    let mut delete_target = use_signal(|| None::<AiConversationSummary>);
    let mut session_action_busy = use_signal(|| false);
    let mut extension_busy = use_signal(|| false);
    let mut editing_message = use_signal(|| None::<PendingMessageEdit>);
    let mut compact_open = use_signal(|| false);
    let mut compact_instructions = use_signal(String::new);
    let mut viewed_image = use_signal(|| None::<AiImageAttachment>);
    let mut visible_items = use_signal(|| 150_usize);
    let mut loaded_draft_key = use_signal(String::new);
    let mut draft_loading = use_signal(|| false);
    let mut draft_revision = use_signal(|| 0_u64);
    let mut client_listener_started = use_signal(|| false);
    let speech_active = use_signal(|| false);
    let read_aloud_available = use_signal(|| false);
    let speaking_message = use_signal(|| None::<String>);
    let mut drag_active = use_signal(|| false);
    let mut touch_input = use_signal(|| false);
    let mut command_index = use_signal(|| 0_usize);
    let mut mention_index = use_signal(|| 0_usize);
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
    let search_workspace = workspace.clone();
    let search_port = conversation_port.clone();
    let conversation_search = use_resource(move || {
        let workspace = search_workspace.clone();
        let port = search_port.clone();
        let query = conversation_query().trim().to_owned();
        async move {
            if query.chars().count() < 2 {
                return Ok(Vec::new());
            }
            dioxus_sdk_time::sleep(std::time::Duration::from_millis(300)).await;
            match port {
                Some(port) => port.search(&workspace, &query).await,
                None => Ok(Vec::new()),
            }
        }
    });
    let mention_workspace = workspace.clone();
    let mention_ports = file_ports.clone();
    let file_mentions = use_resource(move || {
        let workspace = mention_workspace.clone();
        let ports = mention_ports.clone();
        let query = ai_mention_query(&prompt()).map(|mention| mention.query);
        async move {
            let Some(query) = query else {
                return Ok(Vec::new());
            };
            if query.is_empty() {
                return ports
                    .files()
                    .list(&workspace, &RelativePath::root())
                    .await
                    .map(|entries| {
                        entries
                            .into_iter()
                            .filter(|entry| entry.kind != EntryKind::Symlink)
                            .take(12)
                            .map(|entry| {
                                let mut path = entry.path.as_str().to_owned();
                                if entry.kind == EntryKind::Directory {
                                    path.push('/');
                                }
                                path
                            })
                            .collect()
                    })
                    .map_err(syntaxis_app_contracts::AppError::from);
            }
            dioxus_sdk_time::sleep(std::time::Duration::from_millis(120)).await;
            ports
                .search()
                .search(
                    &workspace,
                    SearchRequest {
                        query,
                        options: SearchOptions {
                            fuzzy: true,
                            case_sensitive: false,
                            scope: SearchScope::FileNames,
                        },
                        ignored_paths: Vec::new(),
                        show_ignored: false,
                        max_results: 12,
                    },
                )
                .await
                .map(|results| {
                    results
                        .items
                        .into_iter()
                        .map(|item| {
                            let mut path = item.entry.path.as_str().to_owned();
                            if item.entry.kind == EntryKind::Directory {
                                path.push('/');
                            }
                            path
                        })
                        .collect()
                })
        }
    });
    let model_workspace = workspace.clone();
    let model_port = ports.models().cloned();
    let models = use_resource(move || {
        let workspace = model_workspace.clone();
        let port = model_port.clone();
        let conversation_id = active_conversation_id();
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
    let requested = requested_conversation_id;
    let viewed_workspace = workspace.id.clone();
    let selection_client = ports.client().cloned();

    use_effect(move || {
        let viewed_request = requested_conversation_id();
        let conversation_id = active_conversation_id();
        let request_key = format!(
            "{}:{}",
            viewed_workspace.0,
            viewed_request.as_deref().unwrap_or_default()
        );
        if conversation_loading()
            || conversation_load_error.peek().is_some()
            || loaded_request.peek().as_deref() != Some(request_key.as_str())
        {
            return;
        }
        let active = (!conversation_id.is_empty()).then_some(conversation_id);
        if let Some(id) = active.as_ref()
            && ai_ui.selected(&viewed_workspace).as_ref() != Some(id)
        {
            // Remember synchronously before leaving this route can cancel its async tasks.
            ai_ui.remember(viewed_workspace.clone(), id.clone());
            if let Some(client) = selection_client.clone() {
                let key = selection_key(&viewed_workspace);
                let id = id.clone();
                spawn(async move {
                    let _ = client.save_state(&key, Some(&id)).await;
                });
            }
        }
        on_view_conversation.call(active.clone());
        if active.as_deref() != viewed_request.as_deref()
            && let Some(conversation_id) = active
        {
            on_navigate.call(NavigationIntent::Ai {
                workspace: viewed_workspace.clone(),
                conversation_id: Some(conversation_id),
            });
        }
    });
    use_drop(move || on_stop_viewing.call(()));

    use_effect(move || {
        let Some(text) = conversation().requested_composer_text else {
            return;
        };
        prompt.set(text);
        conversation.write().requested_composer_text = None;
    });

    let draft_workspace_id = workspace.id.0.clone();
    use_effect({
        let client = ports.client().cloned();
        move || {
            if client_listener_started() {
                return;
            }
            let Some(client) = client.clone() else {
                return;
            };
            client_listener_started.set(true);
            spawn(async move {
                match client.listen("syntaxis-ai-composer").await {
                    Ok(mut events) => loop {
                        match events.receive().await {
                            Ok(Some(event)) => apply_ai_client_event(
                                event,
                                prompt,
                                attachments,
                                error,
                                speech_active,
                                read_aloud_available,
                                speaking_message,
                            ),
                            Ok(None) => break,
                            Err(problem) => {
                                error.set(Some(problem.message));
                                break;
                            }
                        }
                    },
                    Err(problem) => error.set(Some(problem.message)),
                }
            });
        }
    });

    use_effect({
        let client = ports.client().cloned();
        move || {
            let conversation_id = conversation().id;
            if conversation_id.is_empty() {
                return;
            }
            let key = format!("syntaxis:ai-draft:{draft_workspace_id}:{conversation_id}");
            if loaded_draft_key() == key {
                return;
            }
            loaded_draft_key.set(key.clone());
            draft_loading.set(true);
            prompt.set(String::new());
            attachments.set(Vec::new());
            editing_message.set(None);
            let Some(client) = client.clone() else {
                draft_loading.set(false);
                return;
            };
            spawn(async move {
                if let Ok(Some(stored)) = client.load_state(&key).await
                    && loaded_draft_key() == key
                    && prompt.peek().is_empty()
                {
                    prompt.set(stored);
                }
                if loaded_draft_key() == key {
                    draft_loading.set(false);
                }
            });
        }
    });

    use_effect({
        let client = ports.client().cloned();
        move || {
            let value = prompt();
            let key = loaded_draft_key();
            if key.is_empty() || draft_loading() {
                return;
            }
            // The revision cancels superseded saves; it must not subscribe this effect to itself.
            let revision = draft_revision.peek().wrapping_add(1);
            draft_revision.set(revision);
            let Some(client) = client.clone() else {
                return;
            };
            spawn(async move {
                dioxus_sdk_time::sleep(std::time::Duration::from_millis(150)).await;
                if *draft_revision.peek() != revision || loaded_draft_key.peek().as_str() != key {
                    return;
                }
                let stored = (!value.is_empty()).then_some(value.as_str());
                let _ = client.save_state(&key, stored).await;
            });
        }
    });

    let restore_client = ports.client().cloned();
    use_effect(move || {
        let requested = requested();
        let request_key = format!(
            "{}:{}",
            load_workspace.id.0,
            requested.clone().unwrap_or_default(),
        );
        if loaded_request.peek().as_ref() == Some(&request_key) {
            return;
        }
        let create_new = loaded_request.peek().is_none() && start_new_conversation;
        loaded_request.set(Some(request_key.clone()));
        // Reflecting a locally opened/created chat into the URL is not another open request.
        if requested.as_deref() == Some(conversation.peek().id.as_str())
            && !*conversation_loading.peek()
        {
            return;
        }
        if let Some(task) = conversation_task.write().take() {
            task.cancel();
        }
        conversation_loading.set(true);
        pending.set(false);
        session_action_busy.set(false);
        conversation_load_error.set(None);
        let workspace = load_workspace.clone();
        let conversation_port = conversation_port.clone();
        let client = restore_client.clone();
        let remembered = ai_ui.selected(&workspace.id);
        let requested = requested.clone();
        let task = spawn(async move {
            let Some(port) = conversation_port else {
                let message = "AI conversations are unavailable in this runtime.".to_owned();
                error.set(Some(message.clone()));
                conversation_load_error.set(Some(message));
                conversation_loading.set(false);
                return;
            };
            let result = if create_new {
                port.create(&workspace).await
            } else {
                restore_conversation(
                    port.as_ref(),
                    client.as_deref(),
                    &workspace,
                    requested.as_deref(),
                    remembered,
                )
                .await
            };
            if loaded_request.peek().as_ref() != Some(&request_key) {
                return;
            }
            match result {
                Ok(value) => {
                    let running = value.running;
                    let conversation_id = value.id.clone();
                    conversation.set(value);
                    conversation_loading.set(false);
                    *list_refresh.write() += 1;
                    if running {
                        pending.set(true);
                        match port.watch(&workspace, &conversation_id).await {
                            Ok(events) => {
                                consume_ai_events(
                                    events,
                                    &conversation_id,
                                    conversation,
                                    pending,
                                    error,
                                    list_refresh,
                                )
                                .await;
                            }
                            Err(problem) => {
                                if conversation.peek().id == conversation_id {
                                    error.set(Some(problem.message));
                                    pending.set(false);
                                }
                            }
                        }
                    }
                }
                Err(problem) => {
                    error.set(Some(problem.message.clone()));
                    conversation_load_error.set(Some(problem.message));
                    conversation_loading.set(false);
                }
            }
        });
        conversation_task.set(Some(task));
    });

    let submit_workspace = workspace.clone();
    let submit_port = ports.conversation().cloned();
    let cancel_workspace = workspace.clone();
    let cancel_port = ports.conversation().cloned();
    let submit = EventHandler::new(move |delivery: crate::AiPromptDelivery| {
        let text = prompt().trim().to_owned();
        let images = attachments();
        let edit = editing_message();
        if conversation_loading()
            || (text.is_empty() && images.is_empty())
            || conversation().id.is_empty()
        {
            return;
        }
        if !images.is_empty()
            && !models().and_then(Result::ok).is_some_and(|available| {
                conversation()
                    .selected_model_id
                    .as_ref()
                    .is_some_and(|selected| {
                        available
                            .iter()
                            .any(|model| &model.id == selected && model.supports_images)
                    })
            })
        {
            error.set(Some(
                "Choose a vision-capable model to send these images.".into(),
            ));
            return;
        }
        let Some(port) = submit_port.clone() else {
            error.set(Some(
                "AI conversations are unavailable in this runtime.".into(),
            ));
            return;
        };
        if let Some(command_name) = text
            .strip_prefix('/')
            .and_then(|value| value.split_whitespace().next())
            && conversation()
                .commands
                .iter()
                .any(|command| command.name == command_name && command.interactive)
        {
            error.set(Some(format!(
                "/{command_name} requires Pi's terminal interface and cannot run here."
            )));
            return;
        }
        error.set(None);
        prompt.set(String::new());
        attachments.set(Vec::new());
        editing_message.set(None);
        let workspace = submit_workspace.clone();
        let conversation_id = conversation().id;
        if pending() {
            if !conversation().supports_queued_prompts {
                error.set(Some("Steering is unavailable with this AI runtime.".into()));
                prompt.set(text);
                attachments.set(images);
                return;
            }
            let queued_text = text.clone();
            spawn(async move {
                match port
                    .deliver(
                        &workspace,
                        &conversation_id,
                        AiPrompt {
                            text,
                            active_file_reference: None,
                            images,
                            delivery,
                        },
                    )
                    .await
                {
                    Ok(()) => {
                        conversation.write().pending_messages += 1;
                        match delivery {
                            crate::AiPromptDelivery::FollowUp => {
                                conversation.write().follow_up_queue.push(queued_text);
                            }
                            crate::AiPromptDelivery::Prompt | crate::AiPromptDelivery::Steer => {
                                conversation.write().steering_queue.push(queued_text);
                            }
                        }
                    }
                    Err(problem) => error.set(Some(problem.message)),
                }
            });
            return;
        }
        pending.set(true);
        let task = spawn(async move {
            let conversation_id = if let Some(edit) = edit {
                match port
                    .fork_at(&workspace, &conversation_id, &edit.entry_id)
                    .await
                {
                    Ok(forked) => {
                        let id = forked.id.clone();
                        conversation.set(forked);
                        *list_refresh.write() += 1;
                        id
                    }
                    Err(problem) => {
                        error.set(Some(problem.message));
                        prompt.set(text.clone());
                        attachments.set(images.clone());
                        editing_message.set(Some(edit));
                        pending.set(false);
                        return;
                    }
                }
            } else {
                conversation_id
            };
            match port
                .send(
                    &workspace,
                    &conversation_id,
                    AiPrompt {
                        text,
                        active_file_reference: None,
                        images,
                        delivery,
                    },
                )
                .await
            {
                Ok(events) => {
                    consume_ai_events(
                        events,
                        &conversation_id,
                        conversation,
                        pending,
                        error,
                        list_refresh,
                    )
                    .await;
                }
                Err(problem) => {
                    if conversation.peek().id == conversation_id {
                        error.set(Some(problem.message));
                        pending.set(false);
                    }
                }
            }
        });
        conversation_task.set(Some(task));
    });

    let available_models = models().and_then(Result::ok).unwrap_or_default();
    let accepts_images = conversation()
        .selected_model_id
        .as_ref()
        .is_some_and(|selected| {
            available_models
                .iter()
                .any(|model| &model.id == selected && model.supports_images)
        });
    let can_submit = !conversation_loading()
        && (!prompt().trim().is_empty() || !attachments().is_empty())
        && (attachments().is_empty() || accepts_images);
    let agent_name = conversation()
        .selected_model_id
        .as_ref()
        .and_then(|selected| available_models.iter().find(|model| &model.id == selected))
        .map_or_else(|| "Assistant".to_owned(), |model| model.label.clone());
    let matching_commands = matching_ai_commands(&conversation().commands, &prompt());
    let active_mention = ai_mention_query(&prompt());
    let mention_paths = file_mentions().and_then(Result::ok).unwrap_or_default();
    let selected_command = matching_commands
        .get(command_index().min(matching_commands.len().saturating_sub(1)))
        .cloned();
    let selected_mention = mention_paths
        .get(mention_index().min(mention_paths.len().saturating_sub(1)))
        .cloned();
    let mention_status = active_mention
        .as_ref()
        .and_then(|mention| match file_mentions() {
            None => Some("Searching project files…".to_owned()),
            Some(Err(_)) => Some("Project file search is unavailable.".to_owned()),
            Some(Ok(paths)) if paths.is_empty() && mention.query.is_empty() => {
                Some("No project files yet.".to_owned())
            }
            Some(Ok(paths)) if paths.is_empty() => Some("No project files match.".to_owned()),
            Some(Ok(_)) => None,
        });
    let ordered_items = ordered_conversation_items(&conversation());
    let hidden_items = ordered_items.len().saturating_sub(visible_items());
    let rendered_items = ordered_items
        .into_iter()
        .skip(hidden_items)
        .collect::<Vec<_>>();
    let worktree_disabled_reason = if !conversation().messages.is_empty()
        || !conversation().activity.is_empty()
        || pending()
    {
        Some("Workspace cannot be changed after the chat starts".to_owned())
    } else if base_workspace.is_none() {
        Some("The registered workspace is unavailable.".to_owned())
    } else if current_head.is_none() {
        Some("Checking repository state…".to_owned())
    } else if !current_head
        .as_deref()
        .is_some_and(|head| head.chars().any(|character| character != '0'))
    {
        Some("Create the repository's first commit before adding a worktree".to_owned())
    } else if files.has_dirty() {
        Some("Save or close modified files before adding a worktree".to_owned())
    } else {
        None
    };

    use_effect(move || {
        let _ = prompt();
        command_index.set(0);
        mention_index.set(0);
    });

    // Search, history rows, and browser Back/Forward all use the same loading path.
    let open_workspace_id = workspace.id.clone();
    let select_conversation = EventHandler::new(move |conversation_id: Option<String>| {
        mobile_sidebar_open.set(false);
        conversation_query.set(String::new());
        search_open.set(false);
        on_navigate.call(NavigationIntent::Ai {
            workspace: open_workspace_id.clone(),
            conversation_id,
        });
    });
    let open_conversation =
        EventHandler::new(move |id: String| select_conversation.call(Some(id)));

    rsx! {
        document::Stylesheet { href: AI_CHAT_CSS }
        document::Script { src: AI_CHAT_SCRIPT }
        section { class: if sidebar_open() { "relative grid size-full min-h-0 min-w-0 grid-cols-[258px_minmax(0,1fr)] overflow-hidden bg-card max-md:grid-cols-[minmax(0,1fr)]" } else { "relative grid size-full min-h-0 min-w-0 grid-cols-[minmax(0,1fr)] overflow-hidden bg-card" }, "aria-label": "AI assistant",
            if sidebar_open() || mobile_sidebar_open() {
                aside { class: format!("min-h-0 min-w-0 flex-col border-r border-border bg-sidebar max-md:absolute max-md:inset-y-0 max-md:left-0 max-md:z-50 max-md:w-[min(258px,88vw)] max-md:shadow-2xl {} {}", if sidebar_open() { "md:flex" } else { "md:hidden" }, if mobile_sidebar_open() { "max-md:flex" } else { "max-md:hidden" }),
                    div { class: "grid h-12 min-h-12 grid-cols-2 items-center gap-1 border-b border-border p-1.25",
                        button { class: "h-8.5 rounded-md bg-muted text-[11px] font-medium text-foreground", r#type: "button", "Chat" }
                        button {
                            class: "h-8.5 rounded-md text-[11px] text-muted-foreground hover:bg-muted/60 hover:text-foreground",
                            r#type: "button",
                            onclick: {
                                let workspace_id = workspace.id.clone();
                                move |_| on_navigate.call(NavigationIntent::AiSettings { workspace: workspace_id.clone(), section: AiSettingsSection::General })
                            },
                            "Settings"
                        }
                    }
                    div { class: "border-b border-border p-2",
                        div { class: "flex items-center gap-1",
                            div { class: "min-w-0 flex-1 [&>button]:w-full",
                                Button {
                                    label: "New chat",
                                    kind: ButtonKind::Primary,
                                    disabled: pending() || session_action_busy() || conversation_loading(),
                                    onclick: {
                                        let port = ports.conversation().cloned();
                                        let workspace = workspace.clone();
                                        move |_| {
                                            let Some(port) = port.clone() else { return; };
                                            let workspace = workspace.clone();
                                            conversation_loading.set(true);
                                            conversation_load_error.set(None);
                                            let task = spawn(async move {
                                                match port.create(&workspace).await {
                                                    Ok(created) => { conversation.set(created); mobile_sidebar_open.set(false); *list_refresh.write() += 1; }
                                                    Err(problem) => {
                                                        error.set(Some(problem.message.clone()));
                                                        conversation_load_error.set(Some(problem.message));
                                                    },
                                                }
                                                conversation_loading.set(false);
                                            });
                                            conversation_task.set(Some(task));
                                        }
                                    },
                                }
                            }
                            IconButton {
                                label: if search_open() { "Close conversation search" } else { "Search conversations" },
                                icon: AppIcon::Search,
                                pressed: search_open(),
                                onclick: move |_| {
                                    let next = !search_open();
                                    search_open.set(next);
                                    if !next { conversation_query.set(String::new()); }
                                },
                            }
                        }
                        if search_open() {
                            div { class: "mt-2 flex min-w-0 items-center gap-2 rounded-md border border-input bg-background/70 px-2 focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/35",
                                Icon { icon: AppIcon::Search, size: 14 }
                                input {
                                    class: "h-8 min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-muted-foreground",
                                    r#type: "search",
                                    value: conversation_query(),
                                    placeholder: "Search conversations…",
                                    aria_label: "Search conversations",
                                    maxlength: 200,
                                    autofocus: true,
                                    oninput: move |event| conversation_query.set(event.value()),
                                    onkeydown: move |event| if event.key() == Key::Escape { conversation_query.set(String::new()); search_open.set(false); },
                                }
                                if !conversation_query().is_empty() {
                                    IconButton { label: "Clear conversation search", icon: AppIcon::Close, onclick: move |_| conversation_query.set(String::new()) }
                                }
                            }
                        }
                    }
                    nav { class: "min-h-0 flex-1 overflow-y-auto p-2", aria_label: "AI conversations",
                        if conversation_query().trim().chars().count() >= 2 {
                            p { class: "mb-1 px-2 py-1 text-[9px] font-semibold tracking-wider text-muted-foreground uppercase", "Message matches" }
                            match conversation_search() {
                                None => rsx! { p { class: "px-3 py-8 text-center text-[11px] text-muted-foreground", "Searching…" } },
                                Some(Err(problem)) => rsx! { p { class: "px-3 py-8 text-center text-[11px] text-destructive", "{problem.message}" } },
                                Some(Ok(items)) if items.is_empty() => rsx! { p { class: "px-3 py-8 text-center text-[11px] text-muted-foreground", "No conversations match." } },
                                Some(Ok(items)) => rsx! {
                                    ul { class: "space-y-1",
                                        for item in items {
                                            ConversationSearchRow {
                                                key: "{item.session_id}",
                                                item,
                                                query: conversation_query().trim().to_owned(),
                                                disabled: pending() || session_action_busy() || conversation_loading(),
                                                on_open: open_conversation,
                                            }
                                        }
                                    }
                                },
                            }
                        } else {
                            p { class: "mb-1 px-2 py-1 text-[9px] font-semibold tracking-wider text-muted-foreground uppercase", "Recent" }
                            match conversations() {
                            None => rsx! { p { class: "px-3 py-8 text-center text-[11px] text-muted-foreground", "Loading chats…" } },
                            Some(Err(problem)) => rsx! { p { class: "px-3 py-8 text-center text-[11px] text-destructive", "{problem.message}" } },
                            Some(Ok(items)) if items.is_empty() => rsx! { p { class: "px-3 py-8 text-center text-[11px] text-muted-foreground", "No chats yet" } },
                            Some(Ok(items)) => rsx! {
                                ul { class: "space-y-1",
                                    for item in items {
                                        if conversation_query().trim().is_empty() || item.title.to_lowercase().contains(&conversation_query().trim().to_lowercase()) {
                                            {
                                                let selected = conversation().id == item.id;
                                                let clone_port = ports.conversation().cloned();
                                                let clone_workspace = workspace.clone();
                                                let export_port = ports.conversation().cloned();
                                                let export_workspace = workspace.clone();
                                                let rename_item = item.clone();
                                                let delete_item = item.clone();
                                                rsx! {
                                                    ConversationRow {
                                                        key: "{item.id}",
                                                        item,
                                                        selected,
                                                        disabled: pending() || session_action_busy() || conversation_loading(),
                                                        on_open: open_conversation,
                                                        on_clone: move |item_id: String| {
                                                            let Some(port) = clone_port.clone() else { return; };
                                                            let workspace = clone_workspace.clone();
                                                            session_action_busy.set(true);
                                                            let task = spawn(async move {
                                                                match port.clone_conversation(&workspace, &item_id).await {
                                                                    Ok(cloned) => { conversation.set(cloned); mobile_sidebar_open.set(false); *list_refresh.write() += 1; }
                                                                    Err(problem) => error.set(Some(problem.message)),
                                                                }
                                                                session_action_busy.set(false);
                                                            });
                                                            conversation_task.set(Some(task));
                                                        },
                                                        on_export: move |item_id: String| {
                                                            let Some(port) = export_port.clone() else { return; };
                                                            let workspace = export_workspace.clone();
                                                            session_action_busy.set(true);
                                                            spawn(async move {
                                                                match port.export_conversation(&workspace, &item_id).await {
                                                                    Ok(()) => {}
                                                                    Err(problem) => error.set(Some(problem.message)),
                                                                }
                                                                session_action_busy.set(false);
                                                            });
                                                        },
                                                        on_rename: move |()| { rename_value.set(rename_item.title.clone()); rename_target.set(Some(rename_item.clone())); },
                                                        on_delete: move |()| delete_target.set(Some(delete_item.clone())),
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            }
                        }
                    }
                }
            }
            if mobile_sidebar_open() {
                button {
                    class: "absolute inset-0 z-40 hidden bg-black/45 max-md:block",
                    r#type: "button",
                    aria_label: "Close AI sidebar",
                    onclick: move |_| mobile_sidebar_open.set(false),
                }
            }
            div {
                class: "relative flex min-h-0 min-w-0 flex-col bg-card",
                ondragover: move |event: DragEvent| {
                    event.prevent_default();
                    if accepts_images { drag_active.set(true); }
                },
                ondragleave: move |_| drag_active.set(false),
                ondrop: move |event: DragEvent| {
                    event.prevent_default();
                    drag_active.set(false);
                    if accepts_images {
                        spawn(load_ai_images(event.files(), attachments, error));
                    }
                },
                header { class: "flex min-h-12 items-center gap-2 border-b border-border bg-background px-2.5",
                    div { class: "max-md:hidden", IconButton { label: if sidebar_open() { "Hide AI sidebar" } else { "Show AI sidebar" }, icon: AppIcon::Explorer, pressed: sidebar_open(), onclick: move |_| sidebar_open.toggle() } }
                    div { class: "hidden max-md:block", IconButton { label: if mobile_sidebar_open() { "Hide AI sidebar" } else { "Show AI sidebar" }, icon: AppIcon::Explorer, pressed: mobile_sidebar_open(), onclick: move |_| mobile_sidebar_open.toggle() } }
                    span { class: ai_header_status_dot(pending(), &conversation().status_message), aria_hidden: true }
                    strong { class: "min-w-0 flex-1 truncate text-xs", if let Some(title) = conversation().extension_title { "{title}" } else if conversation().title.is_empty() { "New chat" } else { "{conversation().title}" } }
                    if ports.worktrees().is_some() {
                        details { class: "group relative min-w-0",
                            summary { class: "flex h-8 max-w-44 cursor-pointer list-none items-center gap-1.5 rounded-lg px-2 text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground",
                                Icon { icon: AppIcon::Worktree, size: 13 }
                                span { class: "truncate max-[520px]:hidden", "Current checkout" }
                                Icon { icon: AppIcon::ChevronDown, size: 11 }
                            }
                            div { class: "absolute top-[calc(100%+6px)] left-0 z-80 w-52 rounded-xl border border-border bg-popover p-1.5 shadow-2xl",
                                p { class: "px-2 py-1.5 text-[9px] font-semibold tracking-wider text-muted-foreground uppercase", "Workspace" }
                                button { class: "flex min-h-9 w-full items-center gap-2 rounded-lg bg-accent/60 px-2.5 text-left text-xs", disabled: true,
                                    Icon { icon: AppIcon::Check, size: 13 }
                                    span { class: "min-w-0 flex-1", strong { class: "block truncate font-medium", "Current checkout" } small { class: "block truncate text-[9px] text-muted-foreground", "{workspace.name}" } }
                                }
                                button { class: "mt-1 flex min-h-9 w-full items-center gap-2 rounded-lg px-2.5 text-left text-xs text-muted-foreground hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40", r#type: "button",
                                    disabled: pending() || worktree_disabled_reason.is_some(),
                                    title: worktree_disabled_reason.clone().unwrap_or_else(|| "Create an isolated worktree and conversation".to_owned()),
                                    onclick: move |_| { isolated_branch.set(default_isolated_branch()); isolated_error.set(None); isolated_open.set(true); },
                                    Icon { icon: AppIcon::Worktree, size: 13 }
                                    "New worktree"
                                }
                                if let Some(reason) = worktree_disabled_reason.as_deref() { p { class: "px-2.5 py-1.5 text-[9px] leading-relaxed text-muted-foreground", "{reason}" } }
                            }
                        }
                    }
                    if !available_models.is_empty() {
                        AiModelPicker {
                            workspace: workspace.clone(),
                            models: available_models,
                            conversation,
                            disabled: pending(),
                            error,
                        }
                    }
                    crate::usage::UsageMenu {
                        stats: conversation().usage,
                        statuses: conversation().extension_statuses,
                        compact_supported: ports.conversation().is_some_and(|port| port.supports_compaction()),
                        compact_disabled: pending() || conversation().messages.is_empty(),
                        on_compact: move |()| {
                            compact_instructions.set(String::new());
                            compact_open.set(true);
                        },
                    }
                }
                if !pending()
                    && (conversation().status_message.to_ascii_lowercase().contains("fail")
                        || conversation().status_message.to_ascii_lowercase().contains("error")
                        || conversation().status_message.to_ascii_lowercase().contains("stopped"))
                {
                    div { class: "border-b border-destructive/25 bg-destructive/8 px-3 py-2 text-center text-[11px] text-destructive", role: "alert",
                        "{conversation().status_message}"
                    }
                }
                div { class: "min-h-0 flex-1 overflow-y-auto overscroll-contain p-4 [scrollbar-gutter:stable] max-md:px-2.5", "data-agent-scroll": true, role: "log", aria_live: "polite",
                    div { class: "mx-auto min-h-full max-w-3xl",
                        if conversation_loading() {
                            div { class: "mx-auto flex min-h-full w-full max-w-2xl flex-col items-center justify-center gap-3 px-3 py-8 text-center text-sm text-muted-foreground", role: "status",
                                span { class: "size-6 animate-spin rounded-full border-2 border-border border-t-primary", aria_hidden: true }
                                "Loading conversation…"
                            }
                        } else if let Some(problem) = conversation_load_error() {
                            div { class: "mx-auto flex min-h-full w-full max-w-2xl flex-col items-center justify-center px-3 py-8 text-center",
                                h2 { class: "text-lg font-semibold tracking-tight text-foreground", "Conversation unavailable" }
                                p { class: "mt-1.5 max-w-sm text-xs leading-relaxed text-muted-foreground", "{problem}" }
                                p { class: "mt-1 text-[10px] text-muted-foreground", "Choose another chat or start a new one." }
                            }
                        } else if conversation().messages.is_empty() && conversation().activity.is_empty() {
                            div { class: "mx-auto flex min-h-full w-full max-w-2xl flex-col items-center justify-center px-3 py-8 text-center",
                                h2 { class: "text-lg font-semibold tracking-tight text-foreground", "What should we work on?" }
                                div { class: "mt-5 grid w-full max-w-md gap-2 sm:grid-cols-3",
                                    for suggestion in ["Explain this project", "Find and fix a bug", "Run tests and resolve failures"] {
                                        button { class: "min-h-15 rounded-lg border border-border bg-background px-3 py-2 text-left text-[11px] leading-snug text-muted-foreground transition-colors hover:border-primary/40 hover:bg-accent hover:text-foreground", r#type: "button", onclick: {
                                            let client = ports.client().cloned();
                                            move |_| {
                                                prompt.set(suggestion.into());
                                                if let Some(client) = client.clone() {
                                                    spawn(async move { let _ = client.focus("syntaxis-ai-composer").await; });
                                                }
                                            }
                                        }, "{suggestion}" }
                                    }
                                }
                            }
                        }
                        if !conversation_loading() {
                            if hidden_items > 0 {
                                div { class: "mb-3 flex justify-center",
                                    Button { label: format!("Load {hidden_items} earlier items"), kind: ButtonKind::Ghost, onclick: move |_| *visible_items.write() += 100 }
                                }
                            }
                            for item in rendered_items {
                                match item {
                                    OrderedConversationItem::Message(message) => {
                                        let message_id = message.id.clone();
                                        let speaking = speaking_message().as_deref() == Some(message_id.as_str());
                                        rsx! { ConversationMessage {
                                        key: "{message_id}",
                                        can_edit: !pending(),
                                        read_aloud_available: read_aloud_available(),
                                        speaking,
                                        message,
                                        on_image: move |image| viewed_image.set(Some(image)),
                                        on_edit: {
                                            let client = ports.client().cloned();
                                            move |message: AiMessage| {
                                                let Some(entry_id) = message.entry_id else { return; };
                                                editing_message.set(Some(PendingMessageEdit {
                                                    entry_id,
                                                    previous_prompt: prompt(),
                                                    previous_attachments: attachments(),
                                                }));
                                                prompt.set(message.content);
                                                attachments.set(message.images);
                                                error.set(None);
                                                if let Some(client) = client.clone() {
                                                    spawn(async move { let _ = client.focus("syntaxis-ai-composer").await; });
                                                }
                                            }
                                        },
                                        on_copy: {
                                            let client = ports.client().cloned();
                                            move |value: String| {
                                                let Some(client) = client.clone() else { return; };
                                                spawn(async move {
                                                    match client.copy_text(&value).await {
                                                        Ok(()) => notice.set(Some("Copied to clipboard".into())),
                                                        Err(problem) => error.set(Some(problem.message)),
                                                    }
                                                });
                                            }
                                        },
                                        on_read: {
                                            let client = ports.client().cloned();
                                            move |message_id: String| {
                                                let Some(client) = client.clone() else { return; };
                                                spawn(async move {
                                                    if let Err(problem) = client.toggle_read_aloud(&message_id).await {
                                                        error.set(Some(problem.message));
                                                    }
                                                });
                                            }
                                        },
                                        } }
                                    },
                                    OrderedConversationItem::Activity(activity) => rsx! { ConversationActivity { key: "{activity_id(&activity)}", item: activity } },
                                }
                            }
                            if pending() { p { class: "text-xs text-muted-foreground", "Waiting for the provider…" } }
                        }
                    }
                }
                form { class: "bg-card px-2.5 pt-1 pb-[max(0.65rem,env(safe-area-inset-bottom))]", onsubmit: move |event| { event.prevent_default(); submit.call(if pending() { crate::AiPromptDelivery::Steer } else { crate::AiPromptDelivery::Prompt }); },
                    div { class: "relative mx-auto max-w-3xl rounded-2xl border border-input bg-card shadow-[0_8px_30px_#0002] focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/20",
                        ExtensionWidgets { widgets: conversation().extension_widgets, placement: "aboveEditor" }
                        if let Some(mention) = active_mention.clone() {
                            if !mention_paths.is_empty() || mention_status.is_some() {
                                div { class: "absolute right-0 bottom-[calc(100%+7px)] left-0 z-60 overflow-hidden rounded-xl border border-border bg-popover shadow-2xl",
                                    div { class: "flex items-center gap-2 border-b border-border px-3 py-2 text-[10px] text-muted-foreground", Icon { icon: AppIcon::Code, size: 13 } "Project files" span { class: "ml-auto", "Enter to reference" } }
                                    div { class: "max-h-[min(16rem,35dvh)] overflow-y-auto p-1.5",
                                        if let Some(status) = mention_status.clone() { p { class: "px-2.5 py-4 text-center text-[10px] text-muted-foreground", role: "status", "{status}" } }
                                        for (index, path) in mention_paths.clone().into_iter().enumerate() {
                                            button { key: "{path}", class: if index == mention_index() { "flex min-h-9 w-full items-center gap-2 rounded-lg bg-accent px-2.5 py-2 text-left text-foreground" } else { "flex min-h-9 w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left hover:bg-accent" }, r#type: "button", onclick: { let mention = mention.clone(); let path = path.clone(); let client = ports.client().cloned(); move |_| { insert_ai_file_mention(prompt, &mention, &path); if let Some(client) = client.clone() { spawn(async move { let _ = client.focus("syntaxis-ai-composer").await; }); } } },
                                                Icon { icon: AppIcon::Code, size: 13 }
                                                span { class: "truncate font-mono text-[10px]", "{path}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if !matching_commands.is_empty() {
                            div { class: "absolute right-0 bottom-[calc(100%+7px)] left-0 z-60 max-h-64 overflow-y-auto rounded-xl border border-border bg-popover p-1.5 shadow-2xl",
                                for (index, command) in matching_commands.clone().into_iter().enumerate() {
                                    {
                                        let name = command.name.clone();
                                        let selected_name = name.clone();
                                        let client = ports.client().cloned();
                                        rsx! { button { class: if index == command_index() { "flex min-h-9 w-full items-center gap-3 rounded-lg bg-accent px-2.5 py-2 text-left text-foreground" } else { "flex min-h-9 w-full items-center gap-3 rounded-lg px-2.5 py-2 text-left hover:bg-accent" }, r#type: "button", onclick: move |_| { prompt.set(format!("/{selected_name} ")); if let Some(client) = client.clone() { spawn(async move { let _ = client.focus("syntaxis-ai-composer").await; }); } },
                                            code { class: "text-[11px] font-semibold text-foreground", "/{name}" }
                                            span { class: "min-w-0 flex-1 truncate text-[10px] text-muted-foreground", "{command.description}" }
                                            if let Some(hint) = command.argument_hint { small { class: "text-[9px] text-muted-foreground", "{hint}" } }
                                        } }
                                    }
                                }
                            }
                        }
                        div { class: "overflow-hidden rounded-2xl",
                        if editing_message().is_some() {
                            div { class: "flex items-center justify-between gap-3 border-b border-border bg-secondary/45 px-3 py-2 text-[11px]",
                                span { class: "min-w-0 text-muted-foreground", strong { class: "font-medium text-foreground", "Editing message" } " · Sending will branch the conversation from here." }
                                button { class: "shrink-0 rounded-md px-2 py-1 font-medium hover:bg-accent", r#type: "button", onclick: move |_| {
                                    if let Some(edit) = editing_message() {
                                        prompt.set(edit.previous_prompt);
                                        attachments.set(edit.previous_attachments);
                                        editing_message.set(None);
                                    }
                                }, "Cancel" }
                            }
                        }
                        if !attachments().is_empty() {
                            div { class: "flex gap-2 overflow-x-auto border-b border-border/70 px-3 pt-3 pb-2",
                                for (index, image) in attachments().iter().enumerate() {
                                    div { class: "group relative size-18 shrink-0 overflow-hidden rounded-xl border border-border bg-background",
                                        img { class: "size-full object-cover", src: image.data_url(), alt: image.name.clone(), width: "72", height: "72" }
                                        button { class: "absolute top-1 right-1 grid size-7 place-items-center rounded-full bg-background/90 text-foreground shadow", r#type: "button", aria_label: "Remove {image.name}", onclick: move |_| { attachments.write().remove(index); }, Icon { icon: AppIcon::Close, size: 11 } }
                                        span { class: "absolute right-0 bottom-0 left-0 truncate bg-black/60 px-1.5 py-1 text-[8px] text-white", "{image.name}" }
                                    }
                                }
                            }
                        }
                        if !attachments().is_empty() && !accepts_images {
                            p { class: "border-b border-warning/25 bg-warning/8 px-3 py-2 text-[10px] text-warning", role: "status", "Choose a vision-capable model to send these images." }
                        }
                        if !conversation().steering_queue.is_empty() || !conversation().follow_up_queue.is_empty() {
                            div { class: "grid max-h-28 gap-1 overflow-y-auto border-b border-border/70 bg-secondary/25 px-3 py-2 text-[10px]", role: "status", aria_live: "polite",
                                for queued in conversation().steering_queue { div { class: "flex min-w-0 items-center gap-2", span { class: "shrink-0 rounded bg-primary/10 px-1.5 py-0.5 font-medium text-primary", "Next turn" } span { class: "truncate text-muted-foreground", "{queued}" } } }
                                for queued in conversation().follow_up_queue { div { class: "flex min-w-0 items-center gap-2", span { class: "shrink-0 rounded bg-secondary px-1.5 py-0.5 font-medium text-foreground", "After task" } span { class: "truncate text-muted-foreground", "{queued}" } } }
                            }
                        }
                        div { class: "ai-composer-editor",
                            textarea { id: "syntaxis-ai-composer", class: "ai-composer-input", rows: 3, value: prompt, maxlength: 0x0001_0000, disabled: conversation_loading(), placeholder: if conversation_loading() { "Loading conversation…".to_owned() } else if pending() { format!("Steer {agent_name} while it works…") } else { format!("Ask {agent_name} to change or inspect this project…") }, aria_label: "Message {agent_name}", "data-images-enabled": accepts_images, ontouchstart: move |_| touch_input.set(true), oninput: move |event| prompt.set(event.value()), onkeydown: move |event: KeyboardEvent| {
                                if editing_message().is_some() && event.key() == Key::Escape {
                                    event.prevent_default();
                                    if let Some(edit) = editing_message() {
                                        prompt.set(edit.previous_prompt);
                                        attachments.set(edit.previous_attachments);
                                        editing_message.set(None);
                                    }
                                } else if active_mention.is_some()
                                    && !mention_paths.is_empty()
                                    && matches!(event.key(), Key::ArrowDown | Key::ArrowUp)
                                {
                                    event.prevent_default();
                                    let length = mention_paths.len();
                                    if event.key() == Key::ArrowDown {
                                        mention_index.set((mention_index() + 1) % length);
                                    } else {
                                        mention_index.set((mention_index() + length - 1) % length);
                                    }
                                } else if active_mention.is_some() && event.key() == Key::Escape {
                                    event.prevent_default();
                                    if let Some(mention) = active_mention.as_ref() {
                                        prompt.set(format!("{}@", &prompt()[..mention.start]));
                                    }
                                } else if active_mention.is_some()
                                    && !mention_paths.is_empty()
                                    && matches!(event.key(), Key::Enter | Key::Tab)
                                {
                                    event.prevent_default();
                                    if let (Some(mention), Some(path)) = (active_mention.as_ref(), selected_mention.as_ref()) {
                                        insert_ai_file_mention(prompt, mention, path);
                                    }
                                } else if !matching_commands.is_empty()
                                    && matches!(event.key(), Key::ArrowDown | Key::ArrowUp)
                                {
                                    event.prevent_default();
                                    let length = matching_commands.len();
                                    if event.key() == Key::ArrowDown {
                                        command_index.set((command_index() + 1) % length);
                                    } else {
                                        command_index.set((command_index() + length - 1) % length);
                                    }
                                } else if selected_command.is_some()
                                    && matches!(event.key(), Key::Enter | Key::Tab)
                                    && !touch_input()
                                {
                                    event.prevent_default();
                                    if let Some(command) = selected_command.as_ref() {
                                        prompt.set(format!("/{} ", command.name));
                                    }
                                } else if !matching_commands.is_empty() && event.key() == Key::Escape {
                                    event.prevent_default();
                                    prompt.set(String::new());
                                } else if event.key() == Key::Enter && !event.modifiers().contains(Modifiers::SHIFT) && !touch_input() {
                                    event.prevent_default();
                                    submit.call(if pending() { crate::AiPromptDelivery::Steer } else { crate::AiPromptDelivery::Prompt });
                                }
                            } }
                        }
                        div { class: "flex min-h-10 items-center gap-2 px-2.5 pb-2",
                            label { class: if accepts_images && !conversation_loading() { "grid size-8 place-items-center rounded-lg text-muted-foreground hover:bg-accent hover:text-foreground" } else { "grid size-8 cursor-not-allowed place-items-center rounded-lg text-muted-foreground opacity-35" }, aria_label: if accepts_images { "Attach images" } else { "Selected model does not accept images" }, title: if accepts_images { "Attach images" } else { "Selected model does not accept images" },
                                input { class: "hidden", r#type: "file", accept: "image/*", multiple: true, disabled: !accepts_images || conversation_loading(), onchange: move |event: FormEvent| { spawn(load_ai_images(event.files(), attachments, error)); } }
                                Icon { icon: AppIcon::Attachment, size: 15 }
                            }
                            if let Some(active_file) = files.active_path() {
                                IconButton { label: "Reference active file", icon: AppIcon::Code, disabled: conversation_loading(), onclick: { let client = ports.client().cloned(); move |_| { append_file_reference(prompt, &active_file); if let Some(client) = client.clone() { spawn(async move { let _ = client.focus("syntaxis-ai-composer").await; }); } } } }
                            }
                            if let Some(reference) = files.active_reference().filter(|reference| files.active_path().as_ref() != Some(reference)) {
                                IconButton { label: "Reference editor location or selection", icon: AppIcon::LineNumbers, disabled: conversation_loading(), onclick: { let client = ports.client().cloned(); move |_| { append_text_reference(prompt, &reference); if let Some(client) = client.clone() { spawn(async move { let _ = client.focus("syntaxis-ai-composer").await; }); } } } }
                            }
                            if ports.client().is_some() {
                                IconButton { label: if speech_active() { "Stop dictation" } else { "Dictate message" }, icon: if speech_active() { AppIcon::Stop } else { AppIcon::Microphone }, pressed: speech_active(), onclick: {
                                    let client = ports.client().cloned();
                                    move |_| {
                                        let Some(client) = client.clone() else { return; };
                                        spawn(async move {
                                            if let Err(problem) = client.toggle_speech("syntaxis-ai-composer").await {
                                                error.set(Some(problem.message));
                                            }
                                        });
                                    }
                                } }
                            }
                            span { class: "min-w-0 flex-1 truncate text-[9px] text-muted-foreground max-[520px]:hidden", if pending() { if conversation().pending_messages > 0 { "Steer queued · {conversation().pending_messages} pending" } else { "Enter steers · Shift+Enter adds a line" } } else { "Markdown supported · Enter sends · Shift+Enter adds a line" } }
                            if pending() {
                                IconButton { label: "Cancel response", icon: AppIcon::Stop, danger: true, onclick: move |_| {
                                    let Some(port) = cancel_port.clone() else { return; };
                                    let workspace = cancel_workspace.clone();
                                    let conversation_id = conversation().id;
                                    spawn(async move { if let Err(problem) = port.cancel(&workspace, &conversation_id).await { error.set(Some(problem.message)); } });
                                } }
                                button { class: "grid size-9 place-items-center rounded-lg bg-primary text-primary-foreground disabled:opacity-40", r#type: "submit", aria_label: "Steer assistant", disabled: !conversation().supports_queued_prompts || !can_submit, Icon { icon: AppIcon::Send, size: 15 } }
                                button { class: "grid size-9 place-items-center rounded-lg border border-input bg-background text-muted-foreground disabled:opacity-40", r#type: "button", aria_label: "Send after assistant finishes", disabled: !conversation().supports_queued_prompts || !can_submit, onclick: move |_| submit.call(crate::AiPromptDelivery::FollowUp), Icon { icon: AppIcon::Next, size: 15 } }
                            } else {
                                button { class: "grid size-9 place-items-center rounded-lg bg-primary text-primary-foreground disabled:opacity-40", r#type: "submit", aria_label: "Send message", disabled: !can_submit, Icon { icon: AppIcon::Send, size: 15 } }
                            }
                        }
                        }
                        ExtensionWidgets { widgets: conversation().extension_widgets, placement: "belowEditor" }
                    }
                }
                if drag_active() {
                    div { class: "pointer-events-none absolute inset-3 z-90 grid place-items-center rounded-2xl border-2 border-dashed border-primary bg-primary/10 text-sm font-medium text-primary backdrop-blur-sm",
                        role: "status",
                        "Drop images to attach"
                    }
                }
            }
        }
        if let Some(image) = viewed_image() {
            Modal { title: image.name.clone(), content_class: "max-w-[min(72rem,calc(100vw-1.5rem))] overflow-hidden", on_close: move |()| viewed_image.set(None),
                div { class: "grid max-h-[calc(100dvh-7rem)] place-items-center px-3 pb-3",
                    img { class: "max-h-[calc(100dvh-8rem)] max-w-full rounded-lg object-contain", src: image.data_url(), alt: image.name, width: "1200", height: "900" }
                }
            }
        }
        if compact_open() {
            Modal {
                title: "Compact context",
                description: "Pi will summarize older conversation context while keeping the current task active.",
                on_close: move |()| if !pending() { compact_open.set(false); },
                DialogForm {
                    Field { control_id: "ai-compact-instructions", label: "Optional instructions",
                        TextArea { value: compact_instructions(), placeholder: "For example: preserve test failures and changed files", disabled: pending(), rows: 4, resize: TextAreaResize::Vertical, oninput: move |event: FormEvent| compact_instructions.set(event.value()) }
                    }
                    DialogActions {
                        Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: pending(), onclick: move |_| compact_open.set(false) }
                        Button { label: if pending() { "Compacting…" } else { "Compact context" }, kind: ButtonKind::Primary, disabled: pending(), onclick: {
                            let port = ports.conversation().cloned();
                            let workspace = workspace.clone();
                            move |_| {
                                let Some(port) = port.clone() else { return; };
                                let workspace = workspace.clone();
                                let conversation_id = conversation().id;
                                let instructions = compact_instructions().trim().to_owned();
                                pending.set(true);
                                let task = spawn(async move {
                                    match port.compact(&workspace, &conversation_id, (!instructions.is_empty()).then_some(instructions)).await {
                                        Ok(events) => {
                                            compact_open.set(false);
                                            consume_ai_events(events, &conversation_id, conversation, pending, error, list_refresh).await;
                                        }
                                        Err(problem) => {
                                            if conversation.peek().id == conversation_id {
                                                error.set(Some(problem.message));
                                                pending.set(false);
                                            }
                                        }
                                    }
                                });
                                conversation_task.set(Some(task));
                            }
                        } }
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
                    if let Some(reason) = worktree_disabled_reason.as_deref() { p { class: "text-xs text-warning", "{reason}" } }
                    DialogActions {
                        Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: isolated_creating(), onclick: move |_| isolated_open.set(false) }
                        Button { label: if isolated_creating() { "Creating…" } else { "Create worktree" }, kind: ButtonKind::Primary, disabled: isolated_creating() || worktree_disabled_reason.is_some() || isolated_branch().trim().is_empty(), onclick: {
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
        if let Some(target) = rename_target() {
            Modal {
                title: "Rename chat",
                description: "Use a short name that makes this conversation easy to find.",
                on_close: move |()| if !session_action_busy() { rename_target.set(None); },
                DialogForm {
                    Field { control_id: "ai-chat-name", label: "Name",
                        TextInput {
                            value: rename_value(),
                            disabled: session_action_busy(),
                            autofocus: true,
                            oninput: move |event: FormEvent| rename_value.set(event.value()),
                        }
                    }
                    DialogActions {
                        Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: session_action_busy(), onclick: move |_| rename_target.set(None) }
                        Button {
                            label: if session_action_busy() { "Renaming…" } else { "Rename" },
                            kind: ButtonKind::Primary,
                            disabled: session_action_busy() || rename_value().trim().is_empty(),
                            onclick: {
                                let port = ports.conversation().cloned();
                                let workspace = workspace.clone();
                                let target_id = target.id.clone();
                                move |_| {
                                    let Some(port) = port.clone() else { return; };
                                    let workspace = workspace.clone();
                                    let target_id = target_id.clone();
                                    let title = rename_value().trim().to_owned();
                                    session_action_busy.set(true);
                                    spawn(async move {
                                        match port.rename(&workspace, &target_id, &title).await {
                                            Ok(()) => {
                                                if conversation().id == target_id { conversation.write().title = title; }
                                                rename_target.set(None);
                                                *list_refresh.write() += 1;
                                            }
                                            Err(problem) => error.set(Some(problem.message)),
                                        }
                                        session_action_busy.set(false);
                                    });
                                }
                            },
                        }
                    }
                }
            }
        }
        if let Some(target) = delete_target() {
            Modal {
                title: format!("Delete {}?", target.title),
                description: "This permanently removes the conversation history.",
                on_close: move |()| if !session_action_busy() { delete_target.set(None); },
                DialogActions {
                    Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: session_action_busy(), onclick: move |_| delete_target.set(None) }
                    Button {
                        label: if session_action_busy() { "Deleting…" } else { "Delete chat" },
                        kind: ButtonKind::Danger,
                        disabled: session_action_busy(),
                        onclick: {
                            let port = ports.conversation().cloned();
                            let workspace = workspace.clone();
                            let target_id = target.id.clone();
                            move |_| {
                                let Some(port) = port.clone() else { return; };
                                let workspace = workspace.clone();
                                let target_id = target_id.clone();
                                session_action_busy.set(true);
                                spawn(async move {
                                    match port.delete(&workspace, &target_id).await {
                                        Ok(()) => {
                                            ai_ui.forget(&workspace.id, &target_id);
                                            if conversation.peek().id == target_id {
                                                let next = conversations.peek().as_ref()
                                                    .and_then(|result| result.as_ref().ok())
                                                    .and_then(|items| selection_after_delete(items, &target_id, &target_id));
                                                conversation_loading.set(true);
                                                conversation.set(AiConversation::default());
                                                // Reuse restoration (including reconnecting a running chat).
                                                // No neighbor means re-list, not unconditionally create.
                                                select_conversation.call(next);
                                            }
                                            delete_target.set(None);
                                            *list_refresh.write() += 1;
                                        }
                                        Err(problem) => error.set(Some(problem.message)),
                                    }
                                    session_action_busy.set(false);
                                });
                            }
                        },
                    }
                }
            }
        }
        if let Some(request) = conversation().extension_request {
            {
                let request_id = request.id.clone();
                rsx! {
                    ExtensionRequestDialog {
                        request,
                        busy: extension_busy(),
                        on_respond: {
                            let port = ports.conversation().cloned();
                            let workspace = workspace.clone();
                            move |(value, confirmed, cancelled): (Option<String>, Option<bool>, bool)| {
                                let Some(port) = port.clone() else {
                                    error.set(Some("Extension prompts are unavailable in this runtime.".into()));
                                    return;
                                };
                                let workspace = workspace.clone();
                                let conversation_id = conversation().id;
                                let request_id = request_id.clone();
                                extension_busy.set(true);
                                spawn(async move {
                                    match port
                                        .respond_to_extension(
                                            &workspace,
                                            &conversation_id,
                                            &request_id,
                                            value,
                                            confirmed,
                                            cancelled,
                                        )
                                        .await
                                    {
                                        Ok(()) => conversation.write().extension_request = None,
                                        Err(problem) => error.set(Some(problem.message)),
                                    }
                                    extension_busy.set(false);
                                });
                            }
                        },
                    }
                }
            }
        }
        if let Some(message) = error() {
            Toast { message, tone: Tone::Destructive, on_close: move |()| error.set(None) }
        }
        if let Some(message) = notice() {
            Toast { message, tone: Tone::Success, on_close: move |()| notice.set(None) }
        }
    }
}

#[component]
fn ExtensionRequestDialog(
    request: AiExtensionRequest,
    busy: bool,
    on_respond: EventHandler<(Option<String>, Option<bool>, bool)>,
) -> Element {
    let mut value = use_signal(|| request.prefill.clone().unwrap_or_default());
    let description = if request.message.is_empty() {
        "An agent extension needs your input.".to_owned()
    } else {
        request.message.clone()
    };
    rsx! {
        Modal {
            title: request.title.clone(),
            description,
            on_close: move |()| if !busy { on_respond.call((None, None, true)); },
            DialogForm {
                if request.method == "select" {
                    div { class: "grid gap-2",
                        for option in request.options.clone() {
                            Button {
                                label: option.clone(),
                                kind: ButtonKind::Secondary,
                                disabled: busy,
                                onclick: move |_| on_respond.call((Some(option.clone()), None, false)),
                            }
                        }
                    }
                } else if request.method == "confirm" {
                    DialogActions {
                        Button { label: "No", kind: ButtonKind::Ghost, disabled: busy, onclick: move |_| on_respond.call((None, Some(false), false)) }
                        Button { label: if busy { "Responding…" } else { "Yes" }, kind: ButtonKind::Primary, disabled: busy, onclick: move |_| on_respond.call((None, Some(true), false)) }
                    }
                } else {
                    TextArea {
                        value: value(),
                        placeholder: request.placeholder.clone().unwrap_or_default(),
                        aria_label: request.title.clone(),
                        disabled: busy,
                        autofocus: true,
                        resize: TextAreaResize::Vertical,
                        oninput: move |event: FormEvent| value.set(event.value()),
                    }
                    DialogActions {
                        Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: busy, onclick: move |_| on_respond.call((None, None, true)) }
                        Button { label: if busy { "Submitting…" } else { "Submit" }, kind: ButtonKind::Primary, disabled: busy || value().trim().is_empty(), onclick: move |_| on_respond.call((Some(value()), None, false)) }
                    }
                }
            }
        }
    }
}

#[component]
fn ExtensionWidgets(widgets: Vec<AiExtensionWidget>, placement: String) -> Element {
    rsx! {
        for widget in widgets.into_iter().filter(|widget| widget.placement == placement) {
            div { key: "{widget.key}", class: "mb-1 max-h-36 overflow-auto rounded-lg border border-border bg-secondary/35 px-3 py-2 font-mono text-[10px] leading-relaxed text-muted-foreground",
                for line in widget.lines { div { "{line}" } }
            }
        }
    }
}

#[component]
fn AiModelPicker(
    workspace: WorkspaceRecord,
    models: Vec<AiModel>,
    mut conversation: Signal<AiConversation>,
    disabled: bool,
    mut error: Signal<Option<String>>,
) -> Element {
    let ports = use_context::<AiPorts>();
    let mut open = use_signal(|| false);
    let mut query = use_signal(String::new);
    let mut preferences = use_signal(AiModelPreferences::default);
    let mut synced_signature = use_signal(String::new);
    let model_keys = models
        .iter()
        .map(|model| model.id.clone())
        .collect::<Vec<_>>();
    let signature = model_keys.join("\n");
    let sync_workspace = workspace.clone();
    let sync_port = ports.models().cloned();
    let selected_for_sync = conversation().selected_model_id;

    use_effect(move || {
        if signature.is_empty() || synced_signature() == signature {
            return;
        }
        synced_signature.set(signature.clone());
        let Some(port) = sync_port.clone() else {
            return;
        };
        let workspace = sync_workspace.clone();
        let available = model_keys.clone();
        let selected = selected_for_sync.clone();
        spawn(async move {
            match port.sync_preferences(&workspace, available).await {
                Ok(synced) => {
                    let restored = selected
                        .as_ref()
                        .and_then(|model_id| synced.efforts.get(model_id).copied());
                    preferences.set(synced);
                    if let (Some(_), Some(level)) = (selected, restored)
                        && conversation().thinking_level != level
                    {
                        let conversation_id = conversation().id;
                        if let Err(problem) = port
                            .select_thinking_level(&workspace, &conversation_id, level)
                            .await
                        {
                            error.set(Some(problem.message));
                        } else {
                            conversation.write().thinking_level = level;
                        }
                    }
                }
                Err(problem) => error.set(Some(problem.message)),
            }
        });
    });

    let selected = conversation()
        .selected_model_id
        .as_ref()
        .and_then(|selected| models.iter().find(|model| &model.id == selected))
        .cloned();
    let selected_name = selected
        .as_ref()
        .map_or_else(|| "Default model".to_owned(), |model| model.label.clone());
    let selected_detail = selected.as_ref().map_or_else(
        || "Agent".to_owned(),
        |model| {
            if model.reasoning {
                format!(
                    "{} · {} effort",
                    model.provider,
                    conversation().thinking_level.label()
                )
            } else {
                model.provider.clone()
            }
        },
    );
    let selected_id = selected.as_ref().map(|model| model.id.clone());
    let filtered = filter_ai_models(models, &query());
    let favourite_ids = preferences().favourites;
    let favourites = favourite_ids
        .iter()
        .filter_map(|id| filtered.iter().find(|model| model.id == *id).cloned())
        .filter(|model| Some(&model.id) != selected_id.as_ref())
        .collect::<Vec<_>>();
    let groups = group_ai_models(
        filtered
            .into_iter()
            .filter(|model| Some(&model.id) != selected_id.as_ref())
            .filter(|model| !favourite_ids.contains(&model.id))
            .collect(),
    );

    rsx! {
        div { class: "relative min-w-0",
            button {
                class: if open() { "flex h-8 min-w-0 max-w-58 items-center gap-2 rounded-lg border border-primary/30 bg-accent px-2.5 text-left shadow-sm max-[590px]:max-w-34 max-[520px]:size-10 max-[520px]:justify-center max-[520px]:p-0" } else { "flex h-8 min-w-0 max-w-58 items-center gap-2 rounded-lg border border-input bg-background/80 px-2.5 text-left shadow-xs hover:bg-accent max-[590px]:max-w-34 max-[520px]:size-10 max-[520px]:justify-center max-[520px]:p-0" },
                r#type: "button",
                disabled,
                aria_label: "Choose AI model",
                aria_expanded: open(),
                onclick: move |_| {
                    open.toggle();
                    if open() { query.set(String::new()); }
                },
                span { class: "grid size-5 shrink-0 place-items-center rounded-md bg-primary/10 text-primary", Icon { icon: AppIcon::BrainCog, size: 12 } }
                span { class: "min-w-0 flex-1 max-[520px]:hidden",
                    strong { class: "block truncate text-[11px] font-medium", "{selected_name}" }
                    small { class: "block truncate text-[9px] text-muted-foreground", "{selected_detail}" }
                }
                span { class: "max-[520px]:hidden", Icon { icon: AppIcon::ChevronDown, size: 13 } }
            }
            if open() {
                button { class: "fixed inset-0 z-70 cursor-default", r#type: "button", aria_label: "Close model picker", onclick: move |_| open.set(false) }
                div { class: "absolute top-[calc(100%+6px)] right-0 z-80 w-[min(430px,calc(100vw-1rem))] overflow-hidden rounded-xl border border-border bg-popover shadow-2xl",
                    if let Some(model) = selected.clone() {
                        div { class: "flex items-start gap-3 border-b border-border px-3 py-3",
                            div { class: "min-w-0 flex-1",
                                strong { class: "block truncate text-sm font-semibold", "{model.label}" }
                                p { class: "mt-1 text-[10px] text-muted-foreground", {format_ai_model_capabilities(&model)} }
                                p { class: "mt-1 text-[10px] text-muted-foreground", title: "Input / output catalog price per million tokens", {format_ai_model_price_long(&model)} }
                            }
                            button {
                                class: if favourite_ids.contains(&model.id) { "grid size-8 shrink-0 place-items-center rounded-md text-primary hover:bg-accent" } else { "grid size-8 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" },
                                r#type: "button",
                                aria_label: if favourite_ids.contains(&model.id) { "Remove model from favourites" } else { "Add model to favourites" },
                                onclick: {
                                    let port = ports.models().cloned();
                                    let workspace = workspace.clone();
                                    let model_id = model.id.clone();
                                    let favourite = !favourite_ids.contains(&model.id);
                                    move |_| {
                                        preferences.with_mut(|current| {
                                            current.favourites.retain(|id| id != &model_id);
                                            if favourite { current.favourites.insert(0, model_id.clone()); }
                                        });
                                        let Some(port) = port.clone() else { return; };
                                        let model_id = model_id.clone();
                                        let workspace = workspace.clone();
                                        spawn(async move {
                                            match port.set_favourite(&workspace, &model_id, favourite).await {
                                                Ok(updated) => preferences.set(updated),
                                                Err(problem) => error.set(Some(problem.message)),
                                            }
                                        });
                                    }
                                },
                                Icon { icon: if favourite_ids.contains(&model.id) { AppIcon::FavouriteFilled } else { AppIcon::Favourite }, size: 15 }
                            }
                            Icon { icon: AppIcon::Check, size: 15 }
                        }
                        if model.reasoning {
                            div { class: "border-b border-border bg-secondary/10 px-3 py-2.5",
                                div { class: "mb-2 flex items-center gap-2 text-[10px] font-medium", Icon { icon: AppIcon::BrainCog, size: 13 } "Reasoning effort" }
                                div { class: "flex flex-wrap gap-1",
                                    for level in model.thinking_levels.clone() {
                                        button {
                                            class: if conversation().thinking_level == level { "rounded-md bg-primary px-2 py-1 text-[9px] font-semibold text-primary-foreground" } else { "rounded-md border border-input bg-background px-2 py-1 text-[9px] text-muted-foreground hover:bg-accent hover:text-foreground" },
                                            r#type: "button",
                                            disabled,
                                            onclick: {
                                                let port = ports.models().cloned();
                                                let workspace = workspace.clone();
                                                let model_id = model.id.clone();
                                                move |_| {
                                                    let Some(port) = port.clone() else { return; };
                                                    let workspace = workspace.clone();
                                                    let conversation_id = conversation().id;
                                                    let model_id = model_id.clone();
                                                    spawn(async move {
                                                        match port.select_thinking_level(&workspace, &conversation_id, level).await {
                                                            Ok(()) => {
                                                                conversation.write().thinking_level = level;
                                                                match port.remember_effort(&workspace, &model_id, level).await {
                                                                    Ok(updated) => preferences.set(updated),
                                                                    Err(problem) => error.set(Some(problem.message)),
                                                                }
                                                            }
                                                            Err(problem) => error.set(Some(problem.message)),
                                                        }
                                                    });
                                                }
                                            },
                                            "{level.label()}"
                                        }
                                    }
                                }
                            }
                        } else {
                            div { class: "flex items-center gap-2 border-b border-border bg-secondary/10 px-3 py-2 text-[10px] text-muted-foreground", Icon { icon: AppIcon::BrainCog, size: 13 } "Reasoning effort is unavailable for this model." }
                        }
                    }
                    div { class: "border-b border-border p-3",
                        div { class: "flex h-9 items-center gap-2 rounded-lg border border-input bg-background px-3 focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/35",
                            Icon { icon: AppIcon::Search, size: 14 }
                            input { class: "min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-muted-foreground", r#type: "search", name: "model-search", autocomplete: "off", value: query(), placeholder: "Search models…", aria_label: "Search models", oninput: move |event| query.set(event.value()), onkeydown: move |event| if event.key() == Key::Escape { open.set(false); } }
                        }
                    }
                    div { class: "max-h-[min(360px,55vh)] overflow-y-auto p-1.5",
                        if favourites.is_empty() && groups.is_empty() { p { class: "px-3 py-8 text-center text-xs text-muted-foreground", "No matching models" } }
                        if !favourites.is_empty() {
                            p { class: "sticky top-0 z-1 flex h-8 items-center gap-2 bg-popover/95 px-2.5 text-[9px] font-semibold tracking-wide text-muted-foreground uppercase", Icon { icon: AppIcon::Favourite, size: 11 } "Favourites" }
                            for model in favourites {
                                AiModelRow { key: "{model.id}", model, favourite: true, workspace: workspace.clone(), conversation, preferences, open, error }
                            }
                        }
                        for (provider, provider_models) in groups {
                            p { key: "heading-{provider}", class: "sticky top-0 z-1 flex h-8 items-center gap-2 bg-popover/95 px-2.5 text-[9px] font-semibold tracking-wide text-muted-foreground uppercase", "{provider}" }
                            for model in provider_models {
                                AiModelRow { key: "{model.id}", model, favourite: false, workspace: workspace.clone(), conversation, preferences, open, error }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AiModelRow(
    model: AiModel,
    favourite: bool,
    workspace: WorkspaceRecord,
    mut conversation: Signal<AiConversation>,
    mut preferences: Signal<AiModelPreferences>,
    mut open: Signal<bool>,
    mut error: Signal<Option<String>>,
) -> Element {
    let ports = use_context::<AiPorts>();
    let select_model = model.clone();
    let favourite_model = model.clone();
    rsx! {
        div { class: "flex min-h-12 w-full items-center rounded-lg text-xs text-muted-foreground hover:bg-accent hover:text-foreground",
            button { class: "grid min-h-12 min-w-0 flex-1 grid-cols-[minmax(0,1fr)_7rem] items-center gap-2 px-2.5 py-1.5 text-left", r#type: "button", onclick: {
                let port = ports.models().cloned();
                let workspace = workspace.clone();
                move |_| {
                    let Some(port) = port.clone() else { return; };
                    let workspace = workspace.clone();
                    let model = select_model.clone();
                    let conversation_id = conversation().id;
                    let remembered = preferences().efforts.get(&model.id).copied();
                    spawn(async move {
                        match port.select_model(&workspace, &conversation_id, &model.id).await {
                            Ok(()) => {
                                conversation.write().selected_model_id = Some(model.id.clone());
                                if let Some(level) = remembered {
                                    match port.select_thinking_level(&workspace, &conversation_id, level).await {
                                        Ok(()) => conversation.write().thinking_level = level,
                                        Err(problem) => error.set(Some(problem.message)),
                                    }
                                }
                                open.set(false);
                            }
                            Err(problem) => error.set(Some(problem.message)),
                        }
                    });
                }
            },
                span { class: "min-w-0", strong { class: "block truncate font-medium", "{model.label}" } small { class: "block truncate text-[9px] text-muted-foreground", {format_ai_model_capabilities(&model)} } }
                strong { class: if model.cost.is_free() { "block text-right text-[9px] font-medium text-success" } else { "block text-right text-[9px] font-medium text-muted-foreground" }, title: "Input / output catalog price per million tokens", {format_ai_model_price(&model)} }
            }
            button { class: if favourite { "mr-1 grid size-9 shrink-0 place-items-center rounded-md text-primary hover:bg-background/70" } else { "mr-1 grid size-9 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-background/70 hover:text-foreground" }, r#type: "button", aria_label: if favourite { "Remove {model.label} from favourites" } else { "Add {model.label} to favourites" }, onclick: {
                let port = ports.models().cloned();
                let workspace = workspace.clone();
                move |_| {
                    let Some(port) = port.clone() else { return; };
                    let workspace = workspace.clone();
                    let model_id = favourite_model.id.clone();
                    preferences.with_mut(|current| {
                        current.favourites.retain(|id| id != &model_id);
                        if !favourite { current.favourites.insert(0, model_id.clone()); }
                    });
                    spawn(async move {
                        match port.set_favourite(&workspace, &model_id, !favourite).await {
                            Ok(updated) => preferences.set(updated),
                            Err(problem) => error.set(Some(problem.message)),
                        }
                    });
                }
            }, Icon { icon: if favourite { AppIcon::FavouriteFilled } else { AppIcon::Favourite }, size: 14 } }
        }
    }
}

fn filter_ai_models(mut models: Vec<AiModel>, query: &str) -> Vec<AiModel> {
    let query = query.trim().to_ascii_lowercase();
    let free_query = query == "free";
    models.retain(|model| {
        let searchable =
            format!("{} {} {}", model.provider, model.label, model.id).to_ascii_lowercase();
        query.is_empty() || searchable.contains(&query) || (free_query && model.cost.is_free())
    });
    models.sort_by_key(|model| (model.provider.to_lowercase(), model.label.to_lowercase()));
    models
}

fn group_ai_models(models: Vec<AiModel>) -> Vec<(String, Vec<AiModel>)> {
    let mut groups = Vec::<(String, Vec<AiModel>)>::new();
    for model in models {
        if let Some((_, provider_models)) = groups
            .last_mut()
            .filter(|(provider, _)| provider == &model.provider)
        {
            provider_models.push(model);
        } else {
            groups.push((model.provider.clone(), vec![model]));
        }
    }
    groups
}

fn format_ai_model_capabilities(model: &AiModel) -> String {
    let mut details = Vec::new();
    if model.context_window > 0 {
        details.push(format_context_window(model.context_window));
    }
    if model.reasoning {
        details.push("Reasoning".to_owned());
    }
    if model.supports_images {
        details.push("Vision".to_owned());
    }
    if details.is_empty() {
        model.provider.clone()
    } else {
        details.join(" · ")
    }
}

fn format_ai_model_price(model: &AiModel) -> String {
    if model.cost.is_free() {
        return "Free".to_owned();
    }
    let tiers = if model.cost.has_paid_tier { "+" } else { "" };
    format!(
        "{} in · {} out{tiers}",
        format_model_rate(model.cost.input),
        format_model_rate(model.cost.output)
    )
}

fn format_ai_model_price_long(model: &AiModel) -> String {
    if model.cost.is_free() {
        return "Free".to_owned();
    }
    let tiers = if model.cost.has_paid_tier { "+" } else { "" };
    format!(
        "{} input · {} output{tiers} / 1M tokens",
        format_model_rate(model.cost.input),
        format_model_rate(model.cost.output)
    )
}

fn format_model_rate(microusd: u64) -> String {
    let mut rate = format!(
        "${}.{:04}",
        microusd / 1_000_000,
        (microusd % 1_000_000) / 100
    );
    while rate.ends_with('0') {
        rate.pop();
    }
    if rate.ends_with('.') {
        rate.pop();
    }
    rate
}

fn format_context_window(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        let whole = tokens / 1_000_000;
        let tenth = (tokens % 1_000_000) / 100_000;
        if tenth == 0 {
            format!("{whole}M context")
        } else {
            format!("{whole}.{tenth}M context")
        }
    } else {
        format!("{}K context", tokens / 1_000)
    }
}

enum OrderedConversationItem {
    Message(AiMessage),
    Activity(AiActivity),
}

fn ordered_conversation_items(conversation: &AiConversation) -> Vec<OrderedConversationItem> {
    let mut items = Vec::with_capacity(conversation.messages.len() + conversation.activity.len());
    for id in &conversation.item_order {
        if let Some(message) = conversation
            .messages
            .iter()
            .find(|message| message.id == *id)
        {
            items.push(OrderedConversationItem::Message(message.clone()));
        } else if let Some(activity) = conversation
            .activity
            .iter()
            .find(|activity| activity_id(activity) == id)
        {
            items.push(OrderedConversationItem::Activity(activity.clone()));
        }
    }
    for message in &conversation.messages {
        if !conversation.item_order.contains(&message.id) {
            items.push(OrderedConversationItem::Message(message.clone()));
        }
    }
    for activity in &conversation.activity {
        if !conversation
            .item_order
            .iter()
            .any(|id| id == activity_id(activity))
        {
            items.push(OrderedConversationItem::Activity(activity.clone()));
        }
    }
    items
}

#[component]
fn ConversationActivity(item: AiActivity) -> Element {
    match item {
        AiActivity::Tool {
            name,
            summary,
            output,
            args,
            details,
            args_truncated,
            details_truncated,
            status,
            ..
        } => {
            let tone = match status {
                AiMessageStatus::Failed => "text-destructive",
                AiMessageStatus::Running | AiMessageStatus::Streaming => "text-primary",
                AiMessageStatus::Complete | AiMessageStatus::Stopped => "text-success",
            };
            let rendered = matches!(status, AiMessageStatus::Complete | AiMessageStatus::Stopped)
                .then(|| render_markdown(&output));
            let line_changes = tool_line_changes(&output);
            rsx! {
                details { class: "mb-2 rounded-lg border border-border bg-background/65 text-[11px]",
                    summary { class: "flex min-h-9 cursor-pointer list-none items-center gap-2 px-3 py-2 select-none",
                        span { class: "size-2 shrink-0 rounded-full bg-current {tone}" }
                        strong { class: "font-mono text-[10px] font-semibold text-foreground", "{name}" }
                        span { class: "min-w-0 flex-1 truncate text-muted-foreground", "{summary}" }
                        if let Some((added, removed)) = line_changes {
                            span { class: "shrink-0 font-mono text-[9px]",
                                if added > 0 { span { class: "text-success", "+{added}" } }
                                if removed > 0 { span { class: "ml-1 text-destructive", "-{removed}" } }
                            }
                        }
                        small { class: "shrink-0 text-[9px] capitalize text-muted-foreground", "{status:?}" }
                    }
                    if !output.is_empty() {
                        div { class: "max-h-80 overflow-auto border-t border-border bg-background px-3 py-2 text-[10px] leading-relaxed text-muted-foreground",
                            if let Some(rendered) = rendered {
                                div { class: "ai-markdown ai-tool-markdown", dangerous_inner_html: rendered }
                            } else {
                                pre { class: "font-mono whitespace-pre-wrap", "{output}" }
                            }
                        }
                    }
                    if let Some(args) = args { details { class: "border-t border-border px-3 py-2", summary { class: "cursor-pointer font-medium text-foreground", if args_truncated { "Arguments (truncated)" } else { "Arguments" } } pre { class: "mt-2 max-h-60 overflow-auto font-mono text-[10px] whitespace-pre-wrap text-muted-foreground", "{args}" } } }
                    if let Some(details) = details { details { class: "border-t border-border px-3 py-2", summary { class: "cursor-pointer font-medium text-foreground", if details_truncated { "Details (truncated)" } else { "Details" } } pre { class: "mt-2 max-h-60 overflow-auto font-mono text-[10px] whitespace-pre-wrap text-muted-foreground", "{details}" } } }
                }
            }
        }
        AiActivity::Custom {
            label,
            text,
            details,
            details_truncated,
            ..
        } => {
            let rendered = render_markdown(&text);
            rsx! {
                article { class: "mb-2 rounded-lg border border-primary/20 bg-primary/5 px-3 py-2.5 text-[11px]",
                    header { class: "mb-1 font-mono text-[9px] font-semibold tracking-wide text-primary uppercase", "{label}" }
                    if !text.is_empty() { div { class: "ai-markdown", dangerous_inner_html: rendered } }
                    if let Some(details) = details {
                        details { class: "mt-2 border-t border-border pt-2",
                            summary { class: "cursor-pointer text-muted-foreground", if details_truncated { "Details (truncated)" } else { "Details" } }
                            pre { class: "mt-2 max-h-72 overflow-auto font-mono text-[10px] whitespace-pre-wrap text-muted-foreground", "{details}" }
                        }
                    }
                }
            }
        }
        AiActivity::Notice { text, status, .. } => rsx! {
            p { class: if status == AiMessageStatus::Failed { "mb-2 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-[11px] text-destructive" } else { "mb-2 rounded-lg border border-border bg-muted/30 px-3 py-2 text-[11px] text-muted-foreground" }, "{text}" }
        },
    }
}

fn tool_line_changes(output: &str) -> Option<(usize, usize)> {
    let mut added = 0;
    let mut removed = 0;
    for line in output.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            added += 1;
        } else if line.starts_with('-') {
            removed += 1;
        }
    }
    (added > 0 || removed > 0).then_some((added, removed))
}

#[component]
fn ConversationSearchRow(
    item: AiConversationMatch,
    query: String,
    disabled: bool,
    on_open: EventHandler<String>,
) -> Element {
    let session_id = item.session_id.clone();
    let role = match item.role {
        AiRole::User => "You",
        AiRole::Assistant => "Agent",
        AiRole::System => "System",
    };
    rsx! {
        li {
            button { class: "w-full rounded-lg border border-transparent px-2.5 py-2.5 text-left hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50", r#type: "button", disabled, onclick: move |_| on_open.call(session_id.clone()),
                strong { class: "block truncate text-[11px] font-medium", "{item.title}" }
                p { class: "mt-1 line-clamp-2 text-[10px] leading-relaxed text-muted-foreground",
                    span { class: "font-medium text-foreground/80", "{role}: " }
                    HighlightedConversationSnippet { text: item.snippet, query }
                }
                div { class: "mt-1 flex items-center gap-2 text-[9px] text-muted-foreground",
                    span { class: "min-w-0 flex-1", if item.match_count == 1 { "1 match" } else { "{item.match_count} matches" } }
                    if item.updated_at_ms > 0 { time { class: "shrink-0", "{conversation_age(item.updated_at_ms)}" } }
                }
            }
        }
    }
}

#[component]
fn HighlightedConversationSnippet(text: String, query: String) -> Element {
    let parts = highlighted_parts(&text, &query);
    rsx! {
        for (index, (part, matched)) in parts.into_iter().enumerate() {
            if matched {
                mark { key: "search-match-{index}", class: "rounded-sm bg-warning/25 px-0.5 text-foreground", "{part}" }
            } else {
                span { key: "search-text-{index}", "{part}" }
            }
        }
    }
}

fn highlighted_parts(text: &str, query: &str) -> Vec<(String, bool)> {
    let Ok(pattern) = regex::RegexBuilder::new(&regex::escape(query))
        .case_insensitive(true)
        .build()
    else {
        return vec![(text.to_owned(), false)];
    };
    let mut parts = Vec::new();
    let mut previous = 0;
    for found in pattern.find_iter(text) {
        if found.start() > previous {
            parts.push((text[previous..found.start()].to_owned(), false));
        }
        parts.push((found.as_str().to_owned(), true));
        previous = found.end();
    }
    if previous < text.len() {
        parts.push((text[previous..].to_owned(), false));
    }
    if parts.is_empty() {
        parts.push((text.to_owned(), false));
    }
    parts
}

#[component]
fn ConversationRow(
    item: AiConversationSummary,
    selected: bool,
    disabled: bool,
    on_open: EventHandler<String>,
    on_clone: EventHandler<String>,
    on_export: EventHandler<String>,
    on_rename: EventHandler<()>,
    on_delete: EventHandler<()>,
) -> Element {
    let open_id = item.id.clone();
    let clone_id = item.id.clone();
    let export_id = item.id.clone();
    rsx! {
        li { "data-conversation-id": item.id.clone(), class: if selected { "group relative flex items-stretch rounded-lg border border-primary/25 bg-primary/10" } else { "group relative flex items-stretch rounded-lg border border-transparent hover:bg-accent" },
            button {
                class: "min-w-0 flex-1 px-2.5 py-2.5 text-left",
                r#type: "button",
                disabled,
                aria_current: selected.then_some("page"),
                onclick: move |_| on_open.call(open_id.clone()),
                div { class: "flex items-center gap-2",
                    span { class: if item.running { "size-1.5 shrink-0 animate-pulse rounded-full bg-primary" } else if selected { "size-1.5 shrink-0 rounded-full bg-success" } else { "size-1.5 shrink-0 rounded-full bg-muted-foreground/50" } }
                    strong { class: "min-w-0 flex-1 truncate text-[11px] font-medium", "{item.title}" }
                }
                div { class: "mt-1 flex items-center gap-2 pl-3.5 text-[9px] text-muted-foreground",
                    span { class: "min-w-0 flex-1 truncate", if item.status_message.is_empty() { "{item.message_count} messages" } else { "{item.status_message}" } }
                    if item.updated_at_ms > 0 { time { class: "shrink-0", "{conversation_age(item.updated_at_ms)}" } }
                }
            }
            ChatActionsMenu {
                title: item.title,
                disabled,
                running: item.running,
                on_action: move |action| match action {
                    ChatAction::Clone => on_clone.call(clone_id.clone()),
                    ChatAction::Export => on_export.call(export_id.clone()),
                    ChatAction::Rename => on_rename.call(()),
                    ChatAction::Delete => on_delete.call(()),
                },
            }
        }
    }
}

fn default_isolated_branch() -> String {
    let milliseconds = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    format!("agent/chat-{milliseconds}")
}

fn conversation_age(timestamp: u64) -> String {
    let now = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(timestamp);
    let minutes = now.saturating_sub(timestamp) / 60_000;
    match minutes {
        0 => "now".into(),
        1..=59 => format!("{minutes}m"),
        60..=1_439 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1_440),
    }
}

fn ai_header_status_dot(pending: bool, status: &str) -> &'static str {
    let status = status.to_ascii_lowercase();
    if pending {
        "size-1.5 shrink-0 animate-pulse rounded-full bg-primary"
    } else if status.contains("fail") || status.contains("error") {
        "size-1.5 shrink-0 rounded-full bg-destructive"
    } else if status.contains("stopped") {
        "size-1.5 shrink-0 rounded-full bg-muted-foreground/50"
    } else {
        "size-1.5 shrink-0 rounded-full bg-success"
    }
}

#[derive(Clone)]
struct AiFileMention {
    start: usize,
    query: String,
}

fn ai_mention_query(text: &str) -> Option<AiFileMention> {
    let start = text.rfind(char::is_whitespace).map_or(0, |index| index + 1);
    text[start..].strip_prefix('@').map(|query| AiFileMention {
        start,
        query: query.to_owned(),
    })
}

fn insert_ai_file_mention(mut prompt: Signal<String>, mention: &AiFileMention, path: &str) {
    let current = prompt();
    let mut value = current[..mention.start].to_owned();
    value.push('@');
    if path.chars().any(char::is_whitespace) {
        value.push('"');
        value.push_str(path);
        value.push('"');
    } else {
        value.push_str(path);
    }
    if !path.ends_with('/') {
        value.push(' ');
    }
    prompt.set(value);
}

fn append_file_reference(mut prompt: Signal<String>, path: &str) {
    let mut value = prompt();
    if !value.is_empty() && !value.ends_with(char::is_whitespace) {
        value.push(' ');
    }
    value.push('@');
    if path.chars().any(char::is_whitespace) {
        value.push('"');
        value.push_str(path);
        value.push('"');
    } else {
        value.push_str(path);
    }
    value.push(' ');
    prompt.set(value);
}

fn append_text_reference(mut prompt: Signal<String>, reference: &str) {
    let mut value = prompt();
    if !value.is_empty() && !value.ends_with(char::is_whitespace) {
        value.push(' ');
    }
    value.push_str(reference);
    value.push(' ');
    prompt.set(value);
}

fn matching_ai_commands(commands: &[AiCommand], prompt: &str) -> Vec<AiCommand> {
    let Some(query) = prompt.strip_prefix('/') else {
        return Vec::new();
    };
    if query.chars().any(char::is_whitespace) {
        return Vec::new();
    }
    let query = query.to_lowercase();
    commands
        .iter()
        .filter(|command| command.name.to_lowercase().contains(&query))
        .take(12)
        .cloned()
        .collect()
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
    let has_managed = managed.is_some();
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
        document::Stylesheet { href: AI_CHAT_CSS }
        section { class: "flex size-full min-h-0 bg-card max-md:flex-col", "aria-label": "AI settings",
            nav { class: "w-64 shrink-0 border-r border-border bg-sidebar max-md:w-full max-md:border-r-0 max-md:border-b",
                div { class: "grid h-12 min-h-12 grid-cols-2 items-center gap-1 border-b border-border p-1.25",
                    button {
                        class: "h-8.5 rounded-md text-[11px] text-muted-foreground hover:bg-muted/60 hover:text-foreground",
                        r#type: "button",
                        onclick: {
                            let workspace_id = workspace.id.clone();
                            move |_| on_navigate.call(NavigationIntent::Ai { workspace: workspace_id.clone(), conversation_id: None })
                        },
                        "Chat"
                    }
                    button { class: "h-8.5 rounded-md bg-muted text-[11px] font-medium text-foreground", r#type: "button", "Settings" }
                }
                div { class: "grid gap-1 p-2 max-md:flex max-md:overflow-x-auto",
                    for candidate in AiSettingsSection::ALL {
                        {
                            let supported = supports_section(&ports, candidate);
                            let workspace_id = workspace.id.clone();
                            rsx! {
                        button {
                            class: if candidate == section { "shrink-0 rounded-md bg-accent px-2 py-2 text-left text-xs font-semibold text-foreground" } else { "shrink-0 rounded-md px-2 py-2 text-left text-xs text-muted-foreground" },
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
                if section == AiSettingsSection::General && ports.general_settings().is_some() {
                    GeneralSettingsPanel { workspace: workspace.clone() }
                } else if section == AiSettingsSection::ProviderAccounts && ports.provider_auth().is_some() {
                    ProviderAccountsPanel { workspace: workspace.clone() }
                } else if section == AiSettingsSection::GlobalInstructions && ports.resources().is_some() {
                    InstructionsPanel { workspace: workspace.clone() }
                } else if section == AiSettingsSection::PromptTemplates && ports.resources().is_some() {
                    PromptTemplatesPanel { workspace: workspace.clone() }
                } else if section == AiSettingsSection::Skills && ports.resources().is_some() {
                    SkillsPanel { workspace: workspace.clone() }
                } else if section == AiSettingsSection::Extensions && ports.extensions().is_some() {
                    ExtensionsPanel { workspace: workspace.clone() }
                } else if section == AiSettingsSection::General && ports.settings().is_some() {
                    p { class: "mt-1 text-xs text-muted-foreground", if settings().volatile_credential { "Browser AI settings are kept only for this tab." } else { "AI defaults are managed by the connected runtime." } }
                    div { class: "mt-5 grid max-w-xl gap-4 rounded-xl border border-border bg-background p-4",
                        Field { control_id: "ai-endpoint", label: "Endpoint",
                            TextInput { value: settings().endpoint, oninput: move |event: FormEvent| settings.write().endpoint = event.value() }
                        }
                        Field { control_id: "ai-model", label: "Default model",
                            TextInput { value: settings().model, oninput: move |event: FormEvent| settings.write().model = event.value() }
                        }
                        Field { control_id: "ai-credential", label: "API key",
                            TextInput { input_type: TextInputType::Password, value: settings().credential, autocomplete: "off", placeholder: if settings().credential_is_set { "Credential configured" } else { "sk-…" }, oninput: move |event: FormEvent| settings.write().credential = event.value() }
                        }
                        Button { label: if saving() { "Saving…" } else { "Save settings" }, kind: ButtonKind::Primary, disabled: saving(), onclick: move |_| save() }
                    }
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
                } else if has_managed {
                    p { class: "mt-4 text-sm text-muted-foreground", "Loading settings…" }
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
        AiSettingsSection::General => {
            ports.general_settings().is_some()
                || ports.settings().is_some()
                || ports.usage().is_some()
        }
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
        AiSettingsSection::General
            if ports.general_settings().is_none() && ports.settings().is_none() =>
        {
            ports
                .usage()
                .cloned()
                .map(|port| (port, AiManagedFeature::Usage))
        }
        AiSettingsSection::General
        | AiSettingsSection::ProviderAccounts
        | AiSettingsSection::GlobalInstructions
        | AiSettingsSection::PromptTemplates
        | AiSettingsSection::Skills
        | AiSettingsSection::Extensions => None,
    }
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum GeneralSettingsView {
    #[default]
    Essentials,
    Advanced,
}

#[component]
fn GeneralSettingsPanel(workspace: WorkspaceRecord) -> Element {
    let ports = use_context::<AiPorts>();
    let Some(port) = ports.general_settings().cloned() else {
        return rsx! {};
    };
    let mut revision = use_signal(|| 0_u64);
    let mut view = use_signal(GeneralSettingsView::default);
    let mut updating = use_signal(|| false);
    let mut notice = use_signal(|| None::<(String, Tone)>);
    let resource_workspace = workspace.clone();
    let resource_port = port.clone();
    let settings = use_resource(move || {
        let workspace = resource_workspace.clone();
        let port = resource_port.clone();
        let _ = revision();
        async move { port.load(&workspace).await }
    });
    rsx! {
        div { class: "mt-5 max-w-3xl space-y-5",
            div { class: "inline-flex rounded-lg border border-border bg-background p-1",
                button { r#type: "button", class: if view() == GeneralSettingsView::Essentials { "rounded-md bg-muted px-3 py-1.5 text-[10px] font-medium text-foreground" } else { "rounded-md px-3 py-1.5 text-[10px] text-muted-foreground hover:text-foreground" }, onclick: move |_| view.set(GeneralSettingsView::Essentials), "Essentials" }
                button { r#type: "button", class: if view() == GeneralSettingsView::Advanced { "rounded-md bg-muted px-3 py-1.5 text-[10px] font-medium text-foreground" } else { "rounded-md px-3 py-1.5 text-[10px] text-muted-foreground hover:text-foreground" }, onclick: move |_| view.set(GeneralSettingsView::Advanced), "Advanced JSON" }
            }
            if view() == GeneralSettingsView::Essentials {
                section {
                    h2 { class: "mb-2 px-1 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground", "Updates" }
                    div { class: "flex items-center gap-4 rounded-xl border border-border bg-background px-4 py-3",
                        div { class: "min-w-0 flex-1",
                            strong { class: "block text-xs font-medium", "Pi and tracked skills" }
                            p { class: "mt-0.5 text-[10px] leading-relaxed text-muted-foreground", "Install the latest Pi release and refresh tracked skills." }
                        }
                        Button { label: if updating() { "Updating…" } else { "Update" }, kind: ButtonKind::Primary, disabled: updating(), onclick: {
                            let port = port.clone();
                            let workspace = workspace.clone();
                            move |_| {
                                updating.set(true);
                                let port = port.clone();
                                let workspace = workspace.clone();
                                spawn(async move {
                                    match port.update_runtime(&workspace).await {
                                        Ok(message) => notice.set(Some((message, Tone::Success))),
                                        Err(problem) => notice.set(Some((problem.message, Tone::Destructive))),
                                    }
                                    updating.set(false);
                                    *revision.write() += 1;
                                });
                            }
                        } }
                    }
                }
                match settings() {
                    None => rsx! { p { class: "text-xs text-muted-foreground", "Loading Pi settings…" } },
                    Some(Err(problem)) => rsx! { p { class: "rounded-lg bg-destructive/10 p-3 text-xs text-destructive", "{problem.message}" } },
                    Some(Ok(rows)) => rsx! {
                        for section_name in general_setting_sections(&rows) {
                            section { class: "overflow-hidden rounded-xl border border-border bg-background",
                                h2 { class: "border-b border-border px-4 py-3 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground", "{section_name}" }
                                for setting in rows.iter().filter(|setting| setting.section == section_name) {
                                    GeneralSettingRow { key: "{setting.path}", workspace: workspace.clone(), setting: setting.clone(), revision }
                                }
                            }
                        }
                    },
                }
            } else {
                AdvancedSettingsPanel { workspace: workspace.clone(), revision, notice }
            }
        }
        if let Some((message, tone)) = notice() { Toast { message, tone, on_close: move |()| notice.set(None) } }
    }
}

#[component]
fn AdvancedSettingsPanel(
    workspace: WorkspaceRecord,
    mut revision: Signal<u64>,
    mut notice: Signal<Option<(String, Tone)>>,
) -> Element {
    let ports = use_context::<AiPorts>();
    let Some(port) = ports.general_settings().cloned() else {
        return rsx! {};
    };
    let mut scope = use_signal(|| AiResourceScope::Global);
    let reload = use_signal(|| 0_u64);
    let resource_workspace = workspace.clone();
    let resource_port = port.clone();
    let settings = use_resource(move || {
        let workspace = resource_workspace.clone();
        let port = resource_port.clone();
        let scope = scope();
        let _ = reload();
        async move { port.load_advanced(&workspace, scope).await }
    });
    rsx! {
        section { class: "space-y-3",
            div { class: "flex flex-wrap items-center gap-3",
                label { class: "text-[10px] font-medium text-muted-foreground", "Scope" }
                select { class: "h-8 rounded-lg border border-input bg-background px-2 text-xs", value: if scope() == AiResourceScope::Project { "project" } else { "global" }, onchange: move |event| scope.set(if event.value() == "project" { AiResourceScope::Project } else { AiResourceScope::Global }),
                    option { value: "global", "Global" }
                    option { value: "project", "Project" }
                }
                p { class: "min-w-0 flex-1 text-[10px] text-muted-foreground", "Edit every setting supported by the installed Pi version. JSON syntax is required." }
            }
            match settings() {
                None => rsx! { p { class: "text-xs text-muted-foreground", "Loading advanced settings…" } },
                Some(Err(problem)) => rsx! { p { class: "rounded-lg bg-destructive/10 p-3 text-xs text-destructive", "{problem.message}" } },
                Some(Ok(snapshot)) => rsx! { AdvancedSettingsEditor { workspace: workspace.clone(), snapshot, revision, reload, notice } },
            }
        }
    }
}

#[component]
fn AdvancedSettingsEditor(
    workspace: WorkspaceRecord,
    snapshot: AiAdvancedSettings,
    mut revision: Signal<u64>,
    mut reload: Signal<u64>,
    mut notice: Signal<Option<(String, Tone)>>,
) -> Element {
    let ports = use_context::<AiPorts>();
    let port = ports.general_settings().cloned();
    let mut draft = use_signal(|| snapshot.content.clone());
    let mut saved_content = use_signal(|| snapshot.content.clone());
    let mut current_revision = use_signal(|| snapshot.revision.clone());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut docs_query = use_signal(String::new);
    use_effect(use_reactive(
        (&snapshot.content, &snapshot.revision),
        move |(content, next_revision)| {
            draft.set(content.clone());
            saved_content.set(content);
            current_revision.set(next_revision);
            error.set(None);
        },
    ));
    let documentation = filter_documentation(&snapshot.documentation, &docs_query());
    rsx! {
        div { class: "space-y-3",
            div { class: "rounded-xl border border-border bg-background",
                div { class: "flex items-center gap-3 border-b border-border px-3 py-2",
                    code { class: "min-w-0 flex-1 truncate text-[9px] text-muted-foreground", "{snapshot.path}" }
                    Button { label: if saving() { "Saving…" } else { "Save" }, kind: ButtonKind::Primary, disabled: saving() || draft() == saved_content(), onclick: {
                        let port = port.clone();
                        let workspace = workspace.clone();
                        let scope = snapshot.scope;
                        move |_| {
                            let Some(port) = port.clone() else { return; };
                            saving.set(true);
                            error.set(None);
                            let workspace = workspace.clone();
                            let settings = AiAdvancedSettings { scope, path: String::new(), content: draft(), revision: current_revision(), documentation: String::new() };
                            spawn(async move {
                                match port.save_advanced(&workspace, settings).await {
                                    Ok(saved) => {
                                        draft.set(saved.content.clone());
                                        saved_content.set(saved.content);
                                        current_revision.set(saved.revision);
                                        notice.set(Some(("Pi settings saved. One rolling backup was kept.".into(), Tone::Success)));
                                        *revision.write() += 1;
                                    }
                                    Err(problem) => error.set(Some(problem.message)),
                                }
                                saving.set(false);
                            });
                        }
                    } }
                    Button { label: "Reload", kind: ButtonKind::Secondary, disabled: saving(), onclick: move |_| *reload.write() += 1 }
                }
                TextArea { class: "min-h-80 rounded-none border-0 p-4 font-mono text-[11px] leading-relaxed shadow-none", resize: TextAreaResize::Vertical, rows: 18, value: draft(), disabled: saving(), oninput: move |event: FormEvent| draft.set(event.value()) }
            }
            if let Some(message) = error() { p { class: "rounded-lg bg-destructive/10 p-3 text-xs text-destructive", "{message}" } }
            details { class: "rounded-xl border border-border bg-background",
                summary { class: "cursor-pointer px-4 py-3 text-xs font-medium", "Installed Pi settings reference" }
                div { class: "space-y-3 border-t border-border p-4",
                    input { class: "h-8 w-full rounded-lg border border-input bg-background px-3 text-xs", placeholder: "Search installed Pi documentation", value: docs_query(), oninput: move |event| docs_query.set(event.value()) }
                    pre { class: "max-h-96 overflow-auto whitespace-pre-wrap font-mono text-[10px] leading-relaxed text-muted-foreground", "{documentation}" }
                }
            }
        }
    }
}

fn filter_documentation(documentation: &str, query: &str) -> String {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return documentation.to_owned();
    }
    documentation
        .lines()
        .filter(|line| line.to_lowercase().contains(&query))
        .collect::<Vec<_>>()
        .join("\n")
}

#[component]
fn GeneralSettingRow(
    workspace: WorkspaceRecord,
    setting: AiGeneralSetting,
    mut revision: Signal<u64>,
) -> Element {
    let ports = use_context::<AiPorts>();
    let port = ports.general_settings().cloned();
    let label = setting.label;
    let description = setting.description;
    let current_value = setting.value;
    let setting_kind = setting.kind;
    let setting_options = setting.options;
    let setting_available = setting.available;
    let mut draft = use_signal(|| current_value.clone());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let setting_path = setting.path.clone();
    use_effect(use_reactive((&current_value,), move |(value,)| {
        draft.set(value);
    }));
    let save = EventHandler::new(move |value: String| {
        let Some(port) = port.clone() else {
            return;
        };
        saving.set(true);
        error.set(None);
        let workspace = workspace.clone();
        let path = setting_path.clone();
        spawn(async move {
            match port.save(&workspace, &path, &value).await {
                Ok(_) => *revision.write() += 1,
                Err(problem) => error.set(Some(problem.message)),
            }
            saving.set(false);
        });
    });
    rsx! {
        div { class: "grid grid-cols-[minmax(0,1fr)_minmax(9rem,14rem)] items-center gap-4 border-b border-border/70 px-4 py-3 last:border-b-0 max-sm:grid-cols-1",
            div { class: "min-w-0",
                strong { class: "block text-xs font-medium", "{label}" }
                p { class: "mt-0.5 text-[10px] leading-relaxed text-muted-foreground", "{description}" }
            }
            div { class: "min-w-0",
                match setting_kind {
                    AiGeneralSettingKind::Toggle => rsx! {
                        select { class: "h-8 w-full rounded-lg border border-input bg-background px-2 text-xs", disabled: saving() || !setting_available, value: draft(), onchange: move |event| { let value = event.value(); draft.set(value.clone()); save.call(value); },
                            option { value: "true", "On" }
                            option { value: "false", "Off" }
                        }
                    },
                    AiGeneralSettingKind::Select => rsx! {
                        select { class: "h-8 w-full rounded-lg border border-input bg-background px-2 text-xs", disabled: saving() || !setting_available, value: draft(), onchange: move |event| { let value = event.value(); draft.set(value.clone()); save.call(value); },
                            if current_value.is_empty() { option { value: "", "Not set" } }
                            for option in setting_options { option { value: option.clone(), "{option}" } }
                        }
                    },
                    AiGeneralSettingKind::Number | AiGeneralSettingKind::Text | AiGeneralSettingKind::StringArray => rsx! {
                        input { class: "h-8 w-full rounded-lg border border-input bg-background px-2 text-xs", r#type: if setting_kind == AiGeneralSettingKind::Number { "number" } else { "text" }, placeholder: if setting_kind == AiGeneralSettingKind::StringArray { "Comma-separated values" } else { "" }, disabled: saving() || !setting_available, value: draft(), oninput: move |event| draft.set(event.value()), onblur: move |_| if draft() != current_value { save.call(draft()); } }
                    },
                }
                if saving() { small { class: "text-[9px] text-muted-foreground", "Saving…" } }
                if !setting_available { small { class: "text-[9px] text-warning", "Unavailable in this Pi version" } }
                if let Some(message) = error() { small { class: "block text-[9px] text-destructive", "{message}" } }
            }
        }
    }
}

fn general_setting_sections(settings: &[AiGeneralSetting]) -> Vec<String> {
    let mut sections = Vec::new();
    for setting in settings {
        if !sections.contains(&setting.section) {
            sections.push(setting.section.clone());
        }
    }
    sections
}

#[component]
fn InstructionsPanel(workspace: WorkspaceRecord) -> Element {
    let ports = use_context::<AiPorts>();
    let Some(port) = ports.resources().cloned() else {
        return rsx! {};
    };
    let mut content = use_signal(String::new);
    let mut saved = use_signal(|| None::<String>);
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
        if saved().is_some() {
            return;
        }
        if let Some(Ok(value)) = loaded() {
            content.set(value.clone());
            saved.set(Some(value));
        }
    });
    let content_bytes = content().len();
    let too_large = content_bytes > MAX_INSTRUCTIONS_BYTES;
    let changed = saved().is_some_and(|value| value != content());
    rsx! {
        div { class: "mt-5 max-w-3xl",
            p { class: "mb-3 text-xs leading-5 text-muted-foreground", "Instance-wide Pi policy loaded automatically for every workspace." }
            match loaded() {
                None => rsx! { p { class: "text-xs text-muted-foreground", "Loading global instructions…" } },
                Some(Err(problem)) => rsx! { p { class: "text-xs text-destructive", "{problem.message}" } },
                Some(Ok(_)) => rsx! {
                    section { class: "overflow-hidden rounded-xl border border-input bg-background shadow-xs focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/20",
                        div { class: "flex min-h-12 items-center justify-between gap-3 border-b border-border px-3.5 py-2",
                            div {
                                label { class: "block text-xs font-semibold text-foreground/85", r#for: "pi-global-instructions", "AGENTS.md" }
                                p { class: "mt-0.5 text-[10px] text-muted-foreground", "Shared by every workspace" }
                            }
                            span { class: "rounded-md bg-warning/10 px-2 py-1 text-[9px] text-warning", title: "Instructions guide the agent; use deployment controls or blocking extensions for hard enforcement.", "Guidance, not enforcement" }
                        }
                        textarea {
                            id: "pi-global-instructions",
                            class: "block min-h-80 w-full resize-y border-0 bg-transparent p-4 font-mono text-xs leading-5 outline-none disabled:cursor-not-allowed disabled:opacity-50",
                            rows: 18,
                            value: content(),
                            disabled: saving(),
                            placeholder: "Add instance-wide operating constraints and preferences for Pi…",
                            aria_invalid: too_large,
                            aria_describedby: "pi-global-instructions-status",
                            oninput: move |event| content.set(event.value()),
                        }
                        div { class: "flex min-h-13 flex-wrap items-center justify-between gap-3 border-t border-border px-3.5 py-2",
                            div {
                                small { id: "pi-global-instructions-status", class: if too_large { "block text-[10px] text-destructive" } else { "block text-[10px] text-muted-foreground" },
                                    if too_large { "Instructions exceed the 512 KiB limit" } else { "{content_bytes} bytes · 512 KiB maximum" }
                                }
                                small { class: "mt-0.5 block text-[9px] text-muted-foreground/75", "Saving an empty file clears the instructions." }
                            }
                            Button { label: if saving() { "Saving…" } else { "Save" }, kind: ButtonKind::Primary, disabled: saving() || !changed || too_large, onclick: {
                            let port = port.clone();
                            let workspace = workspace.clone();
                            move |_| {
                                saving.set(true);
                                let port = port.clone();
                                let workspace = workspace.clone();
                                let value = content();
                                spawn(async move {
                                    match port.save_instructions(&workspace, &value).await {
                                        Ok(()) => {
                                            saved.set(Some(value));
                                            notice.set(Some(("Global instructions saved".into(), Tone::Success)));
                                        }
                                        Err(problem) => notice.set(Some((problem.message, Tone::Destructive))),
                                    }
                                    saving.set(false);
                                });
                            }
                            } }
                        }
                    }
                    p { class: "mt-2 text-[10px] leading-relaxed text-muted-foreground", "New chats use saved changes. Active chats retain their current instructions until reloaded or replaced." }
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
    let editing = original_name.is_some();
    rsx! {
        Modal { title: if original_name.is_some() { "Edit prompt template" } else { "New prompt template" }, description: "Prompt templates are stored in Pi's project or global resource directory.", on_close: move |()| if !saving() { on_close.call(()) },
            DialogForm {
                Field { control_id: "prompt-template-name", label: "Name", error: error(), TextInput { value: name(), autofocus: true, disabled: saving(), oninput: move |event: FormEvent| { name.set(event.value()); error.set(None); } } }
                Field { control_id: "prompt-template-description", label: "Description", TextInput { value: description(), disabled: saving(), oninput: move |event: FormEvent| description.set(event.value()) } }
                Field { control_id: "prompt-template-arguments", label: "Argument hint", TextInput { value: argument_hint(), disabled: saving(), oninput: move |event: FormEvent| argument_hint.set(event.value()) } }
                ResourceScopeSelect { scope, disabled: saving() || editing }
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
    let mut query = use_signal(String::new);
    let mut submitted_query = use_signal(String::new);
    let mut catalog_view = use_signal(AiSkillCatalogView::default);
    let mut catalog_offset = use_signal(|| 0_usize);
    let mut catalog_results = use_signal(Vec::<AiSkillSearchResult>::new);
    let mut next_offset = use_signal(|| 0_usize);
    let mut has_more = use_signal(|| false);
    let mut loading_more = use_signal(|| false);
    let mut search_revision = use_signal(|| 0_u64);
    let installing = use_signal(|| None::<String>);
    let load_port = port.clone();
    let load_workspace = workspace.clone();
    let skills = use_resource(move || {
        let port = load_port.clone();
        let workspace = load_workspace.clone();
        let _ = revision();
        async move { port.skills(&workspace).await }
    });
    let catalog_port = port.clone();
    let catalog_access = use_resource(move || {
        let port = catalog_port.clone();
        async move { port.skill_catalog_available().await }
    });
    let search_port = port.clone();
    let catalog_search = use_resource(move || {
        let port = search_port.clone();
        let query = submitted_query().trim().to_owned();
        let view = catalog_view();
        let offset = catalog_offset();
        let catalog_available = catalog_access().and_then(Result::ok).unwrap_or(false);
        let _ = search_revision();
        async move {
            let result = if query.is_empty() {
                if catalog_available {
                    port.browse_skills(view, offset).await
                } else {
                    Ok(crate::AiSkillSearchPage::default())
                }
            } else {
                port.search_skills(&query, offset).await
            };
            (query, view, result)
        }
    });
    use_effect(move || {
        let Some((resource_query, resource_view, result)) = catalog_search() else {
            return;
        };
        if resource_query != submitted_query().trim() || resource_view != catalog_view() {
            return;
        }
        loading_more.set(false);
        let Ok(page) = result else {
            return;
        };
        if page.start_offset == 0 {
            catalog_results.set(page.skills);
        } else {
            catalog_results.with_mut(|loaded| {
                for result in page.skills {
                    if !loaded.iter().any(|item| item.slug == result.slug) {
                        loaded.push(result);
                    }
                }
            });
        }
        next_offset.set(page.next_offset);
        has_more.set(page.has_more);
    });
    let search_result = catalog_search();
    let searching = search_result.is_none();
    let search_error = search_result
        .as_ref()
        .and_then(|(_, _, result)| result.as_ref().err())
        .map(|problem| problem.message.clone());
    let catalog_enabled = catalog_access().and_then(Result::ok).unwrap_or(false);
    let installed_skills = skills().and_then(Result::ok).unwrap_or_default();
    let result_catalog_view = submitted_query().is_empty().then_some(catalog_view());
    rsx! {
        div { class: "mt-5 max-w-4xl space-y-6",
            div { class: "mb-3 flex items-center justify-between gap-3",
                p { class: "text-xs text-muted-foreground", "Project and global Pi skills." }
                Button { label: "New skill", kind: ButtonKind::Primary, onclick: move |_| editor.set(Some((None, empty_skill()))) }
            }
            section { "aria-labelledby": "skill-discovery-title",
                h2 { id: "skill-discovery-title", class: "mb-2 text-xs font-semibold", "Discover skills" }
                p { class: "mb-3 text-[10px] leading-relaxed text-muted-foreground", "Searches the public skills.sh catalog. Review installed skills before use: they may include executable scripts and instructions with the server user's permissions." }
                div { class: if catalog_enabled { "grid grid-cols-[minmax(12rem,1fr)_10rem_auto] gap-2 max-sm:grid-cols-1" } else { "grid grid-cols-[minmax(12rem,1fr)_auto] gap-2 max-sm:grid-cols-1" },
                    TextInput { value: query(), placeholder: "Search skills (for example: Rust)", oninput: move |event: FormEvent| query.set(event.value()) }
                    if catalog_enabled {
                        select { class: "h-9 rounded-lg border border-input bg-background px-3 text-xs", aria_label: "Skills catalog view", value: skill_catalog_view_value(catalog_view()), disabled: searching, onchange: move |event| {
                            catalog_view.set(match event.value().as_str() {
                                "trending" => AiSkillCatalogView::Trending,
                                "hot" => AiSkillCatalogView::Hot,
                                _ => AiSkillCatalogView::AllTime,
                            });
                            query.set(String::new());
                            submitted_query.set(String::new());
                            catalog_results.set(Vec::new());
                            catalog_offset.set(0);
                            has_more.set(false);
                            loading_more.set(false);
                        },
                            option { value: "all-time", "All time" }
                            option { value: "trending", "Trending" }
                            option { value: "hot", "Hot" }
                        }
                    }
                    Button { label: if searching { "Searching…" } else { "Search" }, kind: ButtonKind::Secondary, disabled: searching || query().trim().chars().count() == 1 || (!catalog_enabled && query().trim().is_empty()), onclick: move |_| {
                        catalog_results.set(Vec::new());
                        catalog_offset.set(0);
                        has_more.set(false);
                        loading_more.set(false);
                        submitted_query.set(query().trim().to_owned());
                        *search_revision.write() += 1;
                    } }
                }
                if !catalog_enabled {
                    p { class: "mt-2 text-[9px] text-muted-foreground", "Leaderboard browsing is not configured; catalog search remains available." }
                }
                if let Some(problem) = search_error.as_ref() {
                    p { class: "mt-3 rounded-lg bg-destructive/10 p-3 text-xs text-destructive", role: "alert", "{problem}" }
                }
                if !catalog_results().is_empty() {
                    p { class: "py-3 text-[9px] text-muted-foreground", "{catalog_results().len()} loaded results" }
                    div { class: "grid grid-cols-2 gap-3 max-lg:grid-cols-1",
                        for result in catalog_results() {
                            SkillCatalogCard {
                                key: "{result.slug}",
                                project_installed: installed_skills.iter().any(|skill| skill.name == result.name && skill.scope == AiResourceScope::Project),
                                global_installed: installed_skills.iter().any(|skill| skill.name == result.name && skill.scope == AiResourceScope::Global),
                                result,
                                catalog_view: result_catalog_view,
                                workspace: workspace.clone(),
                                installing,
                                on_installed: move |()| { *revision.write() += 1; notice.set(Some(("Skill installed".into(), Tone::Success))); },
                                on_error: move |message| notice.set(Some((message, Tone::Destructive))),
                            }
                        }
                    }
                    if has_more() {
                        div { class: "mt-4 flex justify-center",
                            Button { label: if loading_more() { "Loading more…" } else { "Load more" }, kind: ButtonKind::Ghost, disabled: searching || loading_more(), onclick: move |_| { loading_more.set(true); catalog_offset.set(next_offset()); } }
                        }
                    } else {
                        p { class: "py-4 text-center text-[9px] text-muted-foreground", "End of catalog results" }
                    }
                } else if !searching && search_error.is_none() && (catalog_enabled || !submitted_query().is_empty()) {
                    p { class: "mt-4 rounded-xl border border-dashed border-border p-6 text-center text-xs text-muted-foreground", if submitted_query().is_empty() { "No skills are available in this catalog view." } else { "No skills matched this search." } }
                }
            }
            section { "aria-labelledby": "installed-skills-title",
                h2 { id: "installed-skills-title", class: "mb-2 text-xs font-semibold", "Installed skills" }
                match skills() {
                    None => rsx! { p { class: "text-xs text-muted-foreground", "Loading skills…" } },
                    Some(Err(problem)) => rsx! { p { class: "text-xs text-destructive", "{problem.message}" } },
                    Some(Ok(items)) if items.is_empty() => rsx! { p { class: "rounded-xl border border-dashed border-border p-6 text-center text-xs text-muted-foreground", "No directly managed Pi skills yet." } },
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
fn SkillCatalogCard(
    result: AiSkillSearchResult,
    catalog_view: Option<AiSkillCatalogView>,
    project_installed: bool,
    global_installed: bool,
    workspace: WorkspaceRecord,
    mut installing: Signal<Option<String>>,
    on_installed: EventHandler<()>,
    on_error: EventHandler<String>,
) -> Element {
    let installed = project_installed || global_installed;
    rsx! {
        article { class: "flex min-h-40 flex-col rounded-xl border border-border bg-background p-4",
            div { class: "flex items-start gap-3",
                div { class: "min-w-0 flex-1",
                    a { class: "break-all text-sm font-semibold text-foreground hover:text-primary hover:underline", href: result.page_url.clone(), target: "_blank", rel: "noopener noreferrer", "{result.name}" }
                    p { class: "mt-2 text-[10px] text-muted-foreground", "{result.source}" }
                }
                if installed { span { class: "shrink-0 rounded-md bg-success/12 px-2 py-1 text-[8px] font-medium text-success", "Installed" } }
            }
            div { class: "mt-auto flex flex-wrap items-end justify-between gap-3 pt-4",
                small { class: "text-[9px] text-muted-foreground", "{skill_metric(result.installs, catalog_view)}" }
                if result.installable {
                    div { class: "flex gap-1",
                        SkillInstallButton { label: "Project", installed: project_installed, scope: AiResourceScope::Project, result: result.clone(), workspace: workspace.clone(), installing, on_installed, on_error }
                        SkillInstallButton { label: "Global", installed: global_installed, scope: AiResourceScope::Global, result: result.clone(), workspace: workspace.clone(), installing, on_installed, on_error }
                    }
                } else {
                    small { class: "text-[9px] text-muted-foreground", "External source" }
                }
            }
        }
    }
}

#[component]
fn SkillInstallButton(
    label: String,
    installed: bool,
    scope: AiResourceScope,
    result: AiSkillSearchResult,
    workspace: WorkspaceRecord,
    mut installing: Signal<Option<String>>,
    on_installed: EventHandler<()>,
    on_error: EventHandler<String>,
) -> Element {
    let ports = use_context::<AiPorts>();
    let Some(port) = ports.resources().cloned() else {
        return rsx! {};
    };
    let pending = installing().as_deref() == Some(result.slug.as_str());
    rsx! {
        Button { label: if pending { "Installing…".to_owned() } else if installed { "Installed".to_owned() } else { label }, kind: if installed { ButtonKind::Secondary } else { ButtonKind::Primary }, disabled: installed || installing().is_some(), onclick: move |_| {
            installing.set(Some(result.slug.clone()));
            let port = port.clone();
            let workspace = workspace.clone();
            let slug = result.slug.clone();
            spawn(async move {
                match port.install_skill(&workspace, &slug, scope).await {
                    Ok(()) => on_installed.call(()),
                    Err(problem) => on_error.call(problem.message),
                }
                installing.set(None);
            });
        } }
    }
}

fn skill_metric(installs: u64, view: Option<AiSkillCatalogView>) -> String {
    let count = format_compact_count(installs);
    match view {
        Some(AiSkillCatalogView::Trending) => format!("{count} in 24h"),
        Some(AiSkillCatalogView::Hot) => format!("{count} this hour"),
        Some(AiSkillCatalogView::AllTime) | None => format!("{count} installs"),
    }
}

fn format_compact_count(value: u64) -> String {
    let (divisor, suffix) = if value >= 1_000_000 {
        (1_000_000, "M")
    } else if value >= 1_000 {
        (1_000, "K")
    } else {
        return value.to_string();
    };
    let whole = value / divisor;
    let decimal = value % divisor * 10 / divisor;
    if decimal == 0 {
        format!("{whole}{suffix}")
    } else {
        format!("{whole}.{decimal}{suffix}")
    }
}

const fn skill_catalog_view_value(view: AiSkillCatalogView) -> &'static str {
    match view {
        AiSkillCatalogView::AllTime => "all-time",
        AiSkillCatalogView::Trending => "trending",
        AiSkillCatalogView::Hot => "hot",
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
    let editing = original_storage_name.is_some();
    rsx! {
        Modal { title: if original_storage_name.is_some() { "Edit skill" } else { "New skill" }, description: "Skills provide reusable instructions and workflows to Pi.", on_close: move |()| if !saving() { on_close.call(()) },
            DialogForm {
                Field { control_id: "skill-name", label: "Name", error: error(), TextInput { value: name(), autofocus: true, disabled: saving(), oninput: move |event: FormEvent| { name.set(event.value()); error.set(None); } } }
                Field { control_id: "skill-description", label: "Description", TextInput { value: description(), disabled: saving(), oninput: move |event: FormEvent| description.set(event.value()) } }
                ResourceScopeSelect { scope, disabled: saving() || editing }
                Field { control_id: "skill-content", label: "Instructions",
                    textarea { id: "skill-content", class: "min-h-64 w-full resize-y rounded-lg border border-input bg-background p-3 font-mono text-xs", value: content(), disabled: saving(), oninput: move |event| content.set(event.value()) }
                }
                DialogActions {
                    Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: saving(), onclick: move |_| on_close.call(()) }
                    Button { label: if saving() { "Saving…" } else { "Save skill" }, kind: ButtonKind::Primary, disabled: saving() || name().trim().is_empty() || description().trim().is_empty() || content().trim().is_empty(), onclick: {
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
        content: "# Instructions\n\n".into(),
        scope: AiResourceScope::Project,
        storage_name: String::new(),
        single_file: false,
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
    let mut package_type = use_signal(|| "all".to_owned());
    let mut installation = use_signal(|| "all".to_owned());
    let mut sort = use_signal(|| "downloads".to_owned());
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
    let visible = filtered_extensions(&loaded(), &package_type(), &installation(), &sort());
    let result = results();
    let loading = result.is_none();
    let load_error = result
        .as_ref()
        .and_then(|(_, _, result)| result.as_ref().err())
        .map(|problem| problem.message.clone());
    rsx! {
        div { class: "mt-5 max-w-4xl",
            div { class: "grid grid-cols-[minmax(14rem,1fr)_minmax(9rem,0.35fr)_minmax(9rem,0.35fr)_minmax(10rem,0.4fr)_auto] gap-2 max-xl:grid-cols-2 max-sm:grid-cols-1",
                TextInput { value: query(), placeholder: "Filter packages…", oninput: move |event: FormEvent| { query.set(event.value()); offset.set(0); loaded.set(Vec::new()); has_more.set(true); } }
                select { class: "h-9 rounded-lg border border-input bg-background px-2.5 text-xs", value: package_type(), aria_label: "Package type", onchange: move |event| package_type.set(event.value()),
                    option { value: "all", "All types" }
                    option { value: "extension", "Extensions" }
                    option { value: "skill", "Skills" }
                    option { value: "prompt", "Prompts" }
                    option { value: "theme", "Themes" }
                    option { value: "package", "Packages" }
                }
                select { class: "h-9 rounded-lg border border-input bg-background px-2.5 text-xs", value: installation(), aria_label: "Installation status", onchange: move |event| installation.set(event.value()),
                    option { value: "all", "All packages" }
                    option { value: "installed", "Installed" }
                    option { value: "not-installed", "Not installed" }
                }
                select { class: "h-9 rounded-lg border border-input bg-background px-2.5 text-xs", value: sort(), aria_label: "Package sorting", onchange: move |event| sort.set(event.value()),
                    option { value: "downloads", "Most downloads" }
                    option { value: "recent", "Recently published" }
                    option { value: "az", "A–Z" }
                }
                Button { label: "Reset", kind: ButtonKind::Ghost, onclick: move |_| {
                    query.set(String::new());
                    package_type.set("all".into());
                    installation.set("all".into());
                    sort.set("downloads".into());
                    offset.set(0);
                    loaded.set(Vec::new());
                    has_more.set(true);
                } }
            }
            p { class: "py-3 text-[10px] text-muted-foreground", "Showing {visible.len()} of {loaded().len()} loaded packages · {total()} npm matches" }
            if let Some(problem) = load_error.clone() {
                p { class: "mb-3 rounded-lg bg-destructive/10 p-3 text-xs text-destructive", role: "alert", "{problem}" }
            }
            if loaded().is_empty() && loading {
                p { class: "p-6 text-center text-xs text-muted-foreground", "Loading extensions…" }
            } else if loaded().is_empty() && load_error.is_some() {
                p { class: "rounded-xl border border-dashed border-border p-6 text-center text-xs text-destructive", "Could not load packages." }
            } else {
                if visible.is_empty() {
                    p { class: "rounded-xl border border-dashed border-border p-6 text-center text-xs text-muted-foreground", "No loaded packages match these filters. Load more or change the filters." }
                } else {
                    div { class: "grid grid-cols-2 gap-3 max-lg:grid-cols-1",
                        for package in visible {
                            ExtensionCard { key: "{package.name}", package: package.clone(), pending: pending().is_some(), on_manage: move |action| confirm.set(Some((package.clone(), action))) }
                        }
                    }
                }
                if has_more() {
                    div { class: "mt-4 flex justify-center",
                        Button { label: "Load more", kind: ButtonKind::Ghost, disabled: loading, onclick: move |_| offset.set(next_offset()) }
                    }
                } else {
                    p { class: "py-4 text-center text-[9px] text-muted-foreground", "End of catalog results" }
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
                                    Ok(message) => { notice.set(Some((message, Tone::Success))); confirm.set(None); offset.set(0); loaded.set(Vec::new()); has_more.set(true); *revision.write() += 1; }
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
fn ExtensionCard(
    package: AiExtension,
    pending: bool,
    on_manage: EventHandler<AiExtensionAction>,
) -> Element {
    let user_installed = package.installed_scopes.iter().any(|scope| scope == "user");
    let project_only = !user_installed
        && package
            .installed_scopes
            .iter()
            .any(|scope| scope == "project");
    let action = if user_installed {
        AiExtensionAction::Uninstall
    } else {
        AiExtensionAction::Install
    };
    let npm_url = format!("https://www.npmjs.com/package/{}", package.name);
    rsx! {
        article { class: "flex min-h-48 flex-col rounded-xl border border-border bg-background p-4",
            div { class: "flex items-start gap-3",
                div { class: "min-w-0 flex-1",
                    a { class: "break-all text-sm font-semibold text-foreground hover:text-primary hover:underline", href: npm_url, target: "_blank", rel: "noopener noreferrer", "{package.name}" }
                    p { class: "mt-2 line-clamp-3 text-[11px] leading-relaxed text-muted-foreground", "{package.description}" }
                }
                if package.installed { span { class: "shrink-0 rounded-md bg-success/12 px-2 py-1 text-[8px] font-medium text-success", "Installed" } }
            }
            div { class: "mt-3 flex flex-wrap items-center gap-x-3 gap-y-1 text-[9px] text-muted-foreground",
                span { "{package.publisher}" }
                span { "{format_compact_count(package.monthly_downloads)}/mo" }
                if !package.published_at.is_empty() { span { "{format_published_at(&package.published_at)}" } }
            }
            div { class: "mt-3 flex flex-wrap gap-1.5",
                if package.kinds.is_empty() {
                    span { class: "rounded-md border border-border bg-secondary px-2 py-1 text-[8px] text-muted-foreground uppercase", "package" }
                } else {
                    for kind in package.kinds.clone() {
                        span { class: "rounded-md border border-border bg-secondary px-2 py-1 text-[8px] text-muted-foreground uppercase", "{kind}" }
                    }
                }
            }
            div { class: "mt-auto flex items-end justify-between gap-3 pt-4",
                small { class: "truncate text-[9px] text-muted-foreground", "v{package.version}" }
                Button { label: if pending { "Working…" } else if project_only { "Project-installed" } else if user_installed { "Uninstall" } else { "Install" }, kind: if user_installed { ButtonKind::Danger } else { ButtonKind::Primary }, disabled: pending || project_only, onclick: move |_| on_manage.call(action) }
            }
        }
    }
}

fn filtered_extensions(
    packages: &[AiExtension],
    package_type: &str,
    installation: &str,
    sort: &str,
) -> Vec<AiExtension> {
    let mut packages = packages
        .iter()
        .filter(|package| match package_type {
            "all" => true,
            "package" => package.kinds.is_empty(),
            kind => package.kinds.iter().any(|candidate| candidate == kind),
        })
        .filter(|package| match installation {
            "installed" => package.installed,
            "not-installed" => !package.installed,
            _ => true,
        })
        .cloned()
        .collect::<Vec<_>>();
    match sort {
        "recent" => packages.sort_by(|left, right| {
            right
                .published_at
                .cmp(&left.published_at)
                .then_with(|| left.name.cmp(&right.name))
        }),
        "az" => packages.sort_by(|left, right| left.name.cmp(&right.name)),
        _ => packages.sort_by(|left, right| {
            right
                .monthly_downloads
                .cmp(&left.monthly_downloads)
                .then_with(|| left.name.cmp(&right.name))
        }),
    }
    packages
}

fn format_published_at(value: &str) -> &str {
    value.split('T').next().unwrap_or(value)
}

async fn load_ai_images(
    files: Vec<dioxus::html::FileData>,
    mut attachments: Signal<Vec<AiImageAttachment>>,
    mut error: Signal<Option<String>>,
) {
    for file in files {
        if attachments().len() >= MAX_PROMPT_IMAGES {
            error.set(Some(format!("Attach up to {MAX_PROMPT_IMAGES} images.")));
            break;
        }
        let mime_type = file.content_type().unwrap_or_default();
        if !mime_type.starts_with("image/") {
            error.set(Some(format!("{} is not an image.", file.name())));
            continue;
        }
        let total = attachments().iter().map(|image| image.size).sum::<u64>();
        if file.size() > MAX_IMAGE_BYTES
            || total.saturating_add(file.size()) > MAX_TOTAL_IMAGE_BYTES
        {
            error.set(Some("Images can be 8 MiB each and 16 MiB total.".into()));
            continue;
        }
        match file.read_bytes().await {
            Ok(bytes) => attachments.write().push(AiImageAttachment {
                name: file.name(),
                mime_type,
                size: file.size(),
                data: BASE64.encode(bytes),
            }),
            Err(_) => error.set(Some(format!("Could not read {}.", file.name()))),
        }
    }
}

fn apply_ai_client_event(
    event: AiClientEvent,
    mut prompt: Signal<String>,
    mut attachments: Signal<Vec<AiImageAttachment>>,
    mut error: Signal<Option<String>>,
    mut speech_active: Signal<bool>,
    mut read_aloud_available: Signal<bool>,
    mut speaking_message: Signal<Option<String>>,
) {
    match event.kind.as_str() {
        "availability" => read_aloud_available.set(event.available.unwrap_or(false)),
        "error" => {
            speech_active.set(false);
            error.set(Some(event.message.unwrap_or_else(|| {
                "The AI browser control stopped unexpectedly.".into()
            })));
        }
        "transcript" => {
            if let Some(text) = event.text {
                let mut value = prompt.write();
                if !value.is_empty() && !value.ends_with(char::is_whitespace) {
                    value.push(' ');
                }
                value.push_str(text.trim());
            }
        }
        "start" if event.id.as_deref() == Some("syntaxis-ai-composer") => {
            speech_active.set(true);
        }
        "end" if event.id.as_deref() == Some("syntaxis-ai-composer") => {
            speech_active.set(false);
        }
        "start" => speaking_message.set(event.id),
        "end" => {
            if event.id.is_none() || speaking_message() == event.id {
                speaking_message.set(None);
            }
        }
        "image" => {
            let Some(data) = event.data else {
                return;
            };
            let mime_type = event.mime_type.unwrap_or_default();
            if !mime_type.starts_with("image/") {
                return;
            }
            if attachments().len() >= MAX_PROMPT_IMAGES {
                error.set(Some(format!("Attach up to {MAX_PROMPT_IMAGES} images.")));
                return;
            }
            let Ok(bytes) = BASE64.decode(&data) else {
                error.set(Some("Could not read the pasted image.".into()));
                return;
            };
            let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            let total = attachments().iter().map(|image| image.size).sum::<u64>();
            if size > MAX_IMAGE_BYTES || total.saturating_add(size) > MAX_TOTAL_IMAGE_BYTES {
                error.set(Some("Images can be 8 MiB each and 16 MiB total.".into()));
                return;
            }
            attachments.write().push(AiImageAttachment {
                name: event.name.unwrap_or_else(|| "Pasted image".into()),
                mime_type,
                size,
                data,
            });
            error.set(None);
        }
        _ => {}
    }
}

#[cfg(test)]
mod event_tests {
    use super::{filtered_extensions, highlighted_parts};
    use crate::conversation::apply_event_to_conversation;
    use crate::{
        AiActivity, AiConversation, AiEvent, AiExtension, AiMessage, AiMessageStatus, AiRole,
    };

    fn extension(
        name: &str,
        downloads: u64,
        kinds: &[&str],
        installed_scopes: &[&str],
    ) -> AiExtension {
        AiExtension {
            name: name.into(),
            version: "1.0.0".into(),
            description: String::new(),
            publisher: "publisher".into(),
            published_at: format!("2026-01-0{}T00:00:00Z", downloads.min(9)),
            monthly_downloads: downloads,
            kinds: kinds.iter().map(|kind| (*kind).to_owned()).collect(),
            installed_scopes: installed_scopes
                .iter()
                .map(|scope| (*scope).to_owned())
                .collect(),
            installed: !installed_scopes.is_empty(),
        }
    }

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
                ..AiMessage::default()
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
            ..AiMessage::default()
        });
        apply_event_to_conversation(&mut conversation, &event);
        apply_event_to_conversation(&mut conversation, &event);
        assert_eq!(conversation.messages.len(), 1);
    }

    #[test]
    fn authoritative_user_event_replaces_the_local_placeholder() {
        let mut conversation = AiConversation::default();
        conversation.messages.push(AiMessage {
            id: "local-user".into(),
            role: AiRole::User,
            content: "question".into(),
            ..AiMessage::default()
        });
        conversation.item_order.push("local-user".into());

        apply_event_to_conversation(
            &mut conversation,
            &AiEvent::UserMessage(AiMessage {
                id: "user-1".into(),
                entry_id: Some("entry-1".into()),
                role: AiRole::User,
                content: "question".into(),
                ..AiMessage::default()
            }),
        );

        assert_eq!(conversation.messages.len(), 1);
        assert_eq!(conversation.messages[0].id, "user-1");
        assert_eq!(
            conversation.messages[0].entry_id.as_deref(),
            Some("entry-1")
        );
        assert_eq!(conversation.item_order, vec!["user-1".to_owned()]);
    }

    #[test]
    fn authoritative_activity_updates_preserve_terminal_status() {
        let mut conversation = AiConversation::default();
        let running = AiActivity::Tool {
            id: "tool-1".into(),
            name: "shell".into(),
            summary: "Running tests".into(),
            output: String::new(),
            args: None,
            details: None,
            args_truncated: false,
            details_truncated: false,
            status: AiMessageStatus::Running,
        };
        apply_event_to_conversation(&mut conversation, &AiEvent::ActivityUpdated(running));
        let failed = AiActivity::Tool {
            id: "tool-1".into(),
            name: "shell".into(),
            summary: "Running tests".into(),
            output: "failed".into(),
            args: None,
            details: None,
            args_truncated: false,
            details_truncated: false,
            status: AiMessageStatus::Failed,
        };
        apply_event_to_conversation(&mut conversation, &AiEvent::ActivityUpdated(failed));

        assert_eq!(conversation.activity.len(), 1);
        assert!(matches!(
            conversation.activity[0],
            AiActivity::Tool {
                status: AiMessageStatus::Failed,
                ..
            }
        ));
    }

    #[test]
    fn conversation_search_highlights_case_insensitive_matches() {
        assert_eq!(
            highlighted_parts("Fix the Search result", "search"),
            vec![
                ("Fix the ".to_owned(), false),
                ("Search".to_owned(), true),
                (" result".to_owned(), false),
            ]
        );
    }

    #[test]
    fn conversation_search_preserves_non_matching_text() {
        assert_eq!(
            highlighted_parts("No match", "missing"),
            vec![("No match".to_owned(), false)]
        );
    }

    #[test]
    fn extension_filters_preserve_type_and_installation_semantics() {
        let extensions = [
            extension("extension", 1, &["extension"], &["user"]),
            extension("skill", 2, &["skill"], &[]),
        ];

        let visible = filtered_extensions(&extensions, "extension", "installed", "downloads");

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].name, "extension");
    }

    #[test]
    fn extension_sorting_uses_the_selected_order() {
        let extensions = [
            extension("alpha", 1, &[], &[]),
            extension("beta", 9, &[], &[]),
        ];

        assert_eq!(
            filtered_extensions(&extensions, "all", "all", "downloads")[0].name,
            "beta"
        );
        assert_eq!(
            filtered_extensions(&extensions, "all", "all", "az")[0].name,
            "alpha"
        );
    }
}
