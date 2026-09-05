use async_trait::async_trait;
use dioxus::fullstack::{WebSocketOptions, Websocket};
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, PortHandle, RetryAdvice};
use syntaxis_module_terminal::{
    TerminalCommandsPort, TerminalPorts, TerminalSessionPort, TerminalSocket, TerminalTransportPort,
};
use syntaxis_terminal::{ClientMessage, RunCommand, ServerMessage, SessionId};
use syntaxis_workspace::{WorkspaceId, WorkspaceRecord};

use super::api::{self, TerminalEncoding};

pub(crate) fn terminal_ports() -> TerminalPorts {
    TerminalPorts::default()
        .with_transport(PortHandle::new(DioxusTerminalTransport))
        .with_commands(PortHandle::new(DioxusTerminalCommands))
        .with_session(PortHandle::new(DioxusTerminalSession))
}

#[derive(Clone, Copy, Debug, Default)]
struct DioxusTerminalTransport;

#[async_trait(?Send)]
impl TerminalTransportPort for DioxusTerminalTransport {
    async fn connect(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<Box<dyn TerminalSocket>, AppError> {
        api::terminal_socket(workspace.id.0.clone(), WebSocketOptions::new())
            .await
            .map(|socket| Box::new(DioxusTerminalSocket { socket }) as Box<dyn TerminalSocket>)
            .map_err(terminal_server_error)
    }
}

struct DioxusTerminalSocket {
    socket: Websocket<ClientMessage, ServerMessage, TerminalEncoding>,
}

#[async_trait(?Send)]
impl TerminalSocket for DioxusTerminalSocket {
    async fn send(&self, message: ClientMessage) -> Result<(), AppError> {
        self.socket.send(message).await.map_err(|error| {
            terminal_error(
                AppErrorCode::Offline,
                error.to_string(),
                RetryAdvice::Backoff,
            )
        })
    }

    async fn receive(&self) -> Result<ServerMessage, AppError> {
        self.socket.recv().await.map_err(|error| {
            terminal_error(
                AppErrorCode::Offline,
                error.to_string(),
                RetryAdvice::Backoff,
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct DioxusTerminalCommands;

#[async_trait(?Send)]
impl TerminalCommandsPort for DioxusTerminalCommands {
    async fn list(&self, workspace: &WorkspaceRecord) -> Result<Vec<RunCommand>, AppError> {
        api::list_run_commands(workspace.id.0.clone())
            .await
            .map_err(terminal_server_error)
    }

    async fn refresh(&self, workspace: &WorkspaceRecord) -> Result<Vec<RunCommand>, AppError> {
        api::refresh_run_commands(workspace.id.0.clone())
            .await
            .map_err(terminal_server_error)
    }

    async fn add(
        &self,
        workspace: &WorkspaceRecord,
        label: &str,
        command: &str,
    ) -> Result<Vec<RunCommand>, AppError> {
        api::add_run_command(
            workspace.id.0.clone(),
            label.to_owned(),
            command.to_owned(),
        )
        .await
        .map_err(terminal_server_error)
    }

    async fn delete(
        &self,
        workspace: &WorkspaceRecord,
        command_id: &str,
    ) -> Result<Vec<RunCommand>, AppError> {
        api::delete_run_command(workspace.id.0.clone(), command_id.to_owned())
            .await
            .map_err(terminal_server_error)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct DioxusTerminalSession;

#[async_trait(?Send)]
impl TerminalSessionPort for DioxusTerminalSession {
    async fn load(&self, workspace_id: &WorkspaceId) -> Result<Option<SessionId>, AppError> {
        crate::storage::get(format!("syntaxis.terminal.active.{}", workspace_id.0))
            .await
            .map(|session| session.map(SessionId::new))
            .map_err(|error| {
                terminal_error(
                    AppErrorCode::Internal,
                    error,
                    RetryAdvice::AfterUserAction,
                )
            })
    }

    async fn save(
        &self,
        workspace_id: &WorkspaceId,
        session_id: &SessionId,
    ) -> Result<(), AppError> {
        crate::storage::set(
            format!("syntaxis.terminal.active.{}", workspace_id.0),
            session_id.0.clone(),
        )
        .await
        .map_err(|error| {
            terminal_error(
                AppErrorCode::Internal,
                error,
                RetryAdvice::AfterUserAction,
            )
        })
    }
}

fn terminal_server_error(error: dioxus::prelude::ServerFnError) -> AppError {
    terminal_error(
        AppErrorCode::Internal,
        crate::client_error::server_error_message(error),
        RetryAdvice::Backoff,
    )
}

fn terminal_error(
    code: AppErrorCode,
    message: impl Into<String>,
    retry: RetryAdvice,
) -> AppError {
    AppError::new(code, message, retry, ErrorSource::Terminal)
}
