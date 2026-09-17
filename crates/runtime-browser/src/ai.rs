use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use async_trait::async_trait;
#[path = "ai_auth.rs"]
mod auth;
#[path = "ai_resources.rs"]
mod resources;
use crate::bridge::{BrowserBridge, ensure_bridge};
#[cfg(target_arch = "wasm32")]
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use dioxus::prelude::document;
use serde::{Deserialize, Serialize};
use syntaxis_app_contracts::WorkspaceEventBus;
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, RetryAdvice};
use syntaxis_module_ai::apply_event_to_conversation;
use syntaxis_module_ai::{
    AiClientEvent, AiClientEventStream, AiClientPort, AiConversation, AiConversationMatch,
    AiConversationPort, AiConversationSummary, AiEvent, AiEventStream, AiMessage, AiModel,
    AiModelPort, AiPrompt, AiProviderSettings, AiRole, AiSettingsPort, AiThinkingLevel,
};
use syntaxis_workspace::WorkspaceRecord;
const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "openai/gpt-4.1-mini";
const MAX_PROMPT_BYTES: usize = 64 * 1024;
const MAX_CONTEXT_BYTES: usize = 128 * 1024;
const MAX_CONVERSATION_BYTES: usize = 32 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_RESPONSE_EVENTS: usize = 8_192;
const MAX_CONVERSATIONS: usize = 100;
const MAX_ENDPOINT_BYTES: usize = 2 * 1024;
const MAX_MODEL_BYTES: usize = 512;
const MAX_CREDENTIAL_BYTES: usize = 16 * 1024;

#[derive(Clone)]
pub struct BrowserAiAdapter {
    state: Arc<Mutex<BrowserAiState>>,
    next_id: Arc<AtomicU64>,
    workspace_events: WorkspaceEventBus,
}

struct BrowserAiState {
    settings: AiProviderSettings,
    credentials: HashMap<String, String>,
    auth_flows: HashMap<String, syntaxis_module_ai::AiAuthFlow>,
    histories: HashMap<String, serde_json::Value>,
    active_bash: Option<String>,
    model_preferences: syntaxis_module_ai::AiModelPreferences,
    conversations: HashMap<String, AiConversation>,
    cancelled: HashSet<String>,
}

impl Default for BrowserAiAdapter {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(BrowserAiState {
                settings: AiProviderSettings {
                    endpoint: DEFAULT_ENDPOINT.into(),
                    model: DEFAULT_MODEL.into(),
                    credential: String::new(),
                    credential_is_set: false,
                    volatile_credential: true,
                },
                credentials: HashMap::new(),
                auth_flows: HashMap::new(),
                histories: HashMap::new(),
                active_bash: None,
                model_preferences: syntaxis_module_ai::AiModelPreferences::default(),
                conversations: HashMap::new(),
                cancelled: HashSet::new(),
            })),
            next_id: Arc::new(AtomicU64::new(1)),
            workspace_events: WorkspaceEventBus::default(),
        }
    }
}

impl BrowserAiAdapter {
    pub fn new(workspace_events: WorkspaceEventBus) -> Self {
        Self {
            workspace_events,
            ..Self::default()
        }
    }

    fn id(&self, prefix: &str) -> String {
        format!("{prefix}-{}", self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BrowserAiState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

struct BrowserAiClientEventStream {
    events: dioxus::document::Eval,
}

impl Drop for BrowserAiClientEventStream {
    fn drop(&mut self) {
        let _ = self.events.send(true);
    }
}

#[async_trait(?Send)]
impl AiClientEventStream for BrowserAiClientEventStream {
    async fn receive(&mut self) -> Result<Option<AiClientEvent>, AppError> {
        match self.events.recv::<AiClientEvent>().await {
            Ok(event) => Ok(Some(event)),
            Err(_) => Ok(None),
        }
    }
}

#[async_trait(?Send)]
impl AiClientPort for BrowserAiAdapter {
    async fn listen(&self, composer_id: &str) -> Result<Box<dyn AiClientEventStream>, AppError> {
        let events = ai_client_listener();
        events.send(composer_id).map_err(|error| {
            ai_error(
                AppErrorCode::Internal,
                format!("Could not start AI client events: {error}"),
            )
        })?;
        Ok(Box::new(BrowserAiClientEventStream { events }))
    }

    async fn load_state(&self, key: &str) -> Result<Option<String>, AppError> {
        load_client_state(key).await
    }

    async fn save_state(&self, key: &str, value: Option<&str>) -> Result<(), AppError> {
        save_client_state(key, value).await
    }

    async fn copy_text(&self, value: &str) -> Result<(), AppError> {
        copy_client_text(value).await
    }

    async fn focus(&self, element_id: &str) -> Result<(), AppError> {
        call_ai_client("focusComposer", element_id).await
    }

    async fn toggle_speech(&self, composer_id: &str) -> Result<(), AppError> {
        call_ai_client("toggleSpeech", composer_id).await
    }

    async fn toggle_read_aloud(&self, message_id: &str) -> Result<(), AppError> {
        call_ai_client("toggleReadAloud", message_id).await
    }
}

#[async_trait(?Send)]
impl AiConversationPort for BrowserAiAdapter {
    async fn list(
        &self,
        _workspace: &WorkspaceRecord,
    ) -> Result<Vec<AiConversationSummary>, AppError> {
        Ok(self
            .lock()
            .conversations
            .values()
            .map(|conversation| AiConversationSummary {
                id: conversation.id.clone(),
                title: conversation.title.clone(),
                message_count: conversation.messages.len(),
                updated_at_ms: 0,
                status_message: conversation.status_message.clone(),
                running: conversation.running,
            })
            .collect())
    }

    async fn search(
        &self,
        _workspace: &WorkspaceRecord,
        query: &str,
    ) -> Result<Vec<AiConversationMatch>, AppError> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let mut results = Vec::new();
        for conversation in self.lock().conversations.values() {
            for message in &conversation.messages {
                let content = message.content.to_lowercase();
                let count = content.match_indices(&query).count();
                if count == 0 {
                    continue;
                }
                let start = content.find(&query).unwrap_or_default().saturating_sub(48);
                let end = (start + 180).min(message.content.len());
                let snippet = message
                    .content
                    .get(start..end)
                    .unwrap_or(&message.content)
                    .to_owned();
                results.push(AiConversationMatch {
                    session_id: conversation.id.clone(),
                    title: conversation.title.clone(),
                    updated_at_ms: 0,
                    role: message.role,
                    snippet,
                    match_count: count,
                });
                break;
            }
        }
        Ok(results)
    }

    async fn create(&self, workspace: &WorkspaceRecord) -> Result<AiConversation, AppError> {
        if self.lock().conversations.len() >= MAX_CONVERSATIONS {
            return Err(ai_error(
                AppErrorCode::TooLarge,
                "The browser AI conversation limit has been reached.",
            ));
        }
        let default_model = self.lock().settings.model.clone();
        let models = self.list_models(workspace, "").await?;
        let model = models
            .iter()
            .find(|model| model.id == default_model)
            .or_else(|| models.first())
            .ok_or_else(|| {
                ai_error(
                    AppErrorCode::InvalidInput,
                    "No available models. Add a provider API key in Settings > Provider accounts, then choose a default model in General.",
                )
            })?;
        let thinking_level = if model.thinking_levels.contains(&AiThinkingLevel::Medium) {
            AiThinkingLevel::Medium
        } else {
            model
                .thinking_levels
                .first()
                .copied()
                .unwrap_or(AiThinkingLevel::Off)
        };
        let conversation = AiConversation {
            id: self.id("browser-chat"),
            title: "New chat".into(),
            messages: Vec::new(),
            activity: Vec::new(),
            item_order: Vec::new(),
            selected_model_id: Some(model.id.clone()),
            thinking_level,
            usage: None,
            running: false,
            status_message: "Ready".into(),
            pending_messages: 0,
            steering_queue: Vec::new(),
            follow_up_queue: Vec::new(),
            commands: self.resource_commands(workspace).await?,
            supports_queued_prompts: false,
            extension_request: None,
            extension_title: None,
            extension_statuses: Vec::new(),
            extension_widgets: Vec::new(),
            requested_composer_text: None,
        };
        self.lock()
            .conversations
            .insert(conversation.id.clone(), conversation.clone());
        Ok(conversation)
    }

    async fn open(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<AiConversation, AppError> {
        let mut conversation = self
            .lock()
            .conversations
            .get(conversation_id)
            .cloned()
            .ok_or_else(|| {
                ai_error(AppErrorCode::NotFound, "The AI conversation was not found.")
            })?;
        conversation.commands = self.resource_commands(workspace).await?;
        Ok(conversation)
    }

    async fn watch(
        &self,
        _workspace: &WorkspaceRecord,
        _conversation_id: &str,
    ) -> Result<Box<dyn AiEventStream>, AppError> {
        Ok(Box::new(EmptyAiEventStream))
    }

    async fn send(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
        prompt: AiPrompt,
    ) -> Result<Box<dyn AiEventStream>, AppError> {
        ensure_bridge(BrowserBridge::Ai)
            .await
            .map_err(|message| ai_error(AppErrorCode::Internal, message))?;
        if prompt.text.len() > MAX_PROMPT_BYTES {
            return Err(ai_error(
                AppErrorCode::TooLarge,
                "The prompt exceeds 64 KiB.",
            ));
        }
        if prompt.images.len() > 5
            || prompt.images.iter().map(|image| image.size).sum::<u64>() > 16 * 1024 * 1024
        {
            return Err(ai_error(
                AppErrorCode::TooLarge,
                "Attach up to 5 images, 16 MiB total.",
            ));
        }
        let (workspace_instructions, expanded_prompt) =
            self.prepare_resources(workspace, &prompt.text).await?;
        let text = match prompt.active_file_reference.as_ref() {
            Some(reference) if reference.len() <= MAX_CONTEXT_BYTES => {
                format!("Active editor reference: {reference}\n\n{expanded_prompt}")
            }
            Some(_) => {
                return Err(ai_error(
                    AppErrorCode::TooLarge,
                    "The editor reference exceeds 128 KiB.",
                ));
            }
            None => expanded_prompt,
        };
        let (settings, current, history, credential) = {
            let state = self.lock();
            let current = state
                .conversations
                .get(conversation_id)
                .cloned()
                .ok_or_else(|| {
                    ai_error(AppErrorCode::NotFound, "The AI conversation was not found.")
                })?;
            let model = current
                .selected_model_id
                .as_deref()
                .unwrap_or(&state.settings.model);
            let provider = model
                .split_once('/')
                .map_or("custom", |(provider, _)| provider);
            (
                state.settings.clone(),
                current.clone(),
                state.histories.get(conversation_id).cloned(),
                state.credentials.get(provider).cloned().unwrap_or_default(),
            )
        };
        if credential.is_empty() {
            return Err(ai_error(
                AppErrorCode::PermissionDenied,
                "Add this provider's API key in Settings → Provider accounts.",
            ));
        }
        if history
            .as_ref()
            .is_some_and(|history| history.to_string().len() > MAX_CONVERSATION_BYTES)
        {
            return Err(ai_error(
                AppErrorCode::TooLarge,
                "This chat has reached the browser context limit. Start a new chat.",
            ));
        }
        let model = current.selected_model_id.clone().unwrap_or(settings.model);
        let available = self.list_models(workspace, conversation_id).await?;
        let selected = available
            .iter()
            .find(|candidate| candidate.id == model)
            .ok_or_else(|| ai_error(AppErrorCode::InvalidInput, "Choose an available model."))?;
        if !prompt.images.is_empty() && !selected.supports_images {
            return Err(ai_error(
                AppErrorCode::InvalidInput,
                "This model does not accept images.",
            ));
        }
        let user_id = self.id("user");
        let user = AiMessage {
            id: user_id.clone(),
            entry_id: Some(user_id),
            role: AiRole::User,
            content: prompt.text,
            images: prompt.images.clone(),
            ..AiMessage::default()
        };
        let request = BrowserStreamRequest {
            conversation_id: conversation_id.to_owned(),
            endpoint: settings.endpoint,
            credential,
            model,
            thinking_level: current.thinking_level,
            history,
            messages: current.messages,
            prompt: text,
            workspace_instructions,
            images: prompt.images,
            prior_tokens: current.usage.map_or(0, |usage| usage.total_tokens),
            max_response_bytes: MAX_RESPONSE_BYTES,
            max_response_events: MAX_RESPONSE_EVENTS,
        };
        self.lock().cancelled.remove(conversation_id);
        Ok(Box::new(BrowserAiEventStream {
            adapter: self.clone(),
            workspace: workspace.clone(),
            conversation_id: conversation_id.to_owned(),
            user,
            user_emitted: false,
            finished: false,
            request: Some(request),
            events: None,
        }))
    }

    async fn deliver(
        &self,
        _workspace: &WorkspaceRecord,
        _conversation_id: &str,
        _prompt: AiPrompt,
    ) -> Result<(), AppError> {
        Err(AppError::unsupported(
            "Steering and queued follow-ups require the connected Pi runtime.",
            ErrorSource::Ai,
        ))
    }

    async fn respond_to_extension(
        &self,
        _workspace: &WorkspaceRecord,
        _conversation_id: &str,
        _request_id: &str,
        _value: Option<String>,
        _confirmed: Option<bool>,
        _cancelled: bool,
    ) -> Result<(), AppError> {
        Err(AppError::unsupported(
            "Extension prompts require the connected Pi runtime.",
            ErrorSource::Ai,
        ))
    }

    async fn fork_at(
        &self,
        _workspace: &WorkspaceRecord,
        conversation_id: &str,
        entry_id: &str,
    ) -> Result<AiConversation, AppError> {
        let mut state = self.lock();
        let source = state
            .conversations
            .get(conversation_id)
            .cloned()
            .ok_or_else(|| {
                ai_error(AppErrorCode::NotFound, "The AI conversation was not found.")
            })?;
        let Some(index) = source
            .messages
            .iter()
            .position(|message| message.entry_id.as_deref() == Some(entry_id))
        else {
            return Err(ai_error(
                AppErrorCode::NotFound,
                "The message branch point was not found.",
            ));
        };
        let id = self.id("browser-chat");
        let prior_users = source.messages[..index]
            .iter()
            .filter(|message| message.role == AiRole::User)
            .count();
        if let Some(history) = state
            .histories
            .get(conversation_id)
            .and_then(serde_json::Value::as_array)
        {
            let cut = history
                .iter()
                .enumerate()
                .filter(|(_, message)| message["role"] == "user")
                .nth(prior_users)
                .map_or(history.len(), |(index, _)| index);
            let retained_history = serde_json::Value::Array(history[..cut].to_vec());
            state.histories.insert(id.clone(), retained_history);
        }
        let mut forked = source;
        forked.id.clone_from(&id);
        forked.title = format!("{} (branch)", forked.title);
        forked.messages.truncate(index);
        let retained = forked
            .messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        forked
            .item_order
            .retain(|item| retained.contains(item.as_str()));
        forked.activity.clear();
        forked.running = false;
        state.conversations.insert(id, forked.clone());
        Ok(forked)
    }

    async fn compact(
        &self,
        _workspace: &WorkspaceRecord,
        _conversation_id: &str,
        _custom_instructions: Option<String>,
    ) -> Result<Box<dyn AiEventStream>, AppError> {
        Err(AppError::unsupported(
            "Context compaction requires the connected Pi runtime.",
            ErrorSource::Ai,
        ))
    }

    async fn cancel(
        &self,
        _workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<(), AppError> {
        let mut state = self.lock();
        if !state.conversations.contains_key(conversation_id) {
            return Err(ai_error(
                AppErrorCode::NotFound,
                "The AI conversation was not found.",
            ));
        }
        state.cancelled.insert(conversation_id.to_owned());
        let cancel_bash = state.active_bash.as_deref() == Some(conversation_id);
        drop(state);
        abort_ai_request(conversation_id)?;
        if cancel_bash {
            let _ = syntaxis_terminal_browser::cancel();
        }
        Ok(())
    }

    async fn clone_conversation(
        &self,
        _workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<AiConversation, AppError> {
        let mut clone = self
            .lock()
            .conversations
            .get(conversation_id)
            .cloned()
            .ok_or_else(|| {
                ai_error(AppErrorCode::NotFound, "The AI conversation was not found.")
            })?;
        clone.id = self.id("browser-chat");
        clone.title = format!("{} branch", clone.title);
        {
            let mut state = self.lock();
            if let Some(history) = state.histories.get(conversation_id).cloned() {
                state.histories.insert(clone.id.clone(), history);
            }
        }
        self.lock()
            .conversations
            .insert(clone.id.clone(), clone.clone());
        Ok(clone)
    }

    async fn export_conversation(
        &self,
        _workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<(), AppError> {
        let conversation = self
            .lock()
            .conversations
            .get(conversation_id)
            .cloned()
            .ok_or_else(|| {
                ai_error(AppErrorCode::NotFound, "The AI conversation was not found.")
            })?;
        let mut body = String::new();
        for message in conversation.messages {
            let role = match message.role {
                AiRole::User => "You",
                AiRole::Assistant => "Assistant",
                AiRole::System => "System",
            };
            body.push_str("<article><h2>");
            body.push_str(role);
            body.push_str("</h2><pre>");
            body.push_str(&escape_html(&message.content));
            body.push_str("</pre></article>");
        }
        let title = escape_html(&conversation.title);
        let html = format!(
            "<!doctype html><meta charset=\"utf-8\"><title>{title}</title><style>body{{font:16px/1.5 system-ui;max-width:52rem;margin:3rem auto;padding:0 1rem}}article{{margin:0 0 2rem}}pre{{white-space:pre-wrap;font:inherit}}</style><h1>{title}</h1>{body}"
        );
        download_export(
            format!("{}.html", safe_filename(&conversation.title)),
            BASE64.encode(html.as_bytes()),
        );
        Ok(())
    }

    async fn rename(
        &self,
        _workspace: &WorkspaceRecord,
        conversation_id: &str,
        title: &str,
    ) -> Result<(), AppError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(ai_error(
                AppErrorCode::InvalidInput,
                "Enter a conversation name.",
            ));
        }
        let mut state = self.lock();
        let conversation = state
            .conversations
            .get_mut(conversation_id)
            .ok_or_else(|| {
                ai_error(AppErrorCode::NotFound, "The AI conversation was not found.")
            })?;
        conversation.title = title.to_owned();
        Ok(())
    }

    async fn delete(
        &self,
        _workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<(), AppError> {
        self.lock().histories.remove(conversation_id);
        if self.lock().conversations.remove(conversation_id).is_none() {
            return Err(ai_error(
                AppErrorCode::NotFound,
                "The AI conversation was not found.",
            ));
        }
        Ok(())
    }
}

struct EmptyAiEventStream;

#[async_trait(?Send)]
impl AiEventStream for EmptyAiEventStream {
    async fn receive(&mut self) -> Result<Option<AiEvent>, AppError> {
        Ok(None)
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn safe_filename(value: &str) -> String {
    let filename = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let filename = filename.trim_matches('-');
    if filename.is_empty() {
        "conversation".into()
    } else {
        filename.into()
    }
}

fn download_export(filename: String, data_base64: String) {
    dioxus::prelude::spawn(async move {
        let script = document::eval(
            r#"
            const filename = await dioxus.recv();
            const encoded = await dioxus.recv();
            const binary = atob(encoded);
            const bytes = new Uint8Array(binary.length);
            for (let index = 0; index < binary.length; index++) bytes[index] = binary.charCodeAt(index);
            const url = URL.createObjectURL(new Blob([bytes], { type: "text/html" }));
            const link = document.createElement("a");
            link.href = url;
            link.download = filename;
            link.style.display = "none";
            document.body.appendChild(link);
            link.click();
            link.remove();
            setTimeout(() => URL.revokeObjectURL(url), 1000);
            "#,
        );
        let _ = script.send(filename);
        let _ = script.send(data_base64);
    });
}

struct BrowserAiEventStream {
    adapter: BrowserAiAdapter,
    workspace: WorkspaceRecord,
    conversation_id: String,
    user: AiMessage,
    user_emitted: bool,
    finished: bool,
    request: Option<BrowserStreamRequest>,
    events: Option<dioxus::document::Eval>,
}

impl BrowserAiEventStream {
    fn project(&self, event: &AiEvent) {
        if let Some(conversation) = self
            .adapter
            .lock()
            .conversations
            .get_mut(&self.conversation_id)
        {
            apply_event_to_conversation(conversation, event);
        }
    }
}

#[async_trait(?Send)]
impl AiEventStream for BrowserAiEventStream {
    async fn receive(&mut self) -> Result<Option<AiEvent>, AppError> {
        if self.finished {
            return Ok(None);
        }
        if !self.user_emitted {
            let request = self
                .request
                .take()
                .ok_or_else(|| ai_error(AppErrorCode::Internal, "Missing Pi request."))?;
            self.events = Some(start_chat_stream(request)?);
            self.user_emitted = true;
            {
                let mut state = self.adapter.lock();
                if let Some(conversation) = state.conversations.get_mut(&self.conversation_id) {
                    if conversation.messages.is_empty() {
                        conversation.title = self.user.content.chars().take(48).collect();
                    }
                    conversation.running = true;
                    conversation.status_message = "Pi is working…".into();
                }
            }
            let event = AiEvent::UserMessage(self.user.clone());
            self.project(&event);
            return Ok(Some(event));
        }
        loop {
            let event = self
                .events
                .as_mut()
                .expect("Pi stream initialized")
                .recv::<BrowserStreamEvent>()
                .await
                .map_err(|_| {
                    ai_error(
                        AppErrorCode::Internal,
                        "The browser Pi stream ended unexpectedly.",
                    )
                })?;
            match event {
                BrowserStreamEvent::Event { event } => {
                    self.project(&event);
                    return Ok(Some(*event));
                }
                BrowserStreamEvent::Tool { id, name, args } => {
                    if name == "bash" {
                        self.adapter.lock().active_bash = Some(self.conversation_id.clone());
                    }
                    let cancelled = self
                        .adapter
                        .lock()
                        .cancelled
                        .contains(&self.conversation_id);
                    let result = if cancelled {
                        Err(ai_error(AppErrorCode::Cancelled, "Cancelled"))
                    } else {
                        super::ai_tools::execute(
                            &self.workspace,
                            self.adapter.workspace_events.clone(),
                            &name,
                            args,
                        )
                        .await
                    };
                    if name == "bash" {
                        self.adapter.lock().active_bash = None;
                    }
                    let response = match result {
                        Ok(output) => serde_json::json!({"id": id, "output": output}),
                        Err(error) => serde_json::json!({"id": id, "error": error.message}),
                    };
                    self.events
                        .as_ref()
                        .expect("Pi stream initialized")
                        .send(response)
                        .map_err(|_| {
                            ai_error(AppErrorCode::Internal, "Could not return the tool result.")
                        })?;
                }
                BrowserStreamEvent::History { history } => {
                    self.adapter
                        .lock()
                        .histories
                        .insert(self.conversation_id.clone(), history);
                }
                BrowserStreamEvent::Completed { history } => {
                    self.finished = true;
                    let mut state = self.adapter.lock();
                    state
                        .histories
                        .insert(self.conversation_id.clone(), history);
                    state.cancelled.remove(&self.conversation_id);
                    if let Some(conversation) = state.conversations.get_mut(&self.conversation_id) {
                        conversation.running = false;
                        if conversation.status_message == "Pi is working…" {
                            conversation.status_message = "Ready".into();
                        }
                    }
                    return Ok(None);
                }
            }
        }
    }
}

impl Drop for BrowserAiEventStream {
    fn drop(&mut self) {
        if !self.finished && self.events.is_some() {
            let _ = abort_ai_request(&self.conversation_id);
            let mut state = self.adapter.lock();
            if state.active_bash.as_deref() == Some(self.conversation_id.as_str()) {
                let _ = syntaxis_terminal_browser::cancel();
                state.active_bash = None;
            }
        }
        if let Some(conversation) = self
            .adapter
            .lock()
            .conversations
            .get_mut(&self.conversation_id)
        {
            conversation.running = false;
        }
    }
}

#[async_trait(?Send)]
impl AiModelPort for BrowserAiAdapter {
    async fn sync_preferences(
        &self,
        _workspace: &WorkspaceRecord,
        _available_models: Vec<String>,
    ) -> Result<syntaxis_module_ai::AiModelPreferences, AppError> {
        Ok(self.lock().model_preferences.clone())
    }

    async fn set_favourite(
        &self,
        _workspace: &WorkspaceRecord,
        model_id: &str,
        favourite: bool,
    ) -> Result<syntaxis_module_ai::AiModelPreferences, AppError> {
        let mut state = self.lock();
        state
            .model_preferences
            .favourites
            .retain(|id| id != model_id);
        if favourite {
            state.model_preferences.favourites.push(model_id.to_owned());
        }
        Ok(state.model_preferences.clone())
    }

    async fn remember_effort(
        &self,
        _workspace: &WorkspaceRecord,
        model_id: &str,
        level: AiThinkingLevel,
    ) -> Result<syntaxis_module_ai::AiModelPreferences, AppError> {
        let mut state = self.lock();
        state
            .model_preferences
            .efforts
            .insert(model_id.to_owned(), level);
        Ok(state.model_preferences.clone())
    }

    async fn list_models(
        &self,
        _workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<Vec<AiModel>, AppError> {
        ensure_bridge(BrowserBridge::Ai)
            .await
            .map_err(|message| ai_error(AppErrorCode::Internal, message))?;
        let settings = self.lock().settings.clone();
        let active_custom = self
            .lock()
            .conversations
            .get(conversation_id)
            .and_then(|conversation| conversation.selected_model_id.clone())
            .filter(|id| id.starts_with("custom/"));
        let mut eval = document::eval(
            r"
            const settings = await dioxus.recv();
            try {
                dioxus.send({models: globalThis.SyntaxisBrowserAi.cachedModels(settings.endpoint, settings.model, settings.credentials)});
            } catch (error) {
                dioxus.send({error: error?.message ?? 'Pi model catalog failed.'});
            }
        ",
        );
        let credentials = self.lock().credentials.clone();
        eval.send(serde_json::json!({"endpoint": settings.endpoint, "model": active_custom.unwrap_or(settings.model), "credentials": credentials}))
            .map_err(|_| ai_error(AppErrorCode::Internal, "Could not load Pi models."))?;
        let response: BrowserCatalogResponse = eval.recv().await.map_err(|error| {
            ai_error(
                AppErrorCode::Internal,
                format!("Could not decode Pi models: {error}"),
            )
        })?;
        response.models.ok_or_else(|| {
            ai_error(
                AppErrorCode::Internal,
                response
                    .error
                    .unwrap_or_else(|| "Pi returned no model catalog.".into()),
            )
        })
    }

    async fn refresh_models(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<Vec<AiModel>, AppError> {
        // Initialize the bridge and validate the current local catalog first.
        self.list_models(workspace, conversation_id).await?;
        let (settings, credentials) = {
            let state = self.lock();
            (state.settings.clone(), state.credentials.clone())
        };
        let mut eval = document::eval(
            r"
            const settings = await dioxus.recv();
            try {
                await globalThis.SyntaxisBrowserAi.availableModels(settings.endpoint, settings.model, settings.credentials);
                dioxus.send(null);
            } catch (error) {
                dioxus.send(error?.message ?? 'Model refresh failed.');
            }
        ",
        );
        eval.send(serde_json::json!({"endpoint": settings.endpoint, "model": settings.model, "credentials": credentials}))
            .map_err(|_| ai_error(AppErrorCode::Internal, "Could not refresh models."))?;
        let error: Option<String> = eval
            .recv()
            .await
            .map_err(|_| ai_error(AppErrorCode::Internal, "Could not receive model refresh."))?;
        if let Some(message) = error {
            return Err(ai_error(AppErrorCode::Internal, message));
        }
        self.list_models(workspace, conversation_id).await
    }

    async fn select_model(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
        model_id: &str,
    ) -> Result<(), AppError> {
        let models = self.list_models(workspace, conversation_id).await?;
        let model = models
            .iter()
            .find(|model| model.id == model_id)
            .ok_or_else(|| ai_error(AppErrorCode::InvalidInput, "Choose an available model."))?;
        let mut state = self.lock();
        let conversation = state
            .conversations
            .get_mut(conversation_id)
            .ok_or_else(|| {
                ai_error(AppErrorCode::NotFound, "The AI conversation was not found.")
            })?;
        conversation.selected_model_id = Some(model_id.to_owned());
        if !model.thinking_levels.contains(&conversation.thinking_level) {
            conversation.thinking_level = model
                .thinking_levels
                .first()
                .copied()
                .unwrap_or(AiThinkingLevel::Off);
        }
        Ok(())
    }

    async fn select_thinking_level(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
        level: AiThinkingLevel,
    ) -> Result<(), AppError> {
        let models = self.list_models(workspace, conversation_id).await?;
        let mut state = self.lock();
        let conversation = state
            .conversations
            .get_mut(conversation_id)
            .ok_or_else(|| {
                ai_error(AppErrorCode::NotFound, "The AI conversation was not found.")
            })?;
        if !models.iter().any(|model| {
            Some(&model.id) == conversation.selected_model_id.as_ref()
                && model.thinking_levels.contains(&level)
        }) {
            return Err(ai_error(
                AppErrorCode::InvalidInput,
                "This model does not support that reasoning effort.",
            ));
        }
        conversation.thinking_level = level;
        Ok(())
    }
}

#[async_trait(?Send)]
impl AiSettingsPort for BrowserAiAdapter {
    async fn load(&self, _workspace: &WorkspaceRecord) -> Result<AiProviderSettings, AppError> {
        let settings = &self.lock().settings;
        Ok(AiProviderSettings {
            endpoint: settings.endpoint.clone(),
            model: settings.model.clone(),
            credential: String::new(),
            credential_is_set: false,
            volatile_credential: true,
        })
    }

    async fn save(
        &self,
        workspace: &WorkspaceRecord,
        mut settings: AiProviderSettings,
    ) -> Result<(), AppError> {
        settings.endpoint = settings.endpoint.trim().to_owned();
        settings.model = settings.model.trim().to_owned();
        if settings.endpoint.len() > MAX_ENDPOINT_BYTES
            || settings.model.len() > MAX_MODEL_BYTES
            || settings.credential.len() > MAX_CREDENTIAL_BYTES
        {
            return Err(ai_error(
                AppErrorCode::TooLarge,
                "The AI provider settings exceed the browser limits.",
            ));
        }
        if !settings.endpoint.starts_with("https://") {
            return Err(ai_error(
                AppErrorCode::InvalidInput,
                "The provider endpoint must use HTTPS.",
            ));
        }
        if let Some(id) = settings.model.strip_prefix("custom/") {
            if id.trim().is_empty() {
                return Err(ai_error(
                    AppErrorCode::InvalidInput,
                    "Enter a custom model ID.",
                ));
            }
        } else if !self
            .list_models(workspace, "")
            .await?
            .iter()
            .any(|model| model.id == settings.model)
        {
            return Err(ai_error(
                AppErrorCode::InvalidInput,
                "Choose an available default model.",
            ));
        }
        let mut state = self.lock();
        settings.credential.clear();
        settings.credential_is_set = false;
        settings.volatile_credential = true;
        state.settings = settings;
        Ok(())
    }
}

#[derive(Deserialize)]
struct BrowserCatalogResponse {
    models: Option<Vec<AiModel>>,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserStreamRequest {
    workspace_instructions: String,
    conversation_id: String,
    endpoint: String,
    credential: String,
    model: String,
    thinking_level: AiThinkingLevel,
    history: Option<serde_json::Value>,
    messages: Vec<AiMessage>,
    prompt: String,
    images: Vec<syntaxis_module_ai::AiImageAttachment>,
    prior_tokens: u64,
    max_response_bytes: usize,
    max_response_events: usize,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum BrowserStreamEvent {
    Event {
        event: Box<AiEvent>,
    },
    Tool {
        id: String,
        name: String,
        args: serde_json::Value,
    },
    Completed {
        history: serde_json::Value,
    },
    History {
        history: serde_json::Value,
    },
}

fn start_chat_stream(request: BrowserStreamRequest) -> Result<dioxus::document::Eval, AppError> {
    let eval = document::eval(
        r"
        const request = await dioxus.recv();
        try {
            await globalThis.SyntaxisBrowserAi.run(request, dioxus);
        } catch (error) {
            await dioxus.send({kind: 'event', event: {type: 'failed', message: error?.message ?? 'Could not start Pi.'}});
            await dioxus.send({kind: 'completed', history: request.history ?? []});
        }
    ",
    );
    eval.send(request).map_err(|_| {
        ai_error(
            AppErrorCode::Internal,
            "Could not start the browser Pi stream.",
        )
    })?;
    Ok(eval)
}

fn abort_ai_request(conversation_id: &str) -> Result<(), AppError> {
    let eval = document::eval(
        r"
        const id = await dioxus.recv();
        globalThis.SyntaxisBrowserAi?.abort(id);
    ",
    );
    eval.send(conversation_id)
        .map_err(|_| ai_error(AppErrorCode::Internal, "Could not cancel Pi."))
}

fn ai_client_listener() -> dioxus::document::Eval {
    document::eval(
        r#"
        const id = await dioxus.recv();
        const forward = event => {
          const detail = event.detail ?? {};
          // Speech/paste events identify the composer; read-aloud events identify a message.
          if (event.type === "syntaxis-ai-read-aloud" || !detail.id || detail.id === id) {
            dioxus.send(detail);
          }
        };
        window.addEventListener("syntaxis-ai-paste", forward);
        window.addEventListener("syntaxis-ai-speech", forward);
        window.addEventListener("syntaxis-ai-read-aloud", forward);
        dioxus.send({
          kind: "availability",
          available: "speechSynthesis" in window && "SpeechSynthesisUtterance" in window,
        });
        await dioxus.recv();
        window.removeEventListener("syntaxis-ai-paste", forward);
        window.removeEventListener("syntaxis-ai-speech", forward);
        window.removeEventListener("syntaxis-ai-read-aloud", forward);
        "#,
    )
}

async fn load_client_state(key: &str) -> Result<Option<String>, AppError> {
    let mut eval = document::eval(
        r#"
        const key = await dioxus.recv();
        try { await dioxus.send(localStorage.getItem(key)); }
        catch { await dioxus.send(null); }
        "#,
    );
    eval.send(key).map_err(|error| {
        ai_error(
            AppErrorCode::Internal,
            format!("Could not read AI client state: {error}"),
        )
    })?;
    eval.recv::<Option<String>>().await.map_err(|error| {
        ai_error(
            AppErrorCode::Internal,
            format!("Could not read AI client state: {error}"),
        )
    })
}

async fn save_client_state(key: &str, value: Option<&str>) -> Result<(), AppError> {
    let mut eval = document::eval(
        r#"
        const key = await dioxus.recv();
        const value = await dioxus.recv();
        try {
          if (value === null) localStorage.removeItem(key);
          else localStorage.setItem(key, value);
          await dioxus.send(null);
        } catch (error) {
          await dioxus.send(error?.message ?? String(error));
        }
        "#,
    );
    eval.send(key)
        .and_then(|()| eval.send(value.map(str::to_owned)))
        .map_err(|error| {
            ai_error(
                AppErrorCode::Internal,
                format!("Could not save AI client state: {error}"),
            )
        })?;
    match eval.recv::<Option<String>>().await {
        Ok(None) => Ok(()),
        Ok(Some(message)) => Err(ai_error(AppErrorCode::Internal, message)),
        Err(error) => Err(ai_error(
            AppErrorCode::Internal,
            format!("Could not save AI client state: {error}"),
        )),
    }
}

async fn copy_client_text(value: &str) -> Result<(), AppError> {
    let mut eval = document::eval(
        r#"
        const value = await dioxus.recv();
        try {
          if (navigator.clipboard?.writeText) await navigator.clipboard.writeText(value);
          else {
            const input = document.createElement("textarea");
            input.value = value;
            input.style.position = "fixed";
            input.style.opacity = "0";
            document.body.appendChild(input);
            input.select();
            if (!document.execCommand("copy")) throw new Error("The browser rejected the copy command.");
            input.remove();
          }
          await dioxus.send(null);
        } catch (error) {
          await dioxus.send(error?.message ?? String(error));
        }
        "#,
    );
    eval.send(value).map_err(|error| {
        ai_error(
            AppErrorCode::Internal,
            format!("Could not copy the AI message: {error}"),
        )
    })?;
    match eval.recv::<Option<String>>().await {
        Ok(None) => Ok(()),
        Ok(Some(message)) => Err(ai_error(AppErrorCode::Internal, message)),
        Err(error) => Err(ai_error(
            AppErrorCode::Internal,
            format!("Could not copy the AI message: {error}"),
        )),
    }
}

async fn call_ai_client(method: &str, id: &str) -> Result<(), AppError> {
    let mut eval = document::eval(
        r#"
        const method = await dioxus.recv();
        const id = await dioxus.recv();
        try {
          const action = globalThis.SyntaxisAiChat?.[method];
          if (typeof action !== "function") throw new Error("AI client controls are unavailable.");
          action(id);
          await dioxus.send(null);
        } catch (error) {
          await dioxus.send(error?.message ?? String(error));
        }
        "#,
    );
    eval.send(method)
        .and_then(|()| eval.send(id))
        .map_err(|error| {
            ai_error(
                AppErrorCode::Internal,
                format!("Could not use the AI client control: {error}"),
            )
        })?;
    match eval.recv::<Option<String>>().await {
        Ok(None) => Ok(()),
        Ok(Some(message)) => Err(ai_error(AppErrorCode::Internal, message)),
        Err(error) => Err(ai_error(
            AppErrorCode::Internal,
            format!("Could not use the AI client control: {error}"),
        )),
    }
}

fn ai_error(code: AppErrorCode, message: impl Into<String>) -> AppError {
    let retry = match code {
        AppErrorCode::Offline | AppErrorCode::RateLimited | AppErrorCode::Internal => {
            RetryAdvice::Backoff
        }
        AppErrorCode::PermissionDenied => RetryAdvice::AfterUserAction,
        _ => RetryAdvice::Never,
    };
    AppError::new(code, message, retry, ErrorSource::Ai)
}
