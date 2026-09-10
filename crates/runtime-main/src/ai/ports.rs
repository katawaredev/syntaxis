#![allow(
    clippy::clone_on_ref_ptr,
    clippy::too_many_lines,
    reason = "Runtime registration fans one adapter into the AI capability ports"
)]

use async_trait::async_trait;
use dioxus::fullstack::WebSocketOptions;
use std::sync::atomic::{AtomicU64, Ordering};
use syntaxis_agent::{
    AgentSnapshot, AgentStatus, ChatItem, ClientMessage, ItemStatus, PROTOCOL_VERSION,
    PromptDelivery, ServerMessage,
};
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, PortHandle, RetryAdvice};
use syntaxis_git::{WorktreeCreateRequest, WorktreeInfo};
use syntaxis_module_ai::{
    AiAuthEvent, AiAuthFlow, AiAuthPrompt, AiAuthPromptOption, AiConversation, AiConversationPort,
    AiConversationSummary, AiEvent, AiEventStream, AiExtension, AiExtensionAction, AiExtensionPage,
    AiExtensionsPort, AiFeatureSummary, AiManagedFeature, AiManagedFeaturePort, AiMessage, AiModel,
    AiModelPort, AiPorts, AiPrompt, AiPromptDelivery, AiPromptTemplate, AiProviderAccount,
    AiProviderAuthKind, AiProviderAuthMethod, AiProviderAuthPort, AiResourceScope, AiResourcesPort,
    AiRole, AiSkill, AiWorktreePort,
};
use syntaxis_workspace::WorkspaceRecord;

use super::api;

const MAX_PROMPT_BYTES: usize = 64 * 1024;
const MAX_CONTEXT_BYTES: usize = 128 * 1024;
const MAX_CONVERSATION_EVENTS: usize = 10_000;
const MAX_ASSISTANT_BYTES: usize = 1024 * 1024;
const MAX_ACCUMULATED_EVENT_BYTES: usize = 4 * 1024 * 1024;
static NEXT_LOCAL_MESSAGE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Default)]
struct DioxusAi;

pub(crate) fn ai_ports() -> AiPorts {
    let adapter = PortHandle::new(DioxusAi);
    AiPorts::default()
        .with_conversation(adapter.clone())
        .with_models(adapter.clone())
        .with_usage(adapter.clone())
        .with_provider_auth(adapter.clone())
        .with_resources(adapter.clone())
        .with_extensions(adapter.clone())
        .with_worktrees(adapter.clone())
        .with_notifications(adapter)
}

#[async_trait(?Send)]
impl AiConversationPort for DioxusAi {
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
                        })
                        .collect());
                }
                ServerMessage::Error { error } => return Err(agent_error(error.message)),
                _ => {}
            }
        }
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
                    images: Vec::new(),
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
        });
        Ok(Box::new(MainAiEventStream {
            socket,
            conversation_id: conversation_id.to_owned(),
            pending: Some(user),
            assistant: None,
            received: 0,
            accumulated_event_bytes: 0,
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
}

type AgentSocket = dioxus::fullstack::Websocket<ClientMessage, ServerMessage, api::AgentEncoding>;

struct MainAiEventStream {
    socket: AgentSocket,
    conversation_id: String,
    pending: Option<AiEvent>,
    assistant: Option<AiMessage>,
    received: usize,
    accumulated_event_bytes: usize,
    finished: bool,
}

#[async_trait(?Send)]
impl AiEventStream for MainAiEventStream {
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
                        ChatItem::Assistant { id, text, .. } => {
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
                            });
                            if !delta.is_empty() {
                                return Ok(Some(AiEvent::AssistantDelta {
                                    message_id: id,
                                    text: delta,
                                }));
                            }
                        }
                        ChatItem::Tool {
                            id,
                            name,
                            output,
                            status,
                            ..
                        } => {
                            self.accumulated_event_bytes =
                                self.accumulated_event_bytes.saturating_add(output.len());
                            if self.accumulated_event_bytes > MAX_ACCUMULATED_EVENT_BYTES {
                                self.finished = true;
                                return Err(limit_error(
                                    "The AI tool output exceeded the 4 MiB event limit.",
                                ));
                            }
                            let event = match status {
                                ItemStatus::Complete | ItemStatus::Failed | ItemStatus::Stopped => {
                                    AiEvent::ToolCompleted { id, output }
                                }
                                ItemStatus::Streaming | ItemStatus::Running
                                    if output.is_empty() =>
                                {
                                    AiEvent::ToolStarted { id, name }
                                }
                                ItemStatus::Streaming | ItemStatus::Running => {
                                    AiEvent::ToolUpdated { id, output }
                                }
                            };
                            return Ok(Some(event));
                        }
                        _ => {}
                    }
                }
                ServerMessage::ItemDelta {
                    item_id,
                    text,
                    thinking: false,
                } => {
                    let current = self.assistant.get_or_insert_with(|| AiMessage {
                        id: item_id.clone(),
                        role: AiRole::Assistant,
                        content: String::new(),
                    });
                    current.content.push_str(&text);
                    if current.content.len() > MAX_ASSISTANT_BYTES {
                        self.finished = true;
                        return Err(limit_error("The AI response exceeds the 1 MiB limit."));
                    }
                    return Ok(Some(AiEvent::AssistantDelta {
                        message_id: item_id,
                        text,
                    }));
                }
                ServerMessage::SessionStats { stats } => {
                    return Ok(Some(AiEvent::UsageUpdated {
                        input_tokens: stats.tokens.input,
                        output_tokens: stats.tokens.output,
                    }));
                }
                ServerMessage::Status {
                    status: AgentStatus::Ready,
                    ..
                } => {
                    self.finished = true;
                    return Ok(self.assistant.take().map(AiEvent::AssistantCompleted));
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
            .map(|model| AiModel {
                id: model.key(),
                label: format!("{} · {}", model.provider, model.name),
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
                    monthly_downloads: package.monthly_downloads,
                    kinds: package.kinds,
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
    AiConversation {
        id,
        title: snapshot.session_name.unwrap_or_else(|| "New chat".into()),
        messages: snapshot
            .items
            .into_iter()
            .filter_map(message_from_item)
            .collect(),
        selected_model_id: snapshot.model.map(|model| model.key()),
    }
}

fn message_from_item(item: ChatItem) -> Option<AiMessage> {
    match item {
        ChatItem::User { id, text, .. } => Some(AiMessage {
            id,
            role: AiRole::User,
            content: text,
        }),
        ChatItem::Assistant { id, text, .. } => Some(AiMessage {
            id,
            role: AiRole::Assistant,
            content: text,
        }),
        _ => None,
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
