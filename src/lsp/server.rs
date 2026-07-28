use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use dioxus::prelude::ServerFnError;
use futures_util::{SinkExt, StreamExt};
use rand_core::{OsRng, RngCore};
use serde::Deserialize;
use syntaxis_editor::language_server_by_id;
use syntaxis_lsp_host::{resolve_language_server, LanguageServer};
use syntaxis_workspace::WorkspaceId;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::LanguageServiceConnection;

const TICKET_LIFETIME: Duration = Duration::from_secs(30);
static TICKETS: OnceLock<Mutex<HashMap<String, LanguageServiceTicket>>> = OnceLock::new();
static SERVER_SLOTS: OnceLock<std::sync::Arc<Semaphore>> = OnceLock::new();
const MAX_LANGUAGE_SERVERS: usize = 4;

struct LanguageServiceTicket {
    root: std::path::PathBuf,
    server_id: String,
    expires_at: Instant,
}

#[derive(Deserialize)]
pub(crate) struct SocketQuery {
    ticket: String,
}

pub(crate) async fn open_language_service(
    workspace_id: String,
    server_id: String,
) -> Result<LanguageServiceConnection, ServerFnError> {
    let server = language_server_by_id(&server_id)
        .ok_or_else(|| service_error(404, "This language server is not supported"))?;
    let workspace =
        crate::workspace::api::server::workspace_by_id(&WorkspaceId::new(workspace_id.clone()))
            .await?;
    let project_package = server.project_local.map(|local| local.package);
    let minimum_project_major = server.project_local.and_then(|local| local.minimum_major);
    if resolve_language_server(
        std::path::Path::new(&workspace.root),
        server.executable,
        project_package,
        minimum_project_major,
    )
    .await
    .map_err(|message| service_error(503, &message))?
    .is_none()
    {
        return Err(service_error(
            503,
            &format!(
                "{} is not installed in this project's root dependencies or active Mise configuration. Install the project dependencies, add it to Mise, or run Bootstrap when the project has no Mise configuration.",
                server.label
            ),
        ));
    }
    let root_uri = url::Url::from_directory_path(&workspace.root)
        .map_err(|()| service_error(500, "Could not create the language-server workspace URI"))?
        .to_string();
    let ticket = issue_ticket(workspace.root.into(), server.id);
    Ok(LanguageServiceConnection {
        server_id: server.id.into(),
        server_name: server.label.into(),
        session_key: format!("{workspace_id}:{}", server.id),
        endpoint: format!("/api/lsp-socket?ticket={ticket}"),
        root_uri,
    })
}

#[axum::debug_handler]
pub(crate) async fn socket(
    Query(query): Query<SocketQuery>,
    upgrade: WebSocketUpgrade,
) -> Response {
    let Some(ticket) = consume_ticket(&query.ticket) else {
        return (StatusCode::UNAUTHORIZED, "Invalid language-service ticket").into_response();
    };
    upgrade
        .max_message_size(syntaxis_lsp_host::MAX_LSP_MESSAGE_BYTES)
        .on_upgrade(move |socket| async move {
            let Some(permit) = acquire_server_slot() else {
                return;
            };
            proxy_language_server(socket, ticket, permit).await;
        })
        .into_response()
}

async fn proxy_language_server(
    socket: WebSocket,
    ticket: LanguageServiceTicket,
    _permit: OwnedSemaphorePermit,
) {
    let Some(definition) = language_server_by_id(&ticket.server_id) else {
        return;
    };
    let project_package = definition.project_local.map(|local| local.package);
    let minimum_project_major = definition
        .project_local
        .and_then(|local| local.minimum_major);
    let Ok(Some(executable)) = resolve_language_server(
        &ticket.root,
        definition.executable,
        project_package,
        minimum_project_major,
    )
    .await
    else {
        return;
    };
    let Ok(server) = LanguageServer::start_mise(&ticket.root, &executable, definition.arguments)
    else {
        return;
    };
    let LanguageServer {
        mut child,
        mut reader,
        mut writer,
    } = server;
    let (mut outgoing, mut incoming) = socket.split();
    loop {
        tokio::select! {
            client_message = incoming.next() => {
                match client_message {
                    Some(Ok(Message::Text(message))) => {
                        if writer.send(message.as_str()).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_) | Message::Binary(_)) | Err(_)) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        if outgoing.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                }
            }
            server_message = reader.receive() => {
                match server_message {
                    Ok(Some(message)) => {
                        if outgoing.send(Message::Text(message.into())).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) | Err(_) => break,
                }
            }
            _ = child.wait() => break,
        }
    }
    let _ = child.kill().await;
}

fn issue_ticket(root: std::path::PathBuf, server_id: &str) -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let ticket = URL_SAFE_NO_PAD.encode(bytes);
    let now = Instant::now();
    let mut tickets = tickets().lock().expect("LSP ticket mutex poisoned");
    tickets.retain(|_, ticket| ticket.expires_at > now);
    tickets.insert(
        ticket.clone(),
        LanguageServiceTicket {
            root,
            server_id: server_id.into(),
            expires_at: now + TICKET_LIFETIME,
        },
    );
    ticket
}

fn consume_ticket(value: &str) -> Option<LanguageServiceTicket> {
    let now = Instant::now();
    let mut tickets = tickets().lock().expect("LSP ticket mutex poisoned");
    tickets.retain(|_, ticket| ticket.expires_at > now);
    tickets.remove(value)
}

fn tickets() -> &'static Mutex<HashMap<String, LanguageServiceTicket>> {
    TICKETS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn acquire_server_slot() -> Option<OwnedSemaphorePermit> {
    std::sync::Arc::clone(
        SERVER_SLOTS.get_or_init(|| std::sync::Arc::new(Semaphore::new(MAX_LANGUAGE_SERVERS))),
    )
    .try_acquire_owned()
    .ok()
}

fn service_error(code: u16, message: &str) -> ServerFnError {
    ServerFnError::ServerError {
        message: message.into(),
        code,
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_tickets_are_single_use() {
        let ticket = issue_ticket("/tmp/workspace".into(), "rust-analyzer");
        assert!(consume_ticket(&ticket).is_some());
        assert!(consume_ticket(&ticket).is_none());
    }
}
