#![allow(
    clippy::as_conversions,
    reason = "The runtime bridges a concrete socket into its object-safe port"
)]

use async_trait::async_trait;
use dioxus::fullstack::{WebSocketOptions, Websocket};
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, PortHandle, RetryAdvice};
use syntaxis_module_terminal::{TerminalPorts, TerminalSocket, TerminalTransportPort};
use syntaxis_terminal::{ClientMessage, RunCommand, ServerMessage};
use syntaxis_workspace::WorkspaceRecord;

use super::api::{self, TerminalEncoding};

pub(crate) fn terminal_ports() -> TerminalPorts {
    super::adapter::terminal_ports(
        PortHandle::new(DioxusTerminalTransport),
        DioxusTerminalCommands,
    )
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
impl super::RemoteTerminalCommandsTransport for DioxusTerminalCommands {
    async fn list(&self, workspace_id: String) -> Result<Vec<RunCommand>, AppError> {
        api::list_run_commands(workspace_id)
            .await
            .map_err(terminal_server_error)
    }

    async fn refresh(&self, workspace_id: String) -> Result<Vec<RunCommand>, AppError> {
        api::refresh_run_commands(workspace_id)
            .await
            .map_err(terminal_server_error)
    }

    async fn add(
        &self,
        workspace_id: String,
        label: String,
        command: String,
    ) -> Result<Vec<RunCommand>, AppError> {
        api::add_run_command(workspace_id, label, command)
            .await
            .map_err(terminal_server_error)
    }

    async fn delete(
        &self,
        workspace_id: String,
        command_id: String,
    ) -> Result<Vec<RunCommand>, AppError> {
        api::delete_run_command(workspace_id, command_id)
            .await
            .map_err(terminal_server_error)
    }
}

fn terminal_server_error(error: dioxus::prelude::ServerFnError) -> AppError {
    terminal_error(
        AppErrorCode::Internal,
        crate::client_error::server_error_message(error),
        RetryAdvice::Backoff,
    )
}

fn terminal_error(code: AppErrorCode, message: impl Into<String>, retry: RetryAdvice) -> AppError {
    AppError::new(code, message, retry, ErrorSource::Terminal)
}
