#![allow(
    clippy::clone_on_ref_ptr,
    clippy::too_many_lines,
    reason = "Runtime registration fans one adapter into the AI capability ports"
)]

use async_trait::async_trait;
use dioxus::{fullstack::WebSocketOptions, prelude::document};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};
use syntaxis_agent::{
    AgentSnapshot, AgentStatus, ChatItem, ClientMessage, ImageAttachment, ItemStatus,
    MAX_IMAGE_BYTES, MAX_PROMPT_IMAGES, MAX_TOTAL_IMAGE_BYTES, PROTOCOL_VERSION, PromptDelivery,
    ServerMessage, ThinkingLevel,
};
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, PortHandle, RetryAdvice};
use syntaxis_git::{WorktreeCreateRequest, WorktreeInfo};
use syntaxis_module_ai::{
    AiActivity, AiAdvancedSettings, AiAuthEvent, AiAuthFlow, AiAuthPrompt, AiAuthPromptOption,
    AiClientEvent, AiClientEventStream, AiClientPort, AiCommand, AiConversation,
    AiConversationMatch, AiConversationPort, AiConversationSummary, AiEvent, AiEventStream,
    AiExtension, AiExtensionAction, AiExtensionPage, AiExtensionRequest, AiExtensionWidget,
    AiExtensionsPort, AiFeatureSummary, AiGeneralSetting, AiGeneralSettingKind,
    AiGeneralSettingsPort, AiImageAttachment, AiManagedFeature, AiManagedFeaturePort, AiMessage,
    AiMessageStatus, AiModel, AiModelCost, AiModelPort, AiModelPreferences, AiPorts, AiPrompt,
    AiPromptDelivery, AiPromptTemplate, AiProviderAccount, AiProviderAuthKind,
    AiProviderAuthMethod, AiProviderAuthPort, AiResourceScope, AiResourcesPort, AiRole, AiSkill,
    AiSkillCatalogView, AiSkillSearchPage, AiSkillSearchResult, AiThinkingLevel, AiUsage,
    AiWorktreePort,
};
use syntaxis_workspace::WorkspaceRecord;

use super::api;

const MAX_PROMPT_BYTES: usize = 64 * 1024;
const MAX_CONTEXT_BYTES: usize = 128 * 1024;
const MAX_CONVERSATION_EVENTS: usize = 10_000;
const MAX_ASSISTANT_BYTES: usize = 1024 * 1024;
const MAX_ACCUMULATED_EVENT_BYTES: usize = 4 * 1024 * 1024;
static NEXT_LOCAL_MESSAGE_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_ACTION_NONCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Default)]
struct DioxusAi;

pub(crate) fn ai_ports() -> AiPorts {
    let adapter = PortHandle::new(DioxusAi);
    AiPorts::default()
        .with_conversation(adapter.clone())
        .with_models(adapter.clone())
        .with_general_settings(adapter.clone())
        .with_usage(adapter.clone())
        .with_provider_auth(adapter.clone())
        .with_resources(adapter.clone())
        .with_extensions(adapter.clone())
        .with_worktrees(adapter.clone())
        .with_notifications(adapter.clone())
        .with_client(adapter)
}

struct ClientAiEventStream {
    events: dioxus::document::Eval,
}

impl Drop for ClientAiEventStream {
    fn drop(&mut self) {
        let _ = self.events.send(true);
    }
}

#[async_trait(?Send)]
impl AiClientEventStream for ClientAiEventStream {
    async fn receive(&mut self) -> Result<Option<AiClientEvent>, AppError> {
        match self.events.recv::<AiClientEvent>().await {
            Ok(event) => Ok(Some(event)),
            Err(_) => Ok(None),
        }
    }
}

#[async_trait(?Send)]
impl AiClientPort for DioxusAi {
    async fn listen(&self, composer_id: &str) -> Result<Box<dyn AiClientEventStream>, AppError> {
        let events = ai_client_listener();
        events
            .send(composer_id)
            .map_err(|error| agent_error(format!("Could not start AI client events: {error}")))?;
        Ok(Box::new(ClientAiEventStream { events }))
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
impl AiConversationPort for DioxusAi {
    fn supports_compaction(&self) -> bool {
        true
    }

    async fn list(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<Vec<AiConversationSummary>, AppError> {
        let socket = connect(workspace).await?;
        loop {
            match socket.recv().await.map_err(socket_error)? {
                ServerMessage::Sessions { sessions } => {
                    return Ok(sessions
                        .into_iter()
                        .map(|session| AiConversationSummary {
                            id: session.id,
                            title: session.title,
                            message_count: 0,
                            updated_at_ms: session.updated_at_ms,
                            status_message: session.status_message,
                            running: session.running,
                        })
                        .collect());
                }
                ServerMessage::Error { error } => return Err(agent_error(error.message)),
                _ => {}
            }
        }
    }

    async fn search(
        &self,
        workspace: &WorkspaceRecord,
        query: &str,
    ) -> Result<Vec<AiConversationMatch>, AppError> {
        api::search_conversations(workspace.id.0.clone(), query.to_owned())
            .await
            .map(|results| {
                results
                    .into_iter()
                    .map(|result| AiConversationMatch {
                        session_id: result.session_id,
                        title: result.title,
                        updated_at_ms: result.updated_at_ms,
                        role: match result.role {
                            syntaxis_agent::ConversationMatchRole::User => AiRole::User,
                            syntaxis_agent::ConversationMatchRole::Assistant => AiRole::Assistant,
                        },
                        snippet: result.snippet,
                        match_count: result.match_count,
                    })
                    .collect()
            })
            .map_err(management_error)
    }

    async fn create(&self, workspace: &WorkspaceRecord) -> Result<AiConversation, AppError> {
        let socket = connect(workspace).await?;
        socket
            .send(ClientMessage::CreateSession)
            .await
            .map_err(socket_error)?;
        loop {
            match socket.recv().await.map_err(socket_error)? {
                ServerMessage::SelectedSession {
                    session_id,
                    snapshot,
                } => {
                    return Ok(conversation_from_snapshot(session_id, snapshot));
                }
                ServerMessage::Error { error } => return Err(agent_error(error.message)),
                _ => {}
            }
        }
    }

    async fn open(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<AiConversation, AppError> {
        let socket = connect(workspace).await?;
        socket
            .send(ClientMessage::SelectSession {
                session_id: conversation_id.to_owned(),
            })
            .await
            .map_err(socket_error)?;
        loop {
            match socket.recv().await.map_err(socket_error)? {
                ServerMessage::SelectedSession {
                    session_id,
                    snapshot,
                } => {
                    return Ok(conversation_from_snapshot(session_id, snapshot));
                }
                ServerMessage::Error { error } => return Err(agent_error(error.message)),
                _ => {}
            }
        }
    }

    async fn watch(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<Box<dyn AiEventStream>, AppError> {
        let socket = connect(workspace).await?;
        socket
            .send(ClientMessage::SelectSession {
                session_id: conversation_id.to_owned(),
            })
            .await
            .map_err(socket_error)?;
        let snapshot = wait_for_snapshot(&socket, conversation_id).await?;
        let running = matches!(
            snapshot.status,
            AgentStatus::Starting | AgentStatus::Working | AgentStatus::Compacting
        );
        let assistant = snapshot.items.iter().rev().find_map(|item| match item {
            ChatItem::Assistant { .. } => message_from_item(item.clone()),
            _ => None,
        });
        Ok(Box::new(RemoteAiEventStream {
            socket,
            conversation_id: conversation_id.to_owned(),
            pending: None,
            assistant,
            received: 0,
            accumulated_event_bytes: 0,
            activity_bytes: HashMap::new(),
            finished: !running,
        }))
    }

    async fn send(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
        prompt: AiPrompt,
    ) -> Result<Box<dyn AiEventStream>, AppError> {
        if prompt.text.len() > MAX_PROMPT_BYTES {
            return Err(AppError::new(
                AppErrorCode::TooLarge,
                "The prompt exceeds the 64 KiB limit.",
                RetryAdvice::Never,
                ErrorSource::Ai,
            ));
        }
        if prompt
            .active_file_reference
            .as_ref()
            .is_some_and(|reference| reference.len() > MAX_CONTEXT_BYTES)
        {
            return Err(AppError::new(
                AppErrorCode::TooLarge,
                "The editor reference exceeds the 128 KiB limit.",
                RetryAdvice::Never,
                ErrorSource::Ai,
            ));
        }
        if prompt.images.len() > MAX_PROMPT_IMAGES
            || prompt
                .images
                .iter()
                .any(|image| image.size > MAX_IMAGE_BYTES)
            || prompt.images.iter().map(|image| image.size).sum::<u64>() > MAX_TOTAL_IMAGE_BYTES
        {
            return Err(AppError::new(
                AppErrorCode::TooLarge,
                "Attach up to 5 images, 8 MiB each and 16 MiB total.",
                RetryAdvice::Never,
                ErrorSource::Ai,
            ));
        }
        let socket = connect(workspace).await?;
        socket
            .send(ClientMessage::SelectSession {
                session_id: conversation_id.to_owned(),
            })
            .await
            .map_err(socket_error)?;
        wait_for_selection(&socket, conversation_id).await?;
        let text = prompt
            .active_file_reference
            .map_or(prompt.text.clone(), |reference| {
                format!(
                    "Active editor reference: {reference}\n\nRequest: {}",
                    prompt.text
                )
            });
        socket
            .send(ClientMessage::SessionAction {
                session_id: conversation_id.to_owned(),
                action: Box::new(ClientMessage::Prompt {
                    text: text.clone(),
                    delivery: match prompt.delivery {
                        AiPromptDelivery::Prompt => PromptDelivery::Prompt,
                        AiPromptDelivery::Steer => PromptDelivery::Steer,
                        AiPromptDelivery::FollowUp => PromptDelivery::FollowUp,
                    },
                    images: prompt
                        .images
                        .iter()
                        .map(|image| ImageAttachment {
                            name: image.name.clone(),
                            mime_type: image.mime_type.clone(),
                            size: image.size,
                            data: image.data.clone(),
                        })
                        .collect(),
                }),
            })
            .await
            .map_err(socket_error)?;
        let user = AiEvent::UserMessage(AiMessage {
            id: format!(
                "user-{conversation_id}-{}",
                NEXT_LOCAL_MESSAGE_ID.fetch_add(1, Ordering::Relaxed),
            ),
            role: AiRole::User,
            content: prompt.text,
            images: prompt.images,
            ..AiMessage::default()
        });
        Ok(Box::new(RemoteAiEventStream {
            socket,
            conversation_id: conversation_id.to_owned(),
            pending: Some(user),
            assistant: None,
            received: 0,
            accumulated_event_bytes: 0,
            activity_bytes: HashMap::new(),
            finished: false,
        }))
    }

    async fn deliver(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
        prompt: AiPrompt,
    ) -> Result<(), AppError> {
        validate_prompt(&prompt)?;
        let socket = connect(workspace).await?;
        socket
            .send(ClientMessage::SelectSession {
                session_id: conversation_id.to_owned(),
            })
            .await
            .map_err(socket_error)?;
        wait_for_selection(&socket, conversation_id).await?;
        let text = prompt
            .active_file_reference
            .map_or(prompt.text.clone(), |reference| {
                format!(
                    "Active editor reference: {reference}\n\nRequest: {}",
                    prompt.text
                )
            });
        socket
            .send(ClientMessage::SessionAction {
                session_id: conversation_id.to_owned(),
                action: Box::new(ClientMessage::Prompt {
                    text,
                    delivery: match prompt.delivery {
                        AiPromptDelivery::Prompt => PromptDelivery::Prompt,
                        AiPromptDelivery::Steer => PromptDelivery::Steer,
                        AiPromptDelivery::FollowUp => PromptDelivery::FollowUp,
                    },
                    images: prompt
                        .images
                        .into_iter()
                        .map(|image| ImageAttachment {
                            name: image.name,
                            mime_type: image.mime_type,
                            size: image.size,
                            data: image.data,
                        })
                        .collect(),
                }),
            })
            .await
            .map_err(socket_error)?;
        wait_for_action(&socket).await
    }

    async fn respond_to_extension(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
        request_id: &str,
        value: Option<String>,
        confirmed: Option<bool>,
        cancelled: bool,
    ) -> Result<(), AppError> {
        let socket = connect(workspace).await?;
        socket
            .send(ClientMessage::SelectSession {
                session_id: conversation_id.to_owned(),
            })
            .await
            .map_err(socket_error)?;
        wait_for_selection(&socket, conversation_id).await?;
        socket
            .send(ClientMessage::SessionAction {
                session_id: conversation_id.to_owned(),
                action: Box::new(ClientMessage::ExtensionUiResponse {
                    request_id: request_id.to_owned(),
                    value,
                    confirmed,
                    cancelled,
                }),
            })
            .await
            .map_err(socket_error)?;
        wait_for_action(&socket).await
    }

    async fn fork_at(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
        entry_id: &str,
    ) -> Result<AiConversation, AppError> {
        let socket = connect(workspace).await?;
        socket
            .send(ClientMessage::SelectSession {
                session_id: conversation_id.to_owned(),
            })
            .await
            .map_err(socket_error)?;
        wait_for_selection(&socket, conversation_id).await?;
        socket
            .send(ClientMessage::SessionAction {
                session_id: conversation_id.to_owned(),
                action: Box::new(ClientMessage::ForkMessage {
                    entry_id: entry_id.to_owned(),
                }),
            })
            .await
            .map_err(socket_error)?;
        loop {
            match socket.recv().await.map_err(socket_error)? {
                ServerMessage::SelectedSession {
                    session_id,
                    snapshot,
                } if session_id != conversation_id => {
                    return Ok(conversation_from_snapshot(session_id, snapshot));
                }
                ServerMessage::Error { error } => return Err(agent_error(error.message)),
                _ => {}
            }
        }
    }

    async fn compact(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
        custom_instructions: Option<String>,
    ) -> Result<Box<dyn AiEventStream>, AppError> {
        let socket = connect(workspace).await?;
        socket
            .send(ClientMessage::SelectSession {
                session_id: conversation_id.to_owned(),
            })
            .await
            .map_err(socket_error)?;
        wait_for_selection(&socket, conversation_id).await?;
        socket
            .send(ClientMessage::SessionAction {
                session_id: conversation_id.to_owned(),
                action: Box::new(ClientMessage::Compact {
                    custom_instructions,
                }),
            })
            .await
            .map_err(socket_error)?;
        Ok(Box::new(RemoteAiEventStream {
            socket,
            conversation_id: conversation_id.to_owned(),
            pending: None,
            assistant: None,
            received: 0,
            accumulated_event_bytes: 0,
            activity_bytes: HashMap::new(),
            finished: false,
        }))
    }

    async fn cancel(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<(), AppError> {
        let socket = connect(workspace).await?;
        socket
            .send(ClientMessage::SessionAction {
                session_id: conversation_id.to_owned(),
                action: Box::new(ClientMessage::Abort),
            })
            .await
            .map_err(socket_error)
    }

    async fn clone_conversation(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<AiConversation, AppError> {
        let socket = connect(workspace).await?;
        socket
            .send(ClientMessage::SelectSession {
                session_id: conversation_id.to_owned(),
            })
            .await
            .map_err(socket_error)?;
        wait_for_selection(&socket, conversation_id).await?;
        socket
            .send(ClientMessage::SessionAction {
                session_id: conversation_id.to_owned(),
                action: Box::new(ClientMessage::CloneSession),
            })
            .await
            .map_err(socket_error)?;
        loop {
            match socket.recv().await.map_err(socket_error)? {
                ServerMessage::SelectedSession {
                    session_id,
                    snapshot,
                } if session_id != conversation_id => {
                    return Ok(conversation_from_snapshot(session_id, snapshot));
                }
                ServerMessage::SessionEvent { session_id, event } => match *event {
                    ServerMessage::Snapshot { snapshot } if session_id != conversation_id => {
                        return Ok(conversation_from_snapshot(session_id, snapshot));
                    }
                    ServerMessage::Error { error } => return Err(agent_error(error.message)),
                    _ => {}
                },
                ServerMessage::Error { error } => return Err(agent_error(error.message)),
                _ => {}
            }
        }
    }

    async fn export_conversation(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<(), AppError> {
        let socket = connect(workspace).await?;
        socket
            .send(ClientMessage::SelectSession {
                session_id: conversation_id.to_owned(),
            })
            .await
            .map_err(socket_error)?;
        wait_for_selection(&socket, conversation_id).await?;
        socket
            .send(ClientMessage::SessionAction {
                session_id: conversation_id.to_owned(),
                action: Box::new(ClientMessage::ExportHtml),
            })
            .await
            .map_err(socket_error)?;
        loop {
            match unwrap_session_event(socket.recv().await.map_err(socket_error)?, conversation_id)
            {
                ServerMessage::ExportReady {
                    filename,
                    data_base64,
                } => {
                    download_export(filename, data_base64);
                    return Ok(());
                }
                ServerMessage::Error { error } => return Err(agent_error(error.message)),
                _ => {}
            }
        }
    }

    async fn rename(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
        title: &str,
    ) -> Result<(), AppError> {
        let socket = connect(workspace).await?;
        socket
            .send(ClientMessage::RenameSession {
                session_id: conversation_id.to_owned(),
                name: title.to_owned(),
            })
            .await
            .map_err(socket_error)?;
        wait_for_action(&socket).await
    }

    async fn delete(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<(), AppError> {
        let socket = connect(workspace).await?;
        socket
            .send(ClientMessage::DeleteSession {
                session_id: conversation_id.to_owned(),
            })
            .await
            .map_err(socket_error)?;
        wait_for_action(&socket).await
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
        r"
        const key = await dioxus.recv();
        try { await dioxus.send(localStorage.getItem(key)); }
        catch { await dioxus.send(null); }
        ",
    );
    eval.send(key)
        .map_err(|error| agent_error(format!("Could not read AI client state: {error}")))?;
    eval.recv::<Option<String>>()
        .await
        .map_err(|error| agent_error(format!("Could not read AI client state: {error}")))
}

async fn save_client_state(key: &str, value: Option<&str>) -> Result<(), AppError> {
    let mut eval = document::eval(
        r"
        const key = await dioxus.recv();
        const value = await dioxus.recv();
        try {
          if (value === null) localStorage.removeItem(key);
          else localStorage.setItem(key, value);
          await dioxus.send(null);
        } catch (error) {
          await dioxus.send(error?.message ?? String(error));
        }
        ",
    );
    eval.send(key)
        .and_then(|()| eval.send(value.map(str::to_owned)))
        .map_err(|error| agent_error(format!("Could not save AI client state: {error}")))?;
    match eval.recv::<Option<String>>().await {
        Ok(None) => Ok(()),
        Ok(Some(message)) => Err(agent_error(message)),
        Err(error) => Err(agent_error(format!("Could not save AI client state: {error}"))),
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
    eval.send(value)
        .map_err(|error| agent_error(format!("Could not copy the AI message: {error}")))?;
    match eval.recv::<Option<String>>().await {
        Ok(None) => Ok(()),
        Ok(Some(message)) => Err(agent_error(message)),
        Err(error) => Err(agent_error(format!(
            "Could not copy the AI message: {error}"
        ))),
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
        .map_err(|error| agent_error(format!("Could not use the AI client control: {error}")))?;
    match eval.recv::<Option<String>>().await {
        Ok(None) => Ok(()),
        Ok(Some(message)) => Err(agent_error(message)),
        Err(error) => Err(agent_error(format!(
            "Could not use the AI client control: {error}"
        ))),
    }
}

type AgentSocket = dioxus::fullstack::Websocket<ClientMessage, ServerMessage, api::AgentEncoding>;

struct RemoteAiEventStream {
    socket: AgentSocket,
    conversation_id: String,
    pending: Option<AiEvent>,
    assistant: Option<AiMessage>,
    received: usize,
    accumulated_event_bytes: usize,
    activity_bytes: HashMap<String, usize>,
    finished: bool,
}

#[async_trait(?Send)]
impl AiEventStream for RemoteAiEventStream {
    async fn receive(&mut self) -> Result<Option<AiEvent>, AppError> {
        if let Some(event) = self.pending.take() {
            return Ok(Some(event));
        }
        if self.finished {
            return Ok(None);
        }
        loop {
            self.received = self.received.saturating_add(1);
            if self.received > MAX_CONVERSATION_EVENTS {
                self.finished = true;
                return Err(limit_error("The AI response exceeded the event limit."));
            }
            let message = self.socket.recv().await.map_err(socket_error)?;
            match unwrap_session_event(message, &self.conversation_id) {
                ServerMessage::ItemAdded { item } | ServerMessage::ItemUpdated { item } => {
                    match item {
                        ChatItem::Assistant {
                            id,
                            text,
                            thinking,
                            status,
                            truncated,
                        } => {
                            if text.len() > MAX_ASSISTANT_BYTES {
                                self.finished = true;
                                return Err(limit_error(
                                    "The AI response exceeds the 1 MiB limit.",
                                ));
                            }
                            let delta = self
                                .assistant
                                .as_ref()
                                .filter(|current| current.id == id)
                                .and_then(|current| text.strip_prefix(&current.content))
                                .unwrap_or("")
                                .to_owned();
                            self.assistant = Some(AiMessage {
                                id: id.clone(),
                                role: AiRole::Assistant,
                                content: text,
                                thinking,
                                status: item_status_from_agent(status),
                                truncated,
                                ..AiMessage::default()
                            });
                            if !delta.is_empty() {
                                return Ok(Some(AiEvent::AssistantDelta {
                                    message_id: id,
                                    text: delta,
                                }));
                            }
                        }
                        item => {
                            let payload_bytes = match &item {
                                ChatItem::Tool {
                                    output,
                                    args,
                                    details,
                                    ..
                                } => {
                                    output.len()
                                        + args.as_ref().map_or(0, |value| value.to_string().len())
                                        + details
                                            .as_ref()
                                            .map_or(0, |value| value.to_string().len())
                                }
                                ChatItem::Custom { text, details, .. } => {
                                    text.len()
                                        + details
                                            .as_ref()
                                            .map_or(0, |value| value.to_string().len())
                                }
                                ChatItem::Notice { text, .. } => text.len(),
                                ChatItem::User { .. } | ChatItem::Assistant { .. } => 0,
                            };
                            let previous = self
                                .activity_bytes
                                .insert(item.id().to_owned(), payload_bytes)
                                .unwrap_or_default();
                            self.accumulated_event_bytes = self
                                .accumulated_event_bytes
                                .saturating_sub(previous)
                                .saturating_add(payload_bytes);
                            if self.accumulated_event_bytes > MAX_ACCUMULATED_EVENT_BYTES {
                                self.finished = true;
                                return Err(limit_error(
                                    "The AI activity exceeded the 4 MiB event limit.",
                                ));
                            }
                            if let Some(message) = message_from_item(item.clone()) {
                                return Ok(Some(AiEvent::UserMessage(message)));
                            }
                            if let Some(activity) = activity_from_item(&item) {
                                return Ok(Some(AiEvent::ActivityUpdated(activity)));
                            }
                        }
                    }
                }
                ServerMessage::ItemDelta {
                    item_id,
                    text,
                    thinking,
                } => {
                    let current = self.assistant.get_or_insert_with(|| AiMessage {
                        id: item_id.clone(),
                        role: AiRole::Assistant,
                        status: AiMessageStatus::Streaming,
                        ..AiMessage::default()
                    });
                    if thinking {
                        current.thinking.push_str(&text);
                    } else {
                        current.content.push_str(&text);
                    }
                    if current.content.len().saturating_add(current.thinking.len())
                        > MAX_ASSISTANT_BYTES
                    {
                        self.finished = true;
                        return Err(limit_error("The AI response exceeds the 1 MiB limit."));
                    }
                    return Ok(Some(if thinking {
                        AiEvent::AssistantThinkingDelta {
                            message_id: item_id,
                            text,
                        }
                    } else {
                        AiEvent::AssistantDelta {
                            message_id: item_id,
                            text,
                        }
                    }));
                }
                ServerMessage::SessionStats { stats } => {
                    return Ok(Some(AiEvent::UsageUpdated {
                        input_tokens: stats.tokens.input,
                        output_tokens: stats.tokens.output,
                    }));
                }
                ServerMessage::Status {
                    status,
                    message,
                    pending_messages,
                } => {
                    let running = matches!(
                        status,
                        AgentStatus::Starting | AgentStatus::Working | AgentStatus::Compacting
                    );
                    if running {
                        return Ok(Some(AiEvent::StatusChanged {
                            running: true,
                            message,
                            pending_messages,
                        }));
                    }

                    self.finished = true;
                    if let Some(mut assistant) = self.assistant.take() {
                        assistant.status = match status {
                            AgentStatus::Stopped => AiMessageStatus::Stopped,
                            AgentStatus::Failed => AiMessageStatus::Failed,
                            AgentStatus::Starting
                            | AgentStatus::Ready
                            | AgentStatus::Working
                            | AgentStatus::Compacting => AiMessageStatus::Complete,
                        };
                        self.pending = Some(AiEvent::StatusChanged {
                            running: false,
                            message,
                            pending_messages,
                        });
                        return Ok(Some(AiEvent::AssistantCompleted(assistant)));
                    }
                    return Ok(Some(AiEvent::StatusChanged {
                        running: false,
                        message,
                        pending_messages,
                    }));
                }
                ServerMessage::QueueChanged {
                    steering,
                    follow_up,
                } => {
                    return Ok(Some(AiEvent::QueueChanged {
                        steering,
                        follow_up,
                    }));
                }
                ServerMessage::ExtensionUiRequest { request } => {
                    return Ok(Some(AiEvent::ExtensionRequested(
                        extension_request_from_agent(request),
                    )));
                }
                ServerMessage::ExtensionSurfaces {
                    title,
                    statuses,
                    widgets,
                } => {
                    return Ok(Some(AiEvent::ExtensionSurfaces {
                        title,
                        statuses,
                        widgets: widgets
                            .into_iter()
                            .map(extension_widget_from_agent)
                            .collect(),
                    }));
                }
                ServerMessage::ComposerText { text } => {
                    return Ok(Some(AiEvent::ComposerText(text)));
                }
                ServerMessage::Error { error } => {
                    self.finished = true;
                    return Ok(Some(AiEvent::Failed {
                        message: error.message,
                    }));
                }
                _ => {}
            }
        }
    }
}

fn limit_error(message: impl Into<String>) -> AppError {
    AppError::new(
        AppErrorCode::TooLarge,
        message,
        RetryAdvice::Never,
        ErrorSource::Ai,
    )
}

fn validate_prompt(prompt: &AiPrompt) -> Result<(), AppError> {
    if prompt.text.len() > MAX_PROMPT_BYTES {
        return Err(limit_error("The prompt exceeds the 64 KiB limit."));
    }
    if prompt
        .active_file_reference
        .as_ref()
        .is_some_and(|reference| reference.len() > MAX_CONTEXT_BYTES)
    {
        return Err(limit_error(
            "The editor reference exceeds the 128 KiB limit.",
        ));
    }
    if prompt.images.len() > MAX_PROMPT_IMAGES
        || prompt
            .images
            .iter()
            .any(|image| image.size > MAX_IMAGE_BYTES)
        || prompt.images.iter().map(|image| image.size).sum::<u64>() > MAX_TOTAL_IMAGE_BYTES
    {
        return Err(limit_error(
            "Attach up to 5 images, 8 MiB each and 16 MiB total.",
        ));
    }
    Ok(())
}

#[async_trait(?Send)]
impl AiModelPort for DioxusAi {
    async fn list_models(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
    ) -> Result<Vec<AiModel>, AppError> {
        let snapshot = selected_snapshot(workspace, conversation_id).await?;
        Ok(snapshot
            .models
            .into_iter()
            .map(|model| {
                let thinking_levels = model
                    .supported_thinking_levels()
                    .iter()
                    .copied()
                    .map(thinking_from_agent)
                    .collect();
                AiModel {
                    id: model.key(),
                    label: model.name,
                    provider: model.provider,
                    reasoning: model.reasoning,
                    thinking_levels,
                    supports_images: model.supports_images,
                    context_window: model.context_window,
                    max_tokens: model.max_tokens,
                    cost: AiModelCost {
                        input: model.cost.input,
                        output: model.cost.output,
                        cache_read: model.cost.cache_read,
                        cache_write: model.cost.cache_write,
                        has_paid_tier: model.cost.has_paid_tier,
                    },
                }
            })
            .collect())
    }

    async fn select_model(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
        model_id: &str,
    ) -> Result<(), AppError> {
        let Some((provider, model_id)) = model_id.split_once('\u{1f}') else {
            return Err(AppError::new(
                AppErrorCode::InvalidInput,
                "The selected AI model is invalid.",
                RetryAdvice::Never,
                ErrorSource::Ai,
            ));
        };
        let socket = connect(workspace).await?;
        socket
            .send(ClientMessage::SelectSession {
                session_id: conversation_id.to_owned(),
            })
            .await
            .map_err(socket_error)?;
        let snapshot = wait_for_snapshot(&socket, conversation_id).await?;
        let requested = snapshot
            .models
            .iter()
            .find(|model| model.provider == provider && model.id == model_id)
            .ok_or_else(|| {
                AppError::new(
                    AppErrorCode::NotFound,
                    "The selected AI model is no longer available.",
                    RetryAdvice::AfterUserAction,
                    ErrorSource::Ai,
                )
            })?;
        socket
            .send(ClientMessage::SessionAction {
                session_id: conversation_id.to_owned(),
                action: Box::new(ClientMessage::SetModel {
                    provider: provider.to_owned(),
                    model_id: model_id.to_owned(),
                    thinking_level: requested.effective_thinking_level(snapshot.thinking_level),
                }),
            })
            .await
            .map_err(socket_error)
    }

    async fn select_thinking_level(
        &self,
        workspace: &WorkspaceRecord,
        conversation_id: &str,
        level: AiThinkingLevel,
    ) -> Result<(), AppError> {
        let socket = connect(workspace).await?;
        socket
            .send(ClientMessage::SelectSession {
                session_id: conversation_id.to_owned(),
            })
            .await
            .map_err(socket_error)?;
        wait_for_selection(&socket, conversation_id).await?;
        socket
            .send(ClientMessage::SessionAction {
                session_id: conversation_id.to_owned(),
                action: Box::new(ClientMessage::SetThinkingLevel {
                    level: thinking_to_agent(level),
                }),
            })
            .await
            .map_err(socket_error)
    }

    async fn sync_preferences(
        &self,
        workspace: &WorkspaceRecord,
        available_models: Vec<String>,
    ) -> Result<AiModelPreferences, AppError> {
        api::sync_model_preferences(workspace.id.0.clone(), available_models)
            .await
            .map(model_preferences_from_api)
            .map_err(management_error)
    }

    async fn set_favourite(
        &self,
        workspace: &WorkspaceRecord,
        model_id: &str,
        favourite: bool,
    ) -> Result<AiModelPreferences, AppError> {
        api::set_favourite_model(workspace.id.0.clone(), model_id.to_owned(), favourite)
            .await
            .map(model_preferences_from_api)
            .map_err(management_error)
    }

    async fn remember_effort(
        &self,
        workspace: &WorkspaceRecord,
        model_id: &str,
        level: AiThinkingLevel,
    ) -> Result<AiModelPreferences, AppError> {
        api::set_model_effort(
            workspace.id.0.clone(),
            model_id.to_owned(),
            thinking_to_agent(level),
        )
        .await
        .map(model_preferences_from_api)
        .map_err(management_error)
    }
}

fn model_preferences_from_api(preferences: api::ModelPreferences) -> AiModelPreferences {
    AiModelPreferences {
        favourites: preferences.favourites,
        efforts: preferences
            .efforts
            .into_iter()
            .map(|(model, level)| (model, thinking_from_agent(level)))
            .collect(),
    }
}

#[async_trait(?Send)]
impl AiGeneralSettingsPort for DioxusAi {
    async fn load(&self, workspace: &WorkspaceRecord) -> Result<Vec<AiGeneralSetting>, AppError> {
        api::pi_settings(workspace.id.0.clone())
            .await
            .map(|snapshot| setting_rows(&snapshot))
            .map_err(management_error)
    }

    async fn save(
        &self,
        workspace: &WorkspaceRecord,
        path: &str,
        value: &str,
    ) -> Result<Vec<AiGeneralSetting>, AppError> {
        use super::generated_settings::{PI_SETTING_DEFINITIONS, PiSettingKind};

        let definition = PI_SETTING_DEFINITIONS
            .iter()
            .find(|definition| definition.path == path)
            .ok_or_else(|| {
                AppError::new(
                    AppErrorCode::InvalidInput,
                    "This AI setting is not recognized.",
                    RetryAdvice::Never,
                    ErrorSource::Ai,
                )
            })?;
        let value = match definition.kind {
            PiSettingKind::Toggle => serde_json::json!(value == "true"),
            PiSettingKind::Number => value.parse::<u64>().map_or_else(
                |_| serde_json::json!(value),
                |number| serde_json::json!(number),
            ),
            PiSettingKind::StringArray => serde_json::json!(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .collect::<Vec<_>>()
            ),
            PiSettingKind::Select(_) | PiSettingKind::Text => serde_json::json!(value),
        };
        api::update_pi_setting(workspace.id.0.clone(), path.to_owned(), value)
            .await
            .map(|snapshot| setting_rows(&snapshot))
            .map_err(management_error)
    }

    async fn update_runtime(&self, workspace: &WorkspaceRecord) -> Result<String, AppError> {
        api::update_pi(workspace.id.0.clone())
            .await
            .map(|result| result.message)
            .map_err(management_error)
    }

    async fn load_advanced(
        &self,
        workspace: &WorkspaceRecord,
        scope: AiResourceScope,
    ) -> Result<AiAdvancedSettings, AppError> {
        api::pi_advanced_settings(workspace.id.0.clone(), scope_to_api(scope))
            .await
            .map(advanced_settings_from_api)
            .map_err(management_error)
    }

    async fn save_advanced(
        &self,
        workspace: &WorkspaceRecord,
        settings: AiAdvancedSettings,
    ) -> Result<AiAdvancedSettings, AppError> {
        api::save_pi_advanced_settings(
            workspace.id.0.clone(),
            scope_to_api(settings.scope),
            settings.content,
            settings.revision,
        )
        .await
        .map(advanced_settings_from_api)
        .map_err(management_error)
    }
}

fn advanced_settings_from_api(settings: api::PiAdvancedSettingsSnapshot) -> AiAdvancedSettings {
    AiAdvancedSettings {
        scope: scope_from_api(settings.scope),
        path: settings.path,
        content: settings.content,
        revision: settings.revision,
        documentation: settings.documentation,
    }
}

fn setting_rows(snapshot: &api::PiSettingsSnapshot) -> Vec<AiGeneralSetting> {
    use super::generated_settings::{PI_SETTING_DEFINITIONS, PiSettingKind};

    PI_SETTING_DEFINITIONS
        .iter()
        .map(|definition| {
            let value = definition
                .path
                .split('.')
                .try_fold(&snapshot.values, |value, segment| value.get(segment));
            let value = match value {
                Some(serde_json::Value::Bool(value)) => value.to_string(),
                Some(serde_json::Value::Number(value)) => value.to_string(),
                Some(serde_json::Value::String(value)) => value.clone(),
                Some(serde_json::Value::Array(values)) => values
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => definition.default_value.into(),
            };
            AiGeneralSetting {
                section: definition.section.into(),
                path: definition.path.into(),
                label: definition.label.into(),
                description: definition.description.into(),
                kind: match definition.kind {
                    PiSettingKind::Toggle => AiGeneralSettingKind::Toggle,
                    PiSettingKind::Select(_) => AiGeneralSettingKind::Select,
                    PiSettingKind::Number => AiGeneralSettingKind::Number,
                    PiSettingKind::Text => AiGeneralSettingKind::Text,
                    PiSettingKind::StringArray => AiGeneralSettingKind::StringArray,
                },
                options: match definition.kind {
                    PiSettingKind::Select(options) => {
                        options.iter().map(|option| (*option).into()).collect()
                    }
                    _ => Vec::new(),
                },
                value,
                available: snapshot
                    .available_setters
                    .iter()
                    .any(|setter| setter == definition.setter),
            }
        })
        .collect()
}

#[async_trait(?Send)]
impl AiProviderAuthPort for DioxusAi {
    async fn list(&self, workspace: &WorkspaceRecord) -> Result<Vec<AiProviderAccount>, AppError> {
        api::pi_providers(workspace.id.0.clone())
            .await
            .map_err(management_error)
            .map(|providers| providers.into_iter().map(provider_account).collect())
    }

    async fn start(
        &self,
        workspace: &WorkspaceRecord,
        provider_id: &str,
        kind: AiProviderAuthKind,
    ) -> Result<AiAuthFlow, AppError> {
        api::start_pi_provider_login(
            workspace.id.0.clone(),
            provider_id.to_owned(),
            match kind {
                AiProviderAuthKind::ApiKey => api::PiAuthType::ApiKey,
                AiProviderAuthKind::Oauth => api::PiAuthType::Oauth,
            },
        )
        .await
        .map(flow_from_api)
        .map_err(management_error)
    }

    async fn status(
        &self,
        workspace: &WorkspaceRecord,
        flow_id: &str,
    ) -> Result<AiAuthFlow, AppError> {
        let flow = api::pi_provider_login_status(flow_id.to_owned())
            .await
            .map_err(management_error)?;
        if flow.complete {
            api::reload_pi_agent_runtime(workspace.id.0.clone())
                .await
                .map_err(management_error)?;
        }
        Ok(flow_from_api(flow))
    }

    async fn respond(
        &self,
        _workspace: &WorkspaceRecord,
        flow_id: &str,
        prompt_id: u64,
        answer: &str,
    ) -> Result<(), AppError> {
        api::respond_to_pi_provider_login(flow_id.to_owned(), prompt_id, answer.to_owned())
            .await
            .map_err(management_error)
    }

    async fn cancel(&self, _workspace: &WorkspaceRecord, flow_id: &str) -> Result<(), AppError> {
        api::cancel_pi_provider_login(flow_id.to_owned())
            .await
            .map_err(management_error)
    }

    async fn logout(&self, workspace: &WorkspaceRecord, provider_id: &str) -> Result<(), AppError> {
        api::logout_pi_provider(workspace.id.0.clone(), provider_id.to_owned())
            .await
            .map_err(management_error)?;
        api::reload_pi_agent_runtime(workspace.id.0.clone())
            .await
            .map_err(management_error)
    }
}

#[async_trait(?Send)]
impl AiResourcesPort for DioxusAi {
    async fn load_instructions(&self, workspace: &WorkspaceRecord) -> Result<String, AppError> {
        api::pi_global_instructions(workspace.id.0.clone())
            .await
            .map_err(management_error)
    }

    async fn save_instructions(
        &self,
        workspace: &WorkspaceRecord,
        content: &str,
    ) -> Result<(), AppError> {
        api::save_pi_global_instructions(workspace.id.0.clone(), content.to_owned())
            .await
            .map(|_| ())
            .map_err(management_error)
    }

    async fn prompt_templates(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<Vec<AiPromptTemplate>, AppError> {
        api::prompt_templates(workspace.id.0.clone())
            .await
            .map_err(management_error)
            .map(|items| items.into_iter().map(prompt_from_api).collect())
    }

    async fn save_prompt_template(
        &self,
        workspace: &WorkspaceRecord,
        original_name: Option<&str>,
        template: AiPromptTemplate,
    ) -> Result<(), AppError> {
        api::save_prompt_template(
            workspace.id.0.clone(),
            original_name.map(str::to_owned),
            prompt_to_api(template),
        )
        .await
        .map_err(management_error)
    }

    async fn delete_prompt_template(
        &self,
        workspace: &WorkspaceRecord,
        template: &AiPromptTemplate,
    ) -> Result<(), AppError> {
        api::delete_prompt_template(
            workspace.id.0.clone(),
            template.name.clone(),
            scope_to_api(template.scope),
        )
        .await
        .map_err(management_error)
    }

    async fn skills(&self, workspace: &WorkspaceRecord) -> Result<Vec<AiSkill>, AppError> {
        api::pi_skills(workspace.id.0.clone())
            .await
            .map_err(management_error)
            .map(|items| items.into_iter().map(skill_from_api).collect())
    }

    async fn save_skill(
        &self,
        workspace: &WorkspaceRecord,
        original_storage_name: Option<&str>,
        skill: AiSkill,
    ) -> Result<(), AppError> {
        api::save_pi_skill(
            workspace.id.0.clone(),
            original_storage_name.map(str::to_owned),
            skill_to_api(skill),
        )
        .await
        .map_err(management_error)
    }

    async fn delete_skill(
        &self,
        workspace: &WorkspaceRecord,
        skill: &AiSkill,
    ) -> Result<(), AppError> {
        api::delete_pi_skill(
            workspace.id.0.clone(),
            skill.storage_name.clone(),
            scope_to_api(skill.scope),
            skill.single_file,
        )
        .await
        .map_err(management_error)
    }

    async fn skill_catalog_available(&self) -> Result<bool, AppError> {
        api::skill_catalog_available()
            .await
            .map_err(management_error)
    }

    async fn search_skills(
        &self,
        query: &str,
        offset: usize,
    ) -> Result<AiSkillSearchPage, AppError> {
        api::search_pi_skills(query.to_owned(), offset)
            .await
            .map(skill_search_page_from_api)
            .map_err(management_error)
    }

    async fn browse_skills(
        &self,
        view: AiSkillCatalogView,
        offset: usize,
    ) -> Result<AiSkillSearchPage, AppError> {
        let view = match view {
            AiSkillCatalogView::AllTime => api::SkillCatalogView::AllTime,
            AiSkillCatalogView::Trending => api::SkillCatalogView::Trending,
            AiSkillCatalogView::Hot => api::SkillCatalogView::Hot,
        };
        api::browse_pi_skills(view, offset)
            .await
            .map(skill_search_page_from_api)
            .map_err(management_error)
    }

    async fn install_skill(
        &self,
        workspace: &WorkspaceRecord,
        slug: &str,
        scope: AiResourceScope,
    ) -> Result<(), AppError> {
        api::install_pi_skill(workspace.id.0.clone(), slug.to_owned(), scope_to_api(scope))
            .await
            .map_err(management_error)
    }
}

fn skill_search_page_from_api(page: api::SkillSearchPage) -> AiSkillSearchPage {
    AiSkillSearchPage {
        skills: page
            .skills
            .into_iter()
            .map(|skill| AiSkillSearchResult {
                name: skill.name,
                slug: skill.slug,
                source: skill.source,
                installs: skill.installs,
                page_url: skill.page_url,
                installable: skill.installable,
            })
            .collect(),
        start_offset: page.start_offset,
        next_offset: page.next_offset,
        has_more: page.has_more,
    }
}

#[async_trait(?Send)]
impl AiExtensionsPort for DioxusAi {
    async fn search(
        &self,
        workspace: &WorkspaceRecord,
        query: &str,
        offset: usize,
    ) -> Result<AiExtensionPage, AppError> {
        let page = api::pi_packages(workspace.id.0.clone(), query.to_owned(), offset)
            .await
            .map_err(management_error)?;
        Ok(AiExtensionPage {
            items: page
                .packages
                .into_iter()
                .map(|package| AiExtension {
                    name: package.name,
                    version: package.version,
                    description: package.description,
                    publisher: package.publisher,
                    published_at: package.published_at,
                    monthly_downloads: package.monthly_downloads,
                    kinds: package.kinds,
                    installed_scopes: package.installed_scopes.clone(),
                    installed: !package.installed_scopes.is_empty(),
                })
                .collect(),
            total: page.catalog_total,
            next_offset: page.next_offset,
            has_more: page.has_more,
        })
    }

    async fn manage(
        &self,
        workspace: &WorkspaceRecord,
        package_name: &str,
        action: AiExtensionAction,
    ) -> Result<String, AppError> {
        api::manage_pi_package(
            workspace.id.0.clone(),
            package_name.to_owned(),
            match action {
                AiExtensionAction::Install => api::PiPackageAction::Install,
                AiExtensionAction::Uninstall => api::PiPackageAction::Uninstall,
            },
        )
        .await
        .map(|result| result.message)
        .map_err(management_error)
    }
}

#[async_trait(?Send)]
impl AiWorktreePort for DioxusAi {
    async fn list(&self, workspace: &WorkspaceRecord) -> Result<Vec<WorktreeInfo>, AppError> {
        crate::git::api::worktrees(workspace.id.0.clone())
            .await
            .map_err(management_error)
    }

    async fn create(
        &self,
        workspace: &WorkspaceRecord,
        request: WorktreeCreateRequest,
    ) -> Result<WorktreeInfo, AppError> {
        crate::git::api::create_worktree(workspace.id.0.clone(), request)
            .await
            .map_err(management_error)
    }
}

#[async_trait(?Send)]
impl AiManagedFeaturePort for DioxusAi {
    async fn summary(
        &self,
        workspace: &WorkspaceRecord,
        feature: AiManagedFeature,
    ) -> Result<AiFeatureSummary, AppError> {
        let workspace_id = workspace.id.0.clone();
        let (title, detail) = match feature {
            AiManagedFeature::Usage => (
                "Usage",
                "Token and cost totals are reported in the active conversation timeline.".into(),
            ),
            AiManagedFeature::ProviderAuth => {
                let providers = api::pi_providers(workspace_id)
                    .await
                    .map_err(management_error)?;
                let configured = providers.iter().filter(|provider| provider.configured).count();
                (
                    "Provider accounts",
                    format!("{configured} of {} Pi providers are connected.", providers.len()),
                )
            }
            AiManagedFeature::Resources => {
                let prompts = api::prompt_templates(workspace_id.clone())
                    .await
                    .map_err(management_error)?;
                let skills = api::pi_skills(workspace_id)
                    .await
                    .map_err(management_error)?;
                (
                    "Agent resources",
                    format!("{} prompt templates and {} skills are available.", prompts.len(), skills.len()),
                )
            }
            AiManagedFeature::Extensions => {
                let packages = api::pi_packages(workspace_id, String::new(), 0)
                    .await
                    .map_err(management_error)?;
                (
                    "Extensions",
                    format!("{} Pi packages are available from the configured catalog.", packages.catalog_total),
                )
            }
            AiManagedFeature::Worktrees => (
                "Isolated worktrees",
                "AI conversations can use managed isolated Git worktrees.".into(),
            ),
            AiManagedFeature::Notifications => (
                "Notifications",
                "Background AI and terminal completion notifications are provided by the server runtime.".into(),
            ),
        };
        Ok(AiFeatureSummary {
            title: title.into(),
            detail,
        })
    }
}

async fn connect(
    workspace: &WorkspaceRecord,
) -> Result<dioxus::fullstack::Websocket<ClientMessage, ServerMessage, api::AgentEncoding>, AppError>
{
    let socket = api::agent_socket(workspace.id.0.clone(), WebSocketOptions::new())
        .await
        .map_err(|error| agent_error(error.to_string()))?;
    socket
        .send(ClientMessage::Hello {
            version: PROTOCOL_VERSION,
        })
        .await
        .map_err(socket_error)?;
    Ok(socket)
}

async fn wait_for_selection(
    socket: &dioxus::fullstack::Websocket<ClientMessage, ServerMessage, api::AgentEncoding>,
    conversation_id: &str,
) -> Result<(), AppError> {
    loop {
        match socket.recv().await.map_err(socket_error)? {
            ServerMessage::SelectedSession { session_id, .. } if session_id == conversation_id => {
                return Ok(());
            }
            ServerMessage::Error { error } => return Err(agent_error(error.message)),
            _ => {}
        }
    }
}

async fn wait_for_action(
    socket: &dioxus::fullstack::Websocket<ClientMessage, ServerMessage, api::AgentEncoding>,
) -> Result<(), AppError> {
    // The server processes incoming commands in order. Its Pong confirms the
    // preceding action was processed; initial or broadcast Sessions messages do not.
    let nonce = NEXT_ACTION_NONCE.fetch_add(1, Ordering::Relaxed);
    socket
        .send(ClientMessage::Ping { nonce })
        .await
        .map_err(socket_error)?;
    loop {
        if action_completed(socket.recv().await.map_err(socket_error)?, nonce)? {
            return Ok(());
        }
    }
}

fn action_completed(message: ServerMessage, nonce: u64) -> Result<bool, AppError> {
    match message {
        ServerMessage::Pong { nonce: received } => Ok(received == nonce),
        ServerMessage::Error { error } => Err(agent_error(error.message)),
        _ => Ok(false),
    }
}

async fn selected_snapshot(
    workspace: &WorkspaceRecord,
    conversation_id: &str,
) -> Result<AgentSnapshot, AppError> {
    let socket = connect(workspace).await?;
    socket
        .send(ClientMessage::SelectSession {
            session_id: conversation_id.to_owned(),
        })
        .await
        .map_err(socket_error)?;
    wait_for_snapshot(&socket, conversation_id).await
}

async fn wait_for_snapshot(
    socket: &dioxus::fullstack::Websocket<ClientMessage, ServerMessage, api::AgentEncoding>,
    conversation_id: &str,
) -> Result<AgentSnapshot, AppError> {
    loop {
        match socket.recv().await.map_err(socket_error)? {
            ServerMessage::SelectedSession {
                session_id,
                snapshot,
            } if session_id == conversation_id => {
                return Ok(snapshot);
            }
            ServerMessage::Error { error } => return Err(agent_error(error.message)),
            _ => {}
        }
    }
}

fn unwrap_session_event(message: ServerMessage, conversation_id: &str) -> ServerMessage {
    match message {
        ServerMessage::SessionEvent { session_id, event } if session_id == conversation_id => {
            *event
        }
        message => message,
    }
}

fn conversation_from_snapshot(id: String, snapshot: AgentSnapshot) -> AiConversation {
    let usage = snapshot.session_stats.map(|stats| AiUsage {
        total_tokens: stats.tokens.total,
        context_tokens: stats.context_tokens,
        context_window: stats.context_window,
        context_percent: stats.context_percent,
        cost_microusd: stats.cost_microusd,
        total_messages: stats.total_messages,
        tool_calls: stats.tool_calls,
    });
    let thinking_level = thinking_from_agent(snapshot.thinking_level);
    let activity = snapshot
        .items
        .iter()
        .filter_map(activity_from_item)
        .collect();
    let item_order = snapshot
        .items
        .iter()
        .map(|item| item.id().to_owned())
        .collect();
    let running = matches!(
        snapshot.status,
        AgentStatus::Starting | AgentStatus::Working | AgentStatus::Compacting
    );
    let status_message = snapshot.status_message.clone();
    let pending_messages = snapshot.pending_messages;
    let steering_queue = snapshot.steering_queue.clone();
    let follow_up_queue = snapshot.follow_up_queue.clone();
    let commands = snapshot
        .commands
        .iter()
        .map(|command| AiCommand {
            name: command.name.clone(),
            description: command.description.clone(),
            argument_hint: command.argument_hint.clone(),
            interactive: command.invocation.as_deref() == Some("interactive"),
        })
        .collect();
    let extension_request = snapshot
        .pending_extension_request
        .map(extension_request_from_agent);
    let extension_title = snapshot.extension_title;
    let extension_statuses = snapshot.extension_statuses;
    let extension_widgets = snapshot
        .extension_widgets
        .into_iter()
        .map(extension_widget_from_agent)
        .collect();
    AiConversation {
        id,
        title: snapshot.session_name.unwrap_or_else(|| "New chat".into()),
        messages: snapshot
            .items
            .into_iter()
            .filter_map(message_from_item)
            .collect(),
        activity,
        item_order,
        selected_model_id: snapshot.model.map(|model| model.key()),
        thinking_level,
        usage,
        running,
        status_message,
        pending_messages,
        steering_queue,
        follow_up_queue,
        commands,
        supports_queued_prompts: true,
        extension_request,
        extension_title,
        extension_statuses,
        extension_widgets,
        requested_composer_text: None,
    }
}

fn extension_request_from_agent(request: syntaxis_agent::ExtensionUiRequest) -> AiExtensionRequest {
    AiExtensionRequest {
        id: request.id,
        method: request.method,
        title: request.title,
        message: request.message,
        options: request.options,
        placeholder: request.placeholder,
        prefill: request.prefill,
    }
}

fn extension_widget_from_agent(widget: syntaxis_agent::ExtensionWidget) -> AiExtensionWidget {
    AiExtensionWidget {
        key: widget.key,
        lines: widget.lines,
        placement: widget.placement,
    }
}

fn activity_from_item(item: &ChatItem) -> Option<AiActivity> {
    match item {
        ChatItem::Tool {
            id,
            name,
            summary,
            output,
            args,
            details,
            args_truncated,
            details_truncated,
            status,
        } => Some(AiActivity::Tool {
            id: id.clone(),
            name: name.clone(),
            summary: summary.clone(),
            output: output.clone(),
            args: args.as_ref().and_then(pretty_json),
            details: details.as_ref().and_then(pretty_json),
            args_truncated: *args_truncated,
            details_truncated: *details_truncated,
            status: item_status_from_agent(*status),
        }),
        ChatItem::Custom {
            id,
            label,
            text,
            details,
            details_truncated,
        } => Some(AiActivity::Custom {
            id: id.clone(),
            label: label.clone(),
            text: text.clone(),
            details: details.as_ref().and_then(pretty_json),
            details_truncated: *details_truncated,
        }),
        ChatItem::Notice { id, text, status } => Some(AiActivity::Notice {
            id: id.clone(),
            text: text.clone(),
            status: item_status_from_agent(*status),
        }),
        ChatItem::User { .. } | ChatItem::Assistant { .. } => None,
    }
}

fn pretty_json(value: &serde_json::Value) -> Option<String> {
    serde_json::to_string_pretty(value).ok()
}

const fn thinking_from_agent(level: ThinkingLevel) -> AiThinkingLevel {
    match level {
        ThinkingLevel::Off => AiThinkingLevel::Off,
        ThinkingLevel::Minimal => AiThinkingLevel::Minimal,
        ThinkingLevel::Low => AiThinkingLevel::Low,
        ThinkingLevel::Medium => AiThinkingLevel::Medium,
        ThinkingLevel::High => AiThinkingLevel::High,
        ThinkingLevel::Xhigh => AiThinkingLevel::Xhigh,
        ThinkingLevel::Max => AiThinkingLevel::Max,
    }
}

const fn thinking_to_agent(level: AiThinkingLevel) -> ThinkingLevel {
    match level {
        AiThinkingLevel::Off => ThinkingLevel::Off,
        AiThinkingLevel::Minimal => ThinkingLevel::Minimal,
        AiThinkingLevel::Low => ThinkingLevel::Low,
        AiThinkingLevel::Medium => ThinkingLevel::Medium,
        AiThinkingLevel::High => ThinkingLevel::High,
        AiThinkingLevel::Xhigh => ThinkingLevel::Xhigh,
        AiThinkingLevel::Max => ThinkingLevel::Max,
    }
}

fn message_from_item(item: ChatItem) -> Option<AiMessage> {
    match item {
        ChatItem::User {
            id,
            entry_id,
            text,
            images,
        } => Some(AiMessage {
            id,
            role: AiRole::User,
            content: text,
            entry_id,
            images: images
                .into_iter()
                .map(|image| AiImageAttachment {
                    name: image.name,
                    mime_type: image.mime_type,
                    size: image.size,
                    data: image.data,
                })
                .collect(),
            ..AiMessage::default()
        }),
        ChatItem::Assistant {
            id,
            text,
            thinking,
            status,
            truncated,
        } => Some(AiMessage {
            id,
            role: AiRole::Assistant,
            content: text,
            thinking,
            status: item_status_from_agent(status),
            truncated,
            ..AiMessage::default()
        }),
        _ => None,
    }
}

const fn item_status_from_agent(status: ItemStatus) -> AiMessageStatus {
    match status {
        ItemStatus::Streaming => AiMessageStatus::Streaming,
        ItemStatus::Running => AiMessageStatus::Running,
        ItemStatus::Complete => AiMessageStatus::Complete,
        ItemStatus::Failed => AiMessageStatus::Failed,
        ItemStatus::Stopped => AiMessageStatus::Stopped,
    }
}

fn socket_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        AppErrorCode::Offline,
        error.to_string(),
        RetryAdvice::Backoff,
        ErrorSource::Ai,
    )
}

fn agent_error(message: impl Into<String>) -> AppError {
    AppError::new(
        AppErrorCode::Internal,
        message,
        RetryAdvice::Backoff,
        ErrorSource::Ai,
    )
}

fn management_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        AppErrorCode::Internal,
        error.to_string(),
        RetryAdvice::Backoff,
        ErrorSource::Ai,
    )
}

fn provider_account(provider: api::PiProviderAuth) -> AiProviderAccount {
    AiProviderAccount {
        id: provider.id,
        name: provider.name,
        configured: provider.configured,
        can_logout: provider.can_logout,
        status: provider.status,
        methods: provider
            .methods
            .into_iter()
            .map(|method| AiProviderAuthMethod {
                kind: match method.auth_type {
                    api::PiAuthType::ApiKey => AiProviderAuthKind::ApiKey,
                    api::PiAuthType::Oauth => AiProviderAuthKind::Oauth,
                },
                label: method.label,
            })
            .collect(),
    }
}

fn flow_from_api(flow: api::PiAuthFlow) -> AiAuthFlow {
    AiAuthFlow {
        id: flow.id,
        provider_id: flow.provider_id,
        prompt: flow.prompt.map(|prompt| AiAuthPrompt {
            id: prompt.id,
            kind: prompt.kind,
            message: prompt.message,
            placeholder: prompt.placeholder,
            options: prompt
                .options
                .into_iter()
                .map(|option| AiAuthPromptOption {
                    id: option.id,
                    label: option.label,
                    description: option.description,
                })
                .collect(),
        }),
        events: flow
            .events
            .into_iter()
            .map(|event| AiAuthEvent {
                kind: event.kind,
                message: event.message,
                url: event.url,
                user_code: event.user_code,
            })
            .collect(),
        complete: flow.complete,
        error: flow.error,
    }
}

fn scope_from_api(scope: api::PiResourceScope) -> AiResourceScope {
    match scope {
        api::PiResourceScope::Global => AiResourceScope::Global,
        api::PiResourceScope::Project => AiResourceScope::Project,
    }
}

fn scope_to_api(scope: AiResourceScope) -> api::PiResourceScope {
    match scope {
        AiResourceScope::Global => api::PiResourceScope::Global,
        AiResourceScope::Project => api::PiResourceScope::Project,
    }
}

fn prompt_from_api(template: api::PromptTemplate) -> AiPromptTemplate {
    AiPromptTemplate {
        name: template.name,
        description: template.description,
        argument_hint: template.argument_hint,
        content: template.content,
        scope: scope_from_api(template.scope),
    }
}

fn prompt_to_api(template: AiPromptTemplate) -> api::PromptTemplate {
    api::PromptTemplate {
        name: template.name,
        description: template.description,
        argument_hint: template.argument_hint,
        content: template.content,
        scope: scope_to_api(template.scope),
    }
}

fn skill_from_api(skill: api::PiSkill) -> AiSkill {
    AiSkill {
        name: skill.name,
        description: skill.description,
        content: skill.content,
        scope: scope_from_api(skill.scope),
        storage_name: skill.storage_name,
        single_file: skill.single_file,
        extra_frontmatter: skill.extra_frontmatter,
    }
}

fn skill_to_api(skill: AiSkill) -> api::PiSkill {
    api::PiSkill {
        name: skill.name,
        description: skill.description,
        content: skill.content,
        scope: scope_to_api(skill.scope),
        storage_name: skill.storage_name,
        single_file: skill.single_file,
        extra_frontmatter: skill.extra_frontmatter,
    }
}

#[cfg(test)]
mod tests {
    use syntaxis_agent::{AgentError, AgentErrorCode, ServerMessage};

    use super::{action_completed, ai_ports};

    #[test]
    fn session_lists_do_not_acknowledge_mutations() {
        assert!(
            !action_completed(ServerMessage::Sessions { sessions: Vec::new() }, 42).unwrap()
        );
        assert!(!action_completed(ServerMessage::Pong { nonce: 41 }, 42).unwrap());
        assert!(action_completed(ServerMessage::Pong { nonce: 42 }, 42).unwrap());
    }

    #[test]
    fn failed_actions_are_not_reported_as_successful() {
        let message = ServerMessage::Error {
            error: AgentError::new(AgentErrorCode::InvalidRequest, "Delete failed"),
        };
        assert_eq!(
            action_completed(message, 42).unwrap_err().message,
            "Delete failed",
        );
    }

    #[test]
    fn main_runtime_registers_every_ai_surface() {
        let ports = ai_ports();

        assert!(ports.conversation().is_some());
        assert!(ports.models().is_some());
        assert!(ports.general_settings().is_some());
        assert!(ports.usage().is_some());
        assert!(ports.provider_auth().is_some());
        assert!(ports.resources().is_some());
        assert!(ports.extensions().is_some());
        assert!(ports.worktrees().is_some());
        assert!(ports.notifications().is_some());
        assert!(ports.client().is_some());
        ports.validate().unwrap();
    }
}
