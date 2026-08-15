//! Terminal WebSocket lifecycle, protocol handshake, and server-message reduction.

use super::super::api;
use super::super::renderer::{RendererOutput, RendererOutputBatch};
use super::super::runtime::{
    ConnectionState, MAX_RECONNECT_ATTEMPTS, command_input, fail_pending_requests,
    push_renderer_output, reconnect_delay_ms,
};
use super::super::session::{choose_active, remove_session, upsert_session};
use dioxus::prelude::*;
use futures_util::{
    FutureExt, StreamExt,
    future::{Either, select},
    pin_mut,
};
use syntaxis_terminal::{
    ClientMessage, Lifecycle, PROTOCOL_VERSION, ServerMessage, SessionId, SessionSummary,
    TerminalErrorCode, TerminalSize,
};

const HEARTBEAT_INTERVAL_SECONDS: u64 = 10;
const HEARTBEAT_TIMEOUT_SECONDS: u64 = 30;

#[derive(Clone)]
pub(super) struct TerminalConnectionOptions {
    pub workspace_id: String,
    pub requested_session_id: Option<String>,
    pub initializer_label: Option<String>,
    pub on_initializer_finished: Option<EventHandler<bool>>,
    pub embedded: bool,
}

#[derive(Clone, Copy)]
pub(super) struct TerminalConnectionState {
    pub connection: Signal<ConnectionState>,
    pub sessions: Signal<Vec<SessionSummary>>,
    pub sessions_loaded: Signal<bool>,
    pub active: Signal<Option<SessionId>>,
    pub remembered: Signal<Option<SessionId>>,
    pub output: Signal<Option<RendererOutputBatch>>,
    pub pending_command: Signal<Option<String>>,
    pub initializer_started: Signal<bool>,
    pub initializer_finished: Signal<bool>,
    pub toast: Signal<Option<String>>,
    pub new_dialog: Signal<bool>,
    pub new_name: Signal<String>,
    pub new_name_server_error: Signal<Option<String>>,
    pub creating_session: Signal<bool>,
}

pub(super) fn use_terminal_connection(
    options: TerminalConnectionOptions,
    state: &TerminalConnectionState,
) -> Coroutine<ClientMessage> {
    let state = Box::new(*state);
    use_coroutine(move |commands| run_connection(options.clone(), state.clone(), commands))
}

#[expect(
    clippy::cognitive_complexity,
    clippy::too_many_lines,
    reason = "the private runner keeps the labeled reconnect loop, heartbeat select, and ordered protocol reduction in one cancellation-safe future"
)]
async fn run_connection(
    options: TerminalConnectionOptions,
    state: Box<TerminalConnectionState>,
    mut commands: UnboundedReceiver<ClientMessage>,
) {
    let TerminalConnectionOptions {
        workspace_id,
        requested_session_id,
        initializer_label,
        on_initializer_finished,
        embedded,
    } = options;
    let TerminalConnectionState {
        mut connection,
        mut sessions,
        mut sessions_loaded,
        mut active,
        remembered,
        mut output,
        mut pending_command,
        mut initializer_started,
        mut initializer_finished,
        mut toast,
        mut new_dialog,
        mut new_name,
        mut new_name_server_error,
        mut creating_session,
    } = *state;

    let mut retry_attempt = 0_u8;
    let mut last_error = String::new();
    'connections: loop {
        if retry_attempt > MAX_RECONNECT_ATTEMPTS {
            connection.set(ConnectionState::Failed(last_error));
            return;
        }
        if retry_attempt == 0 {
            connection.set(ConnectionState::Connecting);
        } else {
            let delay_ms = reconnect_delay_ms(retry_attempt);
            connection.set(ConnectionState::Reconnecting {
                attempt: retry_attempt,
                delay_ms,
                message: last_error.clone(),
            });
            dioxus_sdk_time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
        let connect = api::terminal_socket(
            workspace_id.clone(),
            dioxus::fullstack::WebSocketOptions::new(),
        )
        .fuse();
        let connect_timeout =
            dioxus_sdk_time::sleep(std::time::Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS))
                .fuse();
        pin_mut!(connect, connect_timeout);
        let socket = match select(connect, connect_timeout).await {
            Either::Left((result, _)) => match result {
                Ok(socket) => socket,
                Err(error) => {
                    last_error = error.to_string();
                    retry_attempt = retry_attempt.saturating_add(1);
                    continue;
                }
            },
            Either::Right(_) => {
                last_error = "Timed out while connecting to the terminal".into();
                retry_attempt = retry_attempt.saturating_add(1);
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
            last_error = "Could not start the terminal protocol".into();
            retry_attempt = retry_attempt.saturating_add(1).max(1);
            continue;
        }
        let connected_at = web_time::Instant::now();
        let mut last_received = connected_at;
        let mut handshake_complete = false;
        let mut heartbeat_nonce = 0_u64;
        let heartbeat_interval = std::time::Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS);
        let mut heartbeat_due = connected_at + heartbeat_interval;
        loop {
            let outgoing = commands.next().fuse();
            let incoming = socket.recv().fuse();
            pin_mut!(outgoing, incoming);
            let traffic = select(outgoing, incoming).fuse();
            let heartbeat = dioxus_sdk_time::sleep(
                heartbeat_due.saturating_duration_since(web_time::Instant::now()),
            )
            .fuse();
            pin_mut!(traffic, heartbeat);
            let next = select(heartbeat, traffic).await;
            match next {
                Either::Right((Either::Left((Some(message), _)), _)) => {
                    if socket.send(message).await.is_err() {
                        last_error = "Terminal connection was lost".into();
                        fail_pending_requests(
                            &mut creating_session,
                            &mut new_name_server_error,
                            &mut pending_command,
                            &mut toast,
                        );
                        retry_attempt = retry_attempt.saturating_add(1).max(1);
                        continue 'connections;
                    }
                }
                Either::Right((Either::Left((None, _)), _)) => return,
                Either::Right((Either::Right((Ok(message), _)), _)) => {
                    last_received = web_time::Instant::now();
                    match message {
                        ServerMessage::Hello { version } if version == PROTOCOL_VERSION => {
                            handshake_complete = true;
                            retry_attempt = 0;
                            connection.set(ConnectionState::Ready);
                            if socket.send(ClientMessage::List).await.is_err() {
                                last_error = "Could not load terminal sessions".into();
                                retry_attempt = 1;
                                continue 'connections;
                            }
                        }
                        ServerMessage::Hello { .. } => {
                            connection.set(ConnectionState::Failed(
                                "The server uses an incompatible terminal protocol".into(),
                            ));
                            return;
                        }
                        ServerMessage::Sessions {
                            sessions: available,
                        } => {
                            sessions_loaded.set(true);
                            let start_initializer =
                                pending_command.read().is_some() && !initializer_started();
                            let requested = requested_session_id.as_ref().map(SessionId::new);
                            let selected = choose_active(
                                &available,
                                requested.as_ref(),
                                active().as_ref(),
                                remembered().as_ref(),
                            );
                            sessions.set(available);
                            active.set(selected.clone());
                            output.set(None);
                            if start_initializer {
                                initializer_started.set(true);
                                let name = initializer_label
                                    .clone()
                                    .unwrap_or_else(|| "Project setup".into());
                                if socket
                                    .send(ClientMessage::Create {
                                        name: Some(name),
                                        size: TerminalSize::DEFAULT,
                                    })
                                    .await
                                    .is_err()
                                {
                                    initializer_started.set(false);
                                    last_error = "Could not start the project initializer".into();
                                    retry_attempt = 1;
                                    continue 'connections;
                                }
                            } else if let Some(session_id) = selected
                                && socket
                                    .send(ClientMessage::Attach { session_id })
                                    .await
                                    .is_err()
                            {
                                last_error = "Could not reattach the terminal session".into();
                                retry_attempt = 1;
                                continue 'connections;
                            }
                        }
                        ServerMessage::Created { session } => {
                            upsert_session(&mut sessions, session.clone());
                            output.set(None);
                            active.set(Some(session.id.clone()));
                            if creating_session() {
                                creating_session.set(false);
                                new_dialog.set(false);
                                new_name.set(String::new());
                                new_name_server_error.set(None);
                            }
                            let command = {
                                let mut pending = pending_command.write();
                                pending.take()
                            };
                            if let Some(command) = command {
                                let bytes = command_input(&command, embedded);
                                if socket
                                    .send(ClientMessage::Write {
                                        session_id: session.id,
                                        data: bytes,
                                    })
                                    .await
                                    .is_err()
                                {
                                    pending_command.set(Some(command));
                                    last_error = "Could not send the terminal command".into();
                                    fail_pending_requests(
                                        &mut creating_session,
                                        &mut new_name_server_error,
                                        &mut pending_command,
                                        &mut toast,
                                    );
                                    retry_attempt = 1;
                                    continue 'connections;
                                }
                            }
                        }
                        ServerMessage::Attached { session } => {
                            upsert_session(&mut sessions, session.clone());
                            output.set(None);
                            active.set(Some(session.id));
                        }
                        ServerMessage::Output {
                            session_id,
                            sequence,
                            data,
                            ..
                        } => {
                            if active().as_ref() == Some(&session_id) {
                                push_renderer_output(
                                    &mut output,
                                    RendererOutput {
                                        session_id,
                                        sequence,
                                        data,
                                    },
                                );
                            }
                        }
                        ServerMessage::Lifecycle { session } => {
                            if embedded
                                && initializer_started()
                                && !initializer_finished()
                                && pending_command.read().is_none()
                                && matches!(
                                    session.lifecycle,
                                    Lifecycle::Exited | Lifecycle::Failed
                                )
                            {
                                initializer_finished.set(true);
                                if let Some(on_finished) = on_initializer_finished {
                                    on_finished.call(
                                        session.lifecycle == Lifecycle::Exited
                                            && session.exit_code == Some(0),
                                    );
                                }
                            }
                            upsert_session(&mut sessions, session);
                        }
                        ServerMessage::Closed { session_id } => {
                            let was_active = active().as_ref() == Some(&session_id);
                            remove_session(&mut sessions, &mut active, &session_id);
                            output.set(None);
                            if was_active
                                && let Some(session_id) = active()
                                && socket
                                    .send(ClientMessage::Attach { session_id })
                                    .await
                                    .is_err()
                            {
                                last_error = "Could not attach the next terminal session".into();
                                retry_attempt = 1;
                                continue 'connections;
                            }
                        }
                        ServerMessage::Detached { session_id } => {
                            let was_active = active().as_ref() == Some(&session_id);
                            remove_session(&mut sessions, &mut active, &session_id);
                            output.set(None);
                            toast.set(Some("Terminal detached; refresh to reattach".into()));
                            if was_active
                                && let Some(session_id) = active()
                                && socket
                                    .send(ClientMessage::Attach { session_id })
                                    .await
                                    .is_err()
                            {
                                last_error = "Could not attach the next terminal session".into();
                                retry_attempt = 1;
                                continue 'connections;
                            }
                        }
                        ServerMessage::Error { error } => {
                            if error.code == TerminalErrorCode::OutputLagged {
                                last_error = error.message;
                                fail_pending_requests(
                                    &mut creating_session,
                                    &mut new_name_server_error,
                                    &mut pending_command,
                                    &mut toast,
                                );
                                retry_attempt = 1;
                                continue 'connections;
                            } else if creating_session()
                                && error.code == TerminalErrorCode::InvalidRequest
                            {
                                creating_session.set(false);
                                new_name_server_error.set(Some(error.message));
                            } else {
                                creating_session.set(false);
                                pending_command.set(None);
                                toast.set(Some(error.message));
                            }
                        }
                        ServerMessage::Pong { .. } => {}
                    }
                }
                Either::Right((Either::Right((Err(error), _)), _)) => {
                    last_error = error.to_string();
                    fail_pending_requests(
                        &mut creating_session,
                        &mut new_name_server_error,
                        &mut pending_command,
                        &mut toast,
                    );
                    retry_attempt = retry_attempt.saturating_add(1).max(1);
                    continue 'connections;
                }
                Either::Left(_) => {
                    heartbeat_due = web_time::Instant::now() + heartbeat_interval;
                    if !handshake_complete
                        && connected_at.elapsed()
                            >= std::time::Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS)
                    {
                        last_error = "Terminal protocol handshake timed out".into();
                        retry_attempt = retry_attempt.saturating_add(1).max(1);
                        continue 'connections;
                    }
                    if last_received.elapsed()
                        >= std::time::Duration::from_secs(HEARTBEAT_TIMEOUT_SECONDS)
                    {
                        last_error = "Terminal heartbeat timed out".into();
                        fail_pending_requests(
                            &mut creating_session,
                            &mut new_name_server_error,
                            &mut pending_command,
                            &mut toast,
                        );
                        retry_attempt = retry_attempt.saturating_add(1).max(1);
                        continue 'connections;
                    }
                    heartbeat_nonce = heartbeat_nonce.saturating_add(1);
                    if socket
                        .send(ClientMessage::Ping {
                            nonce: heartbeat_nonce,
                        })
                        .await
                        .is_err()
                    {
                        last_error = "Terminal heartbeat failed".into();
                        fail_pending_requests(
                            &mut creating_session,
                            &mut new_name_server_error,
                            &mut pending_command,
                            &mut toast,
                        );
                        retry_attempt = retry_attempt.saturating_add(1).max(1);
                        continue 'connections;
                    }
                }
            }
        }
    }
}
