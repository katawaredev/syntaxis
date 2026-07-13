use dioxus::{
    fullstack::{TypedWebsocket, WebSocketOptions, Websocket},
    prelude::ServerFnError,
};
use std::{collections::HashMap, sync::OnceLock};
use syntaxis_terminal::{
    ClientMessage, ServerMessage, SessionId, TerminalError, TerminalErrorCode, PROTOCOL_VERSION,
};
use syntaxis_terminal_host::{
    HostTerminalEvent, HostTerminalManager, SessionAttachment, TerminalHostConfig,
};
use syntaxis_workspace::{WorkspaceId, WorkspaceRecord};
use tokio::{sync::mpsc, task::JoinHandle};

use super::TerminalEncoding;

const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
static TERMINALS: OnceLock<HostTerminalManager> = OnceLock::new();
pub(super) async fn terminal_socket(
    workspace_id: WorkspaceId,
    options: WebSocketOptions,
) -> Result<Websocket<ClientMessage, ServerMessage, TerminalEncoding>, ServerFnError> {
    let workspace = crate::workspace::api::get_workspace(workspace_id.0.clone()).await?;
    let manager = terminals().clone();
    Ok(options.on_upgrade(
        move |mut socket: TypedWebsocket<ClientMessage, ServerMessage, TerminalEncoding>| async move {
            let Ok(Ok(handshake)) = tokio::time::timeout(HANDSHAKE_TIMEOUT, socket.recv()).await
            else {
                let _ = socket
                    .send(protocol_error("Terminal protocol handshake required"))
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
            {
                return;
            }
            let (event_tx, mut event_rx) = mpsc::channel(256);
            let mut attachments = HashMap::<SessionId, JoinHandle<()>>::new();
            loop {
                tokio::select! {
                    incoming = socket.recv() => { let Ok(message) = incoming else {
                    break; }; let outgoing = handle_message(message, & workspace, &
                    manager, & event_tx, & mut attachments,); for message in outgoing
                    { if socket.send(message). await .is_err() { abort_attachments(&
                    mut attachments); return; } } } forwarded = event_rx.recv() => {
                    let Some(message) = forwarded else { break; }; if socket
                    .send(message). await .is_err() { break; } }
                }
            }
            abort_attachments(&mut attachments);
        },
    ))
}
fn handle_message(
    message: ClientMessage,
    workspace: &WorkspaceRecord,
    manager: &HostTerminalManager,
    event_tx: &mpsc::Sender<ServerMessage>,
    attachments: &mut HashMap<SessionId, JoinHandle<()>>,
) -> Vec<ServerMessage> {
    if let Err(error) = message.validate() {
        return vec![ServerMessage::Error { error }];
    }
    let result = match message {
        ClientMessage::Hello { .. } => Err(TerminalError::new(
            TerminalErrorCode::InvalidProtocol,
            "The protocol handshake was already completed",
        )),
        ClientMessage::List => manager
            .list(&workspace.id)
            .map(|sessions| vec![ServerMessage::Sessions { sessions }]),
        ClientMessage::Create { name, size } => manager
            .create(workspace, name.as_deref(), size)
            .and_then(|session| {
                let (_, attachment) = manager.attach(&workspace.id, &session.id)?;
                abort_attachments(attachments);
                let mut messages = vec![ServerMessage::Created {
                    session: session.clone(),
                }];
                messages.extend(attachment.replay.iter().map(|(sequence, data)| {
                    ServerMessage::Output {
                        session_id: session.id.clone(),
                        sequence: *sequence,
                        data: data.clone(),
                        replay: true,
                    }
                }));
                install_attachment(attachment, event_tx, attachments, &session.id);
                Ok(messages)
            }),
        ClientMessage::Attach { session_id } => {
            manager
                .attach(&workspace.id, &session_id)
                .map(|(session, attachment)| {
                    abort_attachments(attachments);
                    let mut messages = vec![ServerMessage::Attached {
                        session: session.clone(),
                    }];
                    messages.extend(attachment.replay.iter().map(|(sequence, data)| {
                        ServerMessage::Output {
                            session_id: session_id.clone(),
                            sequence: *sequence,
                            data: data.clone(),
                            replay: true,
                        }
                    }));
                    install_attachment(attachment, event_tx, attachments, &session_id);
                    messages
                })
        }
        ClientMessage::Detach { session_id } => {
            if let Some(attachment) = attachments.remove(&session_id) {
                attachment.abort();
            }
            Ok(vec![ServerMessage::Detached { session_id }])
        }
        ClientMessage::Write { session_id, data } => manager
            .write(&workspace.id, &session_id, &data)
            .map(|()| Vec::new()),
        ClientMessage::Resize { session_id, size } => manager
            .resize(&workspace.id, &session_id, size)
            .map(|()| Vec::new()),
        ClientMessage::Close { session_id } => {
            if let Some(attachment) = attachments.remove(&session_id) {
                attachment.abort();
            }
            manager
                .close(&workspace.id, &session_id)
                .map(|()| vec![ServerMessage::Closed { session_id }])
        }
        ClientMessage::CloseAll => manager.close_all(&workspace.id).map(|session_ids| {
            for attachment in attachments.drain().map(|(_, attachment)| attachment) {
                attachment.abort();
            }
            session_ids
                .into_iter()
                .map(|session_id| ServerMessage::Closed { session_id })
                .collect()
        }),
        ClientMessage::Ping { nonce } => Ok(vec![ServerMessage::Pong { nonce }]),
    };
    result.unwrap_or_else(|error| vec![ServerMessage::Error { error }])
}
fn install_attachment(
    mut attachment: SessionAttachment,
    event_tx: &mpsc::Sender<ServerMessage>,
    attachments: &mut HashMap<SessionId, JoinHandle<()>>,
    session_id: &SessionId,
) {
    let event_tx = event_tx.clone();
    let attached_id = session_id.clone();
    let task = tokio::spawn(async move {
        loop {
            match attachment.events.recv().await {
                Ok(HostTerminalEvent::Output {
                    session_id,
                    sequence,
                    data,
                }) => {
                    if event_tx
                        .send(ServerMessage::Output {
                            session_id,
                            sequence,
                            data,
                            replay: false,
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(HostTerminalEvent::Lifecycle(session)) => {
                    if event_tx
                        .send(ServerMessage::Lifecycle { session })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    let error = TerminalError::new(
                            TerminalErrorCode::OutputLagged,
                            "Terminal output exceeded the live connection buffer; reconnect to replay recent output",
                        )
                        .for_session(attached_id.clone());
                    if event_tx.send(ServerMessage::Error { error }).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    attachments.insert(session_id.clone(), task);
}
fn abort_attachments(attachments: &mut HashMap<SessionId, JoinHandle<()>>) {
    for attachment in attachments.drain().map(|(_, attachment)| attachment) {
        attachment.abort();
    }
}
fn protocol_error(message: &'static str) -> ServerMessage {
    ServerMessage::Error {
        error: TerminalError::new(TerminalErrorCode::InvalidProtocol, message),
    }
}
fn terminals() -> &'static HostTerminalManager {
    TERMINALS.get_or_init(|| HostTerminalManager::new(TerminalHostConfig::default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use syntaxis_terminal::TerminalSize;
    use syntaxis_workspace::{WorkspaceAvailability, WorkspaceIcon};

    fn workspace(root: &std::path::Path) -> WorkspaceRecord {
        WorkspaceRecord {
            id: WorkspaceId::new("workspace"),
            slug: "workspace".into(),
            name: "Workspace".into(),
            root: root.to_string_lossy().into_owned(),
            icon: WorkspaceIcon::default(),
            registered_at_unix_ms: 0,
            last_opened_unix_ms: 0,
            availability: WorkspaceAvailability::Available,
        }
    }

    #[tokio::test]
    async fn selecting_or_creating_a_session_keeps_one_socket_attachment() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = workspace(directory.path());
        let manager = HostTerminalManager::default();
        let (event_tx, _event_rx) = mpsc::channel(8);
        let mut attachments = HashMap::new();

        for name in ["first", "second"] {
            let messages = handle_message(
                ClientMessage::Create {
                    name: Some(name.into()),
                    size: TerminalSize::DEFAULT,
                },
                &workspace,
                &manager,
                &event_tx,
                &mut attachments,
            );
            assert!(matches!(
                messages.first(),
                Some(ServerMessage::Created { .. })
            ));
            assert_eq!(attachments.len(), 1);
        }

        abort_attachments(&mut attachments);
        manager.close_all(&workspace.id).unwrap();
    }
}
