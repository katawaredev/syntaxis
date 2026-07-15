use std::sync::OnceLock;

use dioxus::{
    fullstack::{TypedWebsocket, WebSocketOptions, Websocket},
    prelude::ServerFnError,
};
use syntaxis_agent::{AgentError, AgentErrorCode, ClientMessage, ServerMessage, PROTOCOL_VERSION};
use syntaxis_agent_host::HostAgentManager;
use syntaxis_workspace::WorkspaceId;

use super::AgentEncoding;

const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
static AGENTS: OnceLock<HostAgentManager> = OnceLock::new();

pub(super) async fn agent_socket(
    workspace_id: WorkspaceId,
    options: WebSocketOptions,
) -> Result<Websocket<ClientMessage, ServerMessage, AgentEncoding>, ServerFnError> {
    let workspace = crate::workspace::api::get_workspace(workspace_id.0).await?;
    let agent = agents().workspace(&workspace);
    Ok(options.on_upgrade(
        move |mut socket: TypedWebsocket<ClientMessage, ServerMessage, AgentEncoding>| async move {
            let Ok(Ok(handshake)) = tokio::time::timeout(HANDSHAKE_TIMEOUT, socket.recv()).await
            else {
                let _ = socket
                    .send(ServerMessage::Error {
                        error: AgentError::new(
                            AgentErrorCode::InvalidProtocol,
                            "AI protocol handshake required",
                        ),
                    })
                    .await;
                return;
            };
            if let Err(error) = handshake.validate_handshake() {
                let _ = socket.send(ServerMessage::Error { error }).await;
                return;
            }
            if socket
                .send(ServerMessage::Hello {
                    version: PROTOCOL_VERSION,
                })
                .await
                .is_err()
                || socket
                    .send(ServerMessage::Sessions {
                        sessions: agent.sessions(),
                    })
                    .await
                    .is_err()
            {
                return;
            }

            let mut events = agent.subscribe();
            loop {
                tokio::select! {
                    incoming = socket.recv() => {
                        let Ok(message) = incoming else { break; };
                        let result = match message {
                            ClientMessage::Ping { nonce } => {
                                if socket.send(ServerMessage::Pong { nonce }).await.is_err() {
                                    break;
                                }
                                continue;
                            }
                            ClientMessage::CreateSession => agent.create_session().await.map(|(session_id, snapshot)| {
                                ServerMessage::SelectedSession { session_id, snapshot }
                            }),
                            ClientMessage::SelectSession { session_id } => agent.select_session(&session_id).await.map(|snapshot| {
                                ServerMessage::SelectedSession { session_id, snapshot }
                            }),
                            ClientMessage::SessionAction { session_id, action } => {
                                agent.handle(&session_id, *action).await.map(|()| ServerMessage::Sessions { sessions: agent.sessions() })
                            }
                            _ => Err(AgentError::new(
                                AgentErrorCode::InvalidRequest,
                                "A workspace-level AI action is required",
                            )),
                        };
                        match result {
                            Ok(message) => {
                                if socket.send(message).await.is_err() {
                                    break;
                                }
                            }
                            Err(error) => {
                            if socket.send(ServerMessage::Error { error }).await.is_err() {
                                break;
                            }
                            }
                        }
                    }
                    event = events.recv() => match event {
                        Ok(message) => {
                            if socket.send(message).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            if socket.send(ServerMessage::Sessions { sessions: agent.sessions() }).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        },
    ))
}

fn agents() -> &'static HostAgentManager {
    AGENTS.get_or_init(HostAgentManager::default)
}
