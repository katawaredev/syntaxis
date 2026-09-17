use async_trait::async_trait;
use syntaxis_app_contracts::{AppError, PortHandle};
use syntaxis_terminal::{ClientMessage, RunCommand, ServerMessage, SessionId};
use syntaxis_workspace::{WorkspaceChange, WorkspaceId, WorkspaceRecord};

use crate::{RendererAction, TerminalRendererEvent};

/// One normalized bidirectional terminal protocol connection.
#[async_trait(?Send)]
pub trait TerminalSocket {
    async fn send(&self, message: ClientMessage) -> Result<(), AppError>;
    async fn receive(&self) -> Result<ServerMessage, AppError>;
}

#[async_trait(?Send)]
pub trait TerminalRendererSession {
    async fn receive(&self) -> Result<TerminalRendererEvent, AppError>;
    async fn write(&self, data: Vec<u8>) -> Result<(), AppError>;
    async fn action(&self, action: RendererAction) -> Result<(), AppError>;
    fn close(&self);
}

#[async_trait(?Send)]
pub trait TerminalRendererPort: Send + Sync {
    async fn mount(
        &self,
        element_id: &str,
    ) -> Result<PortHandle<dyn TerminalRendererSession>, AppError>;
}

/// Optional interactive-session transport selected by the runtime.
#[async_trait(?Send)]
pub trait TerminalTransportPort: Send + Sync {
    async fn connect(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<Box<dyn TerminalSocket>, AppError>;
}

/// Project command discovery used by the Terminal run menu.
#[async_trait(?Send)]
pub trait TerminalCommandsPort: Send + Sync {
    async fn list(&self, workspace: &WorkspaceRecord) -> Result<Vec<RunCommand>, AppError>;
    async fn refresh(&self, workspace: &WorkspaceRecord) -> Result<Vec<RunCommand>, AppError>;
}

/// Optional persistence for user-defined project commands.
#[async_trait(?Send)]
pub trait TerminalCommandMutationsPort: Send + Sync {
    async fn add(
        &self,
        workspace: &WorkspaceRecord,
        label: &str,
        command: &str,
    ) -> Result<Vec<RunCommand>, AppError>;
    async fn delete(
        &self,
        workspace: &WorkspaceRecord,
        command_id: &str,
    ) -> Result<Vec<RunCommand>, AppError>;
}

/// Remembers the active session independently of the UI runtime.
#[async_trait(?Send)]
pub trait TerminalSessionPort: Send + Sync {
    async fn load(&self, workspace_id: &WorkspaceId) -> Result<Option<SessionId>, AppError>;
    async fn save(
        &self,
        workspace_id: &WorkspaceId,
        session_id: &SessionId,
    ) -> Result<(), AppError>;
}

/// Bounded result of one browser-local command execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalCommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub changes: Vec<WorkspaceChange>,
    pub reconciliation_succeeded: bool,
}

/// Optional isolated command runner used when interactive PTY transport is unavailable.
#[async_trait(?Send)]
pub trait TerminalCommandRunnerPort: Send + Sync {
    async fn ready(&self) -> Result<(), AppError>;
    async fn execute(
        &self,
        workspace: &WorkspaceRecord,
        command: &str,
    ) -> Result<TerminalCommandResult, AppError>;
    /// # Errors
    ///
    /// Returns an error when the active command cannot be cancelled.
    fn cancel(&self) -> Result<(), AppError>;
}

/// Structurally advertised Terminal capabilities.
#[derive(Clone, Default)]
pub struct TerminalPorts {
    transport: Option<PortHandle<dyn TerminalTransportPort>>,
    commands: Option<PortHandle<dyn TerminalCommandsPort>>,
    command_mutations: Option<PortHandle<dyn TerminalCommandMutationsPort>>,
    session: Option<PortHandle<dyn TerminalSessionPort>>,
    command_runner: Option<PortHandle<dyn TerminalCommandRunnerPort>>,
    renderer: Option<PortHandle<dyn TerminalRendererPort>>,
}

impl TerminalPorts {
    #[must_use]
    pub fn with_transport(mut self, transport: PortHandle<dyn TerminalTransportPort>) -> Self {
        self.transport = Some(transport);
        self
    }

    #[must_use]
    pub fn with_commands(mut self, commands: PortHandle<dyn TerminalCommandsPort>) -> Self {
        self.commands = Some(commands);
        self
    }

    #[must_use]
    pub fn with_session(mut self, session: PortHandle<dyn TerminalSessionPort>) -> Self {
        self.session = Some(session);
        self
    }

    pub fn transport(&self) -> Option<&PortHandle<dyn TerminalTransportPort>> {
        self.transport.as_ref()
    }

    pub fn commands(&self) -> Option<&PortHandle<dyn TerminalCommandsPort>> {
        self.commands.as_ref()
    }

    #[must_use]
    pub fn with_command_mutations(
        mut self,
        command_mutations: PortHandle<dyn TerminalCommandMutationsPort>,
    ) -> Self {
        self.command_mutations = Some(command_mutations);
        self
    }

    pub fn command_mutations(&self) -> Option<&PortHandle<dyn TerminalCommandMutationsPort>> {
        self.command_mutations.as_ref()
    }

    pub fn session(&self) -> Option<&PortHandle<dyn TerminalSessionPort>> {
        self.session.as_ref()
    }

    #[must_use]
    pub fn with_command_runner(
        mut self,
        command_runner: PortHandle<dyn TerminalCommandRunnerPort>,
    ) -> Self {
        self.command_runner = Some(command_runner);
        self
    }

    pub fn command_runner(&self) -> Option<&PortHandle<dyn TerminalCommandRunnerPort>> {
        self.command_runner.as_ref()
    }

    #[must_use]
    pub fn with_renderer(mut self, renderer: PortHandle<dyn TerminalRendererPort>) -> Self {
        self.renderer = Some(renderer);
        self
    }

    pub fn renderer(&self) -> Option<&PortHandle<dyn TerminalRendererPort>> {
        self.renderer.as_ref()
    }

    /// Verifies capability combinations that the shared controller relies on.
    ///
    /// # Errors
    ///
    /// Returns an error when the configured capabilities are inconsistent.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.transport.is_none() && self.command_runner.is_none() {
            return Err("Terminal requires an interactive transport or a command runner");
        }
        if self.transport.is_some() && self.renderer.is_none() {
            return Err("interactive Terminal transport requires a renderer");
        }
        if self.command_mutations.is_some() && self.commands.is_none() {
            return Err("Terminal command mutations require command discovery");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalPorts;

    #[test]
    fn optional_terminal_capabilities_are_absent_by_default() {
        let ports = TerminalPorts::default();
        assert!(ports.transport().is_none());
        assert!(ports.commands().is_none());
        assert!(ports.command_mutations().is_none());
        assert!(ports.session().is_none());
        assert!(ports.command_runner().is_none());
        assert!(ports.renderer().is_none());
        assert!(ports.validate().is_err());
    }
}
