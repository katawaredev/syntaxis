use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use async_trait::async_trait;
use dioxus::prelude::document;
use serde::{Deserialize, Serialize};
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, RetryAdvice};
use syntaxis_module_ai::{
    AiConversation, AiConversationPort, AiConversationSummary, AiEvent, AiEventStream, AiMessage,
    AiModel, AiModelPort, AiPrompt, AiProviderSettings, AiRole, AiSettingsPort,
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
            })
            .collect())
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
            selected_model_id: Some(self.lock().settings.model.clone()),
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
            id: user_id,
            role: AiRole::User,
            content: display_text,
        };
        let assistant = AiMessage {
            id: self.id("assistant"),
            role: AiRole::Assistant,
            content: String::new(),
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
