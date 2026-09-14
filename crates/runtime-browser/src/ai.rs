use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use async_trait::async_trait;
#[cfg(target_arch = "wasm32")]
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use dioxus::prelude::document;
use serde::{Deserialize, Serialize};
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, RetryAdvice};
use syntaxis_module_ai::{
    AiClientEvent, AiClientEventStream, AiClientPort, AiConversation, AiConversationMatch,
    AiConversationPort, AiConversationSummary, AiEvent, AiEventStream, AiMessage, AiModel,
    AiModelPort, AiPrompt, AiProviderSettings, AiRole, AiSettingsPort, AiThinkingLevel,
};
use syntaxis_workspace::WorkspaceRecord;
const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";
const DEFAULT_MODEL: &str = "gpt-4.1-mini";
const MAX_PROMPT_BYTES: usize = 64 * 1024;
const MAX_CONTEXT_BYTES: usize = 128 * 1024;
const MAX_CONVERSATION_BYTES: usize = 512 * 1024;
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
}

struct BrowserAiState {
    settings: AiProviderSettings,
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
                conversations: HashMap::new(),
                cancelled: HashSet::new(),
            })),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }
}

impl BrowserAiAdapter {
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
                status_message: "Ready".into(),
                running: false,
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

    async fn create(&self, _workspace: &WorkspaceRecord) -> Result<AiConversation, AppError> {
        if self.lock().conversations.len() >= MAX_CONVERSATIONS {
            return Err(ai_error(
                AppErrorCode::TooLarge,
                "The browser AI conversation limit has been reached.",
            ));
        }
        let conversation = AiConversation {
            id: self.id("browser-chat"),
            title: "New chat".into(),
            messages: Vec::new(),
            activity: Vec::new(),
            item_order: Vec::new(),
            selected_model_id: Some(self.lock().settings.model.clone()),
            thinking_level: AiThinkingLevel::Off,
            usage: None,
            running: false,
            status_message: "Ready".into(),
            pending_messages: 0,
            steering_queue: Vec::new(),
            follow_up_queue: Vec::new(),
            commands: Vec::new(),
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
        _workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<AiConversation, AppError> {
        self.lock()
            .conversations
            .get(conversation_id)
            .cloned()
            .ok_or_else(|| ai_error(AppErrorCode::NotFound, "The AI conversation was not found."))
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
        _workspace: &WorkspaceRecord,
        conversation_id: &str,
        prompt: AiPrompt,
    ) -> Result<Box<dyn AiEventStream>, AppError> {
        self.lock().cancelled.remove(conversation_id);
        if prompt.text.len() > MAX_PROMPT_BYTES {
            return Err(ai_error(
                AppErrorCode::TooLarge,
                "The prompt exceeds the 64 KiB browser limit.",
            ));
        }
        if !prompt.images.is_empty() {
            return Err(AppError::unsupported(
                "Image prompts are unavailable with the browser AI provider.",
                ErrorSource::Ai,
            ));
        }
        let display_text = prompt.text.clone();
        let content = match prompt.active_file_reference {
            Some(reference) if reference.len() <= MAX_CONTEXT_BYTES => {
                format!(
                    "Active editor reference: {reference}\n\nRequest: {}",
                    prompt.text
                )
            }
            Some(_) => {
                return Err(ai_error(
                    AppErrorCode::TooLarge,
                    "The editor reference exceeds the 128 KiB browser limit.",
                ));
            }
            None => prompt.text,
        };
        let (settings, mut request_messages) = {
            let state = self.lock();
            let conversation = state.conversations.get(conversation_id).ok_or_else(|| {
                ai_error(AppErrorCode::NotFound, "The AI conversation was not found.")
            })?;
            (state.settings.clone(), conversation.messages.clone())
        };
        let user_id = self.id("user");
        request_messages.push(AiMessage {
            id: user_id.clone(),
            role: AiRole::User,
            content: content.clone(),
            entry_id: Some(user_id.clone()),
            ..AiMessage::default()
        });
        while request_messages
            .iter()
            .map(|message| message.content.len())
            .sum::<usize>()
            > MAX_CONVERSATION_BYTES
            && request_messages.len() > 1
        {
            request_messages.remove(0);
        }
        if request_messages
            .iter()
            .map(|message| message.content.len())
            .sum::<usize>()
            > MAX_CONVERSATION_BYTES
        {
            return Err(ai_error(
                AppErrorCode::TooLarge,
                "The AI conversation exceeds the 512 KiB browser limit.",
            ));
        }
        if settings.credential.is_empty() {
            return Err(ai_error(
                AppErrorCode::PermissionDenied,
                "Enter an API key in AI settings.",
            ));
        }
        let user = AiMessage {
            id: user_id.clone(),
            role: AiRole::User,
            content: display_text,
            entry_id: Some(user_id),
            ..AiMessage::default()
        };
        let assistant = AiMessage {
            id: self.id("assistant"),
            role: AiRole::Assistant,
            content: String::new(),
            status: syntaxis_module_ai::AiMessageStatus::Streaming,
            ..AiMessage::default()
        };
        let request = chat_stream_request(
            conversation_id,
            &settings,
            &request_messages,
            MAX_RESPONSE_BYTES,
            MAX_RESPONSE_EVENTS,
        )?;
        Ok(Box::new(BrowserAiEventStream {
            adapter: self.clone(),
            conversation_id: conversation_id.to_owned(),
            user,
            assistant,
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
        drop(state);
        abort_ai_request(conversation_id)?;
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
    conversation_id: String,
    user: AiMessage,
    assistant: AiMessage,
    user_emitted: bool,
    finished: bool,
    request: Option<BrowserStreamRequest>,
    events: Option<dioxus::document::Eval>,
}

#[async_trait(?Send)]
impl AiEventStream for BrowserAiEventStream {
    async fn receive(&mut self) -> Result<Option<AiEvent>, AppError> {
        if !self.user_emitted {
            self.user_emitted = true;
            return Ok(Some(AiEvent::UserMessage(self.user.clone())));
        }
        if self.finished {
            return Ok(None);
        }
        if self.adapter.lock().cancelled.remove(&self.conversation_id) {
            self.finished = true;
            return Err(ai_error(
                AppErrorCode::Cancelled,
                "The AI request was cancelled.",
            ));
        }
        if self.events.is_none() {
            let request = self.request.take().ok_or_else(|| {
                ai_error(
                    AppErrorCode::Internal,
                    "The browser AI request was not initialized.",
                )
            })?;
            self.events = Some(start_chat_stream(request)?);
        }
        let event = self
            .events
            .as_mut()
            .expect("the browser AI event bridge was initialized")
            .recv::<BrowserStreamEvent>()
            .await
            .map_err(|_| {
                ai_error(
                    AppErrorCode::Internal,
                    "The browser AI stream ended unexpectedly.",
                )
            })?;
        match event {
            BrowserStreamEvent::Delta { text } => {
                self.assistant.content.push_str(&text);
                if self.assistant.content.len() > MAX_RESPONSE_BYTES {
                    self.finished = true;
                    let _ = abort_ai_request(&self.conversation_id);
                    return Err(ai_error(
                        AppErrorCode::TooLarge,
                        "The provider response exceeds the 1 MiB browser limit.",
                    ));
                }
                Ok(Some(AiEvent::AssistantDelta {
                    message_id: self.assistant.id.clone(),
                    text,
                }))
            }
            BrowserStreamEvent::Completed => {
                self.finished = true;
                if let Some(conversation) = self
                    .adapter
                    .lock()
                    .conversations
                    .get_mut(&self.conversation_id)
                {
                    if conversation.messages.is_empty() {
                        conversation.title = self.user.content.chars().take(48).collect();
                    }
                    conversation.messages.push(self.user.clone());
                    conversation.messages.push(self.assistant.clone());
                    conversation.item_order.push(self.user.id.clone());
                    conversation.item_order.push(self.assistant.id.clone());
                }
                Ok(Some(AiEvent::AssistantCompleted(self.assistant.clone())))
            }
            BrowserStreamEvent::Failed { code, message } => {
                self.finished = true;
                match code {
                    BrowserStreamErrorCode::Cancelled => {
                        Err(ai_error(AppErrorCode::Cancelled, message))
                    }
                    BrowserStreamErrorCode::TooLarge => {
                        Err(ai_error(AppErrorCode::TooLarge, message))
                    }
                    BrowserStreamErrorCode::RateLimited => {
                        Err(ai_error(AppErrorCode::RateLimited, message))
                    }
                    BrowserStreamErrorCode::Offline => Ok(Some(AiEvent::Failed { message })),
                    BrowserStreamErrorCode::Internal => {
                        Err(ai_error(AppErrorCode::Internal, message))
                    }
                }
            }
        }
    }
}

impl Drop for BrowserAiEventStream {
    fn drop(&mut self) {
        if !self.finished && self.events.is_some() {
            let _ = abort_ai_request(&self.conversation_id);
        }
    }
}

#[async_trait(?Send)]
impl AiModelPort for BrowserAiAdapter {
    async fn list_models(
        &self,
        _workspace: &WorkspaceRecord,
        _conversation_id: &str,
    ) -> Result<Vec<AiModel>, AppError> {
        let model = self.lock().settings.model.clone();
        Ok(vec![AiModel {
            id: model.clone(),
            label: model,
            provider: "Browser provider".into(),
            reasoning: false,
            thinking_levels: vec![AiThinkingLevel::Off],
            supports_images: false,
            context_window: 0,
            max_tokens: 0,
            cost: syntaxis_module_ai::AiModelCost::default(),
        }])
    }

    async fn select_model(
        &self,
        _workspace: &WorkspaceRecord,
        conversation_id: &str,
        model_id: &str,
    ) -> Result<(), AppError> {
        if model_id.trim().is_empty() {
            return Err(ai_error(AppErrorCode::InvalidInput, "Enter a model name."));
        }
        let mut state = self.lock();
        state.settings.model = model_id.trim().to_owned();
        if let Some(conversation) = state.conversations.get_mut(conversation_id) {
            conversation.selected_model_id = Some(model_id.trim().to_owned());
        }
        Ok(())
    }

    async fn select_thinking_level(
        &self,
        _workspace: &WorkspaceRecord,
        conversation_id: &str,
        level: AiThinkingLevel,
    ) -> Result<(), AppError> {
        if level != AiThinkingLevel::Off {
            return Err(AppError::unsupported(
                "Reasoning effort is unavailable with the browser AI provider.",
                ErrorSource::Ai,
            ));
        }
        let mut state = self.lock();
        let conversation = state
            .conversations
            .get_mut(conversation_id)
            .ok_or_else(|| {
                ai_error(AppErrorCode::NotFound, "The AI conversation was not found.")
            })?;
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
            credential_is_set: !settings.credential.is_empty(),
            volatile_credential: true,
        })
    }

    async fn save(
        &self,
        _workspace: &WorkspaceRecord,
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
        if settings.model.is_empty() {
            return Err(ai_error(AppErrorCode::InvalidInput, "Enter a model name."));
        }
        let mut state = self.lock();
        if settings.credential.is_empty() && settings.credential_is_set {
            settings.credential = state.settings.credential.clone();
        }
        settings.credential_is_set = !settings.credential.is_empty();
        settings.volatile_credential = true;
        state.settings = settings;
        Ok(())
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatRequestMessage<'a>>,
    stream: bool,
}

#[derive(Serialize)]
struct ChatRequestMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserStreamRequest {
    conversation_id: String,
    endpoint: String,
    credential: String,
    body: String,
    max_response_bytes: usize,
    max_response_events: usize,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum BrowserStreamEvent {
    Delta {
        text: String,
    },
    Completed,
    Failed {
        code: BrowserStreamErrorCode,
        message: String,
    },
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BrowserStreamErrorCode {
    Cancelled,
    TooLarge,
    RateLimited,
    Offline,
    Internal,
}

fn chat_stream_request(
    conversation_id: &str,
    settings: &AiProviderSettings,
    messages: &[AiMessage],
    max_response_bytes: usize,
    max_response_events: usize,
) -> Result<BrowserStreamRequest, AppError> {
    let request_messages = messages
        .iter()
        .map(|message| ChatRequestMessage {
            role: match message.role {
                AiRole::User => "user",
                AiRole::Assistant => "assistant",
                AiRole::System => "system",
            },
            content: &message.content,
        })
        .collect();
    let body = serde_json::to_string(&ChatRequest {
        model: &settings.model,
        messages: request_messages,
        stream: true,
    })
    .map_err(|error| ai_error(AppErrorCode::Internal, error.to_string()))?;
    Ok(BrowserStreamRequest {
        conversation_id: conversation_id.to_owned(),
        endpoint: settings.endpoint.clone(),
        credential: settings.credential.clone(),
        body,
        max_response_bytes,
        max_response_events,
    })
}

fn start_chat_stream(request: BrowserStreamRequest) -> Result<dioxus::document::Eval, AppError> {
    let eval = document::eval(
        r#"
        const request = await dioxus.recv();
        const requests = globalThis.__syntaxisAiRequests ??= new Map();
        requests.get(request.conversationId)?.abort();
        const controller = new AbortController();
        requests.set(request.conversationId, controller);
        let responseEvents = 0;
        const send = async event => {
          responseEvents += 1;
          if (responseEvents > request.maxResponseEvents) {
            const error = new Error("The provider response exceeded the browser event limit.");
            error.syntaxisCode = "too_large";
            throw error;
          }
          await dioxus.send(event);
        };
        const sendDelta = async text => {
          if (typeof text === "string" && text.length > 0) {
            await send({ kind: "delta", text });
          }
        };
        try {
          const response = await fetch(request.endpoint, {
            method: "POST",
            mode: "cors",
            headers: {
              "Content-Type": "application/json",
              Authorization: `Bearer ${request.credential}`,
            },
            body: request.body,
            signal: controller.signal,
          });
          if (!response.ok) {
            await send({
              kind: "failed",
              code: response.status === 429 ? "rate_limited" : "offline",
              message: `Provider returned HTTP ${response.status}.`,
            });
            return;
          }
          const declaredSize = Number(response.headers.get("Content-Length"));
          if (Number.isFinite(declaredSize) && declaredSize > request.maxResponseBytes) {
            controller.abort();
            await send({
              kind: "failed",
              code: "too_large",
              message: "The provider response exceeds the 1 MiB browser limit.",
            });
            return;
          }
          if (!response.body) {
            await send({
              kind: "failed",
              code: "internal",
              message: "The provider response did not expose a readable body.",
            });
            return;
          }
          const reader = response.body.getReader();
          const decoder = new TextDecoder();
          const eventStream = response.headers
            .get("Content-Type")
            ?.toLowerCase()
            .includes("text/event-stream");
          let pending = "";
          let receivedBytes = 0;
          let completed = false;
          const consumeEvent = async block => {
            const data = block
              .split(/\r?\n/)
              .filter(line => line.startsWith("data:"))
              .map(line => line.slice(5).trimStart())
              .join("\n")
              .trim();
            if (!data) return false;
            if (data === "[DONE]") return true;
            let payload;
            try {
              payload = JSON.parse(data);
            } catch {
              const error = new Error("The provider returned an invalid streaming event.");
              error.syntaxisCode = "internal";
              throw error;
            }
            await sendDelta(payload.choices?.[0]?.delta?.content);
            return false;
          };
          while (!completed) {
            const chunk = await reader.read();
            if (chunk.done) break;
            receivedBytes += chunk.value.byteLength;
            if (receivedBytes > request.maxResponseBytes) {
              controller.abort();
              const error = new Error("The provider response exceeds the 1 MiB browser limit.");
              error.syntaxisCode = "too_large";
              throw error;
            }
            pending += decoder.decode(chunk.value, { stream: true });
            if (!eventStream) continue;
            let boundary = pending.match(/\r?\n\r?\n/);
            while (boundary?.index !== undefined) {
              const block = pending.slice(0, boundary.index);
              pending = pending.slice(boundary.index + boundary[0].length);
              completed = await consumeEvent(block);
              if (completed) {
                await reader.cancel();
                break;
              }
              boundary = pending.match(/\r?\n\r?\n/);
            }
          }
          pending += decoder.decode();
          if (eventStream) {
            if (!completed && pending.trim()) completed = await consumeEvent(pending);
          } else {
            let payload;
            try {
              payload = JSON.parse(pending);
            } catch {
              const error = new Error("The provider returned an incompatible response.");
              error.syntaxisCode = "internal";
              throw error;
            }
            await sendDelta(payload.choices?.[0]?.message?.content);
          }
          await send({ kind: "completed" });
        } catch (error) {
          const cancelled = controller.signal.aborted && error?.syntaxisCode !== "too_large";
          await dioxus.send({
            kind: "failed",
            code: cancelled ? "cancelled" : (error?.syntaxisCode ?? "offline"),
            message: cancelled
              ? "The AI request was cancelled."
              : (error?.message ?? "The provider request failed."),
          });
        } finally {
          if (requests.get(request.conversationId) === controller) {
            requests.delete(request.conversationId);
          }
        }
        "#,
    );
    eval.send(request).map_err(|_| {
        ai_error(
            AppErrorCode::Internal,
            "Could not start the browser AI stream.",
        )
    })?;
    Ok(eval)
}

fn abort_ai_request(conversation_id: &str) -> Result<(), AppError> {
    let eval = document::eval(
        r#"
        const conversationId = await dioxus.recv();
        globalThis.__syntaxisAiRequests?.get(conversationId)?.abort();
        "#,
    );
    eval.send(conversation_id).map_err(|_| {
        ai_error(
            AppErrorCode::Internal,
            "Could not cancel the browser AI request.",
        )
    })
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
