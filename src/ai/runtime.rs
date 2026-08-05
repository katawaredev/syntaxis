use dioxus::prelude::*;
use futures_util::{future::FutureExt, StreamExt};
use syntaxis_agent::{
    AgentSessionSummary, AgentSnapshot, ClientMessage, ExtensionUiRequest, ImageAttachment,
    PromptDelivery, ServerMessage, PROTOCOL_VERSION,
};
use syntaxis_workspace::WorkspaceId;

use super::{
    api,
    components::ComposerSubmission,
    session::{
        apply_server_message, initial_session_request, pending_session_request, session_action,
    },
};

const MAX_RECONNECT_ATTEMPTS: u8 = 6;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum ConnectionState {
    Connecting,
    Reconnecting(u8),
    Ready,
    Failed(String),
}

impl ConnectionState {
    pub(super) fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    pub(super) fn label(&self) -> String {
        match self {
            Self::Connecting => "Connecting".into(),
            Self::Reconnecting(_) => "Reconnecting".into(),
            Self::Ready => "Pi connected".into(),
            Self::Failed(_) => "Offline".into(),
        }
    }

    pub(super) fn banner(&self) -> Option<String> {
        match self {
            Self::Connecting => Some("Connecting to Pi…".into()),
            Self::Reconnecting(attempt) => Some(format!(
                "Connection lost. Reconnecting (attempt {attempt})…"
            )),
            Self::Failed(message) => Some(message.clone()),
            Self::Ready => None,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct AgentRuntime {
    pub connection: Signal<ConnectionState>,
    pub snapshot: Signal<AgentSnapshot>,
    pub sessions: Signal<Vec<AgentSessionSummary>>,
    pub selected_id: Signal<Option<String>>,
    pub draft: Signal<String>,
    pub error: Signal<Option<String>>,
    pub extension_request: Signal<Option<ExtensionUiRequest>>,
    pub attachments: Signal<Vec<ImageAttachment>>,
    pub composer_error: Signal<Option<String>>,
    pub draft_session: Signal<bool>,
    pub creating_session: Signal<bool>,
    pub pending_new_prompt: Signal<Option<ComposerSubmission>>,
    pub client: Coroutine<ClientMessage>,
}

impl AgentRuntime {
    pub(super) fn select_session(mut self, session_id: String) {
        self.attachments.set(Vec::new());
        self.composer_error.set(None);
        self.draft_session.set(false);
        self.pending_new_prompt.set(None);
        self.selected_id.set(Some(session_id.clone()));
        self.snapshot.set(AgentSnapshot::default());
        self.extension_request.set(None);
        self.client
            .send(ClientMessage::SelectSession { session_id });
    }

    pub(super) fn create_session(mut self) {
        self.attachments.set(Vec::new());
        self.composer_error.set(None);
        self.draft.set(String::new());
        self.selected_id.set(None);
        self.snapshot.set(AgentSnapshot::default());
        self.extension_request.set(None);
        self.pending_new_prompt.set(None);
        self.draft_session.set(true);
        self.creating_session.set(true);
        self.client.send(ClientMessage::CreateSession);
    }

    pub(super) fn submit_prompt(mut self, submission: ComposerSubmission, working: bool) {
        let text = submission.text.trim().to_owned();
        if (text.is_empty() && submission.images.is_empty()) || !self.connection.read().is_ready() {
            return;
        }
        let prompt = ComposerSubmission {
            text,
            images: submission.images,
        };
        if let Some(session_id) = (self.selected_id)() {
            self.client.send(session_action(
                session_id,
                ClientMessage::Prompt {
                    text: prompt.text,
                    images: prompt.images,
                    delivery: if working {
                        PromptDelivery::Steer
                    } else {
                        PromptDelivery::Prompt
                    },
                },
            ));
        } else if (self.draft_session)()
            && !(self.creating_session)()
            && (self.pending_new_prompt)().is_none()
        {
            self.pending_new_prompt.set(Some(prompt));
            self.creating_session.set(true);
            self.client.send(ClientMessage::CreateSession);
        } else {
            return;
        }
        self.draft.set(String::new());
    }

    pub(super) fn send_to_selected(self, action: ClientMessage) {
        if let Some(session_id) = (self.selected_id)() {
            self.client.send(session_action(session_id, action));
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "the hook keeps the complete reconnecting client-side AI protocol state machine in one place"
)]
pub(super) fn use_agent_runtime(
    workspace_id: &str,
    requested_session_id: Option<&str>,
    active_workspace: crate::workspace::ActiveWorkspace,
) -> AgentRuntime {
    let workspace_target_id = WorkspaceId::new(workspace_id.to_owned());
    let mut connection = use_signal(|| ConnectionState::Connecting);
    let mut snapshot = use_signal(AgentSnapshot::default);
    let mut sessions = use_signal(Vec::<AgentSessionSummary>::new);
    let mut selected_id = use_signal(|| None::<String>);
    let mut draft = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut extension_request = use_signal(|| None);
    let mut attachments = use_signal(Vec::new);
    let composer_error = use_signal(|| None::<String>);
    let mut draft_session = use_signal(|| false);
    let mut creating_session = use_signal(|| false);
    let mut pending_new_prompt = use_signal(|| None::<ComposerSubmission>);

    let client = use_coroutine({
        let workspace_id = workspace_id.to_owned();
        let workspace_target_id = workspace_target_id.clone();
        let requested_session_id = requested_session_id.map(str::to_owned);
        move |mut outgoing: UnboundedReceiver<ClientMessage>| {
            let workspace_id = workspace_id.clone();
            let workspace_target_id = workspace_target_id.clone();
            let requested_session_id = requested_session_id.clone();
            async move {
                let mut attempt = 0_u8;
                loop {
                    if attempt > MAX_RECONNECT_ATTEMPTS {
                        connection
                            .set(ConnectionState::Failed("Could not reconnect to Pi.".into()));
                        return;
                    }
                    if attempt == 0 {
                        connection.set(ConnectionState::Connecting);
                    } else {
                        connection.set(ConnectionState::Reconnecting(attempt));
                        dioxus_sdk_time::sleep(std::time::Duration::from_millis(
                            reconnect_delay_ms(attempt),
                        ))
                        .await;
                    }
                    let socket = match api::agent_socket(
                        workspace_id.clone(),
                        dioxus::fullstack::WebSocketOptions::new(),
                    )
                    .await
                    {
                        Ok(socket) => socket,
                        Err(socket_error) => {
                            error.set(Some(socket_error.to_string()));
                            attempt = attempt.saturating_add(1);
                            continue;
                        }
                    };
                    if socket
                        .send(ClientMessage::Hello {
                            version: PROTOCOL_VERSION,
                        })
                        .await
                        .is_err()
                    {
                        attempt = attempt.saturating_add(1);
                        continue;
                    }
                    let mut initial_selection_sent = false;
                    let mut replacement_selection_pending = false;
                    loop {
                        let send = outgoing.next().fuse();
                        let receive = socket.recv().fuse();
                        futures_util::pin_mut!(send, receive);
                        match futures_util::future::select(send, receive).await {
                            futures_util::future::Either::Left((Some(message), _)) => {
                                if socket.send(message).await.is_err() {
                                    attempt = attempt.saturating_add(1).max(1);
                                    break;
                                }
                            }
                            futures_util::future::Either::Left((None, _)) => return,
                            futures_util::future::Either::Right((Ok(message), _)) => {
                                if matches!(message, ServerMessage::Hello { version } if version == PROTOCOL_VERSION)
                                {
                                    attempt = 0;
                                    error.set(None);
                                    connection.set(ConnectionState::Ready);
                                    continue;
                                }
                                if let ServerMessage::Sessions {
                                    sessions: available,
                                } = &message
                                {
                                    if !initial_selection_sent {
                                        let create_requested = active_workspace
                                            .should_create_agent_session(&workspace_target_id);
                                        let request = if pending_new_prompt().is_some() {
                                            Some(pending_session_request(available))
                                        } else {
                                            initial_session_request(
                                                available,
                                                requested_session_id.clone().or_else(&*selected_id),
                                                create_requested || draft_session(),
                                            )
                                        };
                                        if let Some(request) = request {
                                            if socket.send(request).await.is_err() {
                                                attempt = attempt.saturating_add(1).max(1);
                                                break;
                                            }
                                        } else {
                                            selected_id.set(None);
                                            snapshot.set(AgentSnapshot::default());
                                            draft_session.set(true);
                                            creating_session.set(true);
                                            if socket
                                                .send(ClientMessage::CreateSession)
                                                .await
                                                .is_err()
                                            {
                                                attempt = attempt.saturating_add(1).max(1);
                                                break;
                                            }
                                        }
                                        if create_requested {
                                            active_workspace.complete_agent_session_request(
                                                &workspace_target_id,
                                            );
                                        }
                                        initial_selection_sent = true;
                                    } else if !replacement_selection_pending
                                        && selected_id().as_ref().is_some_and(|selected| {
                                            !available.iter().any(|session| session.id == *selected)
                                        })
                                    {
                                        if let Some(request) =
                                            initial_session_request(available, None, false)
                                        {
                                            if socket.send(request).await.is_err() {
                                                attempt = attempt.saturating_add(1).max(1);
                                                break;
                                            }
                                            replacement_selection_pending = true;
                                        } else {
                                            selected_id.set(None);
                                            snapshot.set(AgentSnapshot::default());
                                            draft_session.set(true);
                                        }
                                    }
                                }
                                if let ServerMessage::SelectedSession { session_id, .. } = &message
                                {
                                    creating_session.set(false);
                                    replacement_selection_pending = false;
                                    if let Some(submission) = pending_new_prompt() {
                                        let action = session_action(
                                            session_id.clone(),
                                            ClientMessage::Prompt {
                                                text: submission.text,
                                                images: submission.images,
                                                delivery: PromptDelivery::Prompt,
                                            },
                                        );
                                        if socket.send(action).await.is_err() {
                                            attempt = attempt.saturating_add(1).max(1);
                                            break;
                                        }
                                        pending_new_prompt.set(None);
                                    }
                                    draft_session.set(false);
                                } else if matches!(message, ServerMessage::Error { .. })
                                    && pending_new_prompt().is_some()
                                {
                                    if let Some(submission) = pending_new_prompt.write().take() {
                                        draft.set(submission.text);
                                        attachments.set(submission.images);
                                    }
                                    draft_session.set(true);
                                    creating_session.set(false);
                                } else if matches!(message, ServerMessage::Error { .. })
                                    && creating_session()
                                {
                                    creating_session.set(false);
                                }
                                apply_server_message(
                                    message,
                                    &mut sessions,
                                    &mut selected_id,
                                    &mut snapshot,
                                    &mut draft,
                                    &mut error,
                                    &mut extension_request,
                                );
                            }
                            futures_util::future::Either::Right((Err(socket_error), _)) => {
                                error.set(Some(socket_error.to_string()));
                                attempt = attempt.saturating_add(1).max(1);
                                break;
                            }
                        }
                    }
                }
            }
        }
    });

    use_browser_resume(connection, client);

    AgentRuntime {
        connection,
        snapshot,
        sessions,
        selected_id,
        draft,
        error,
        extension_request,
        attachments,
        composer_error,
        draft_session,
        creating_session,
        pending_new_prompt,
        client,
    }
}

fn use_browser_resume(connection: Signal<ConnectionState>, mut client: Coroutine<ClientMessage>) {
    let mut bridge = use_signal(|| None::<dioxus::document::Eval>);
    use_effect(move || {
        let mut events = document::eval(
            r#"
            let scheduled = false;
            const resume = () => {
                if (document.visibilityState !== "visible" || scheduled) return;
                scheduled = true;
                requestAnimationFrame(() => {
                    scheduled = false;
                    dioxus.send(true);
                });
            };
            window.addEventListener("focus", resume);
            window.addEventListener("online", resume);
            document.addEventListener("visibilitychange", resume);
            await dioxus.recv();
            window.removeEventListener("focus", resume);
            window.removeEventListener("online", resume);
            document.removeEventListener("visibilitychange", resume);
            "#,
        );
        bridge.set(Some(events));
        spawn(async move {
            while events.recv::<bool>().await.is_ok() {
                match connection() {
                    ConnectionState::Ready => client.send(ClientMessage::Ping { nonce: 0 }),
                    ConnectionState::Reconnecting(_) | ConnectionState::Failed(_) => {
                        client.restart();
                    }
                    ConnectionState::Connecting => {}
                }
            }
        });
    });
    use_drop(move || {
        if let Some(events) = bridge() {
            let _ = events.send(true);
        }
    });
}

fn reconnect_delay_ms(attempt: u8) -> u64 {
    250_u64
        .saturating_mul(1_u64 << attempt.saturating_sub(1).min(5))
        .min(8_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_backoff_is_exponential_and_bounded() {
        assert_eq!(reconnect_delay_ms(1), 250);
        assert_eq!(reconnect_delay_ms(2), 500);
        assert_eq!(reconnect_delay_ms(6), 8_000);
        assert_eq!(reconnect_delay_ms(u8::MAX), 8_000);
    }
}
