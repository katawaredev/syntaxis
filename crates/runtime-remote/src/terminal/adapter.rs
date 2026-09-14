use async_trait::async_trait;
use syntaxis_app_contracts::{AppError, PortHandle};
use syntaxis_module_terminal::{
    TerminalCommandMutationsPort, TerminalCommandsPort, TerminalPorts, TerminalTransportPort,
};
use syntaxis_terminal::RunCommand;
use syntaxis_workspace::WorkspaceRecord;

#[async_trait(?Send)]
pub trait RemoteTerminalCommandsTransport: Clone + Send + Sync + 'static {
    async fn list(&self, workspace_id: String) -> Result<Vec<RunCommand>, AppError>;
    async fn refresh(&self, workspace_id: String) -> Result<Vec<RunCommand>, AppError>;
    async fn add(
        &self,
        workspace_id: String,
        label: String,
        command: String,
    ) -> Result<Vec<RunCommand>, AppError>;
    async fn delete(
        &self,
        workspace_id: String,
        command_id: String,
    ) -> Result<Vec<RunCommand>, AppError>;
}

#[derive(Clone)]
pub struct RemoteTerminalCommands<T> {
    transport: T,
}

pub fn terminal_ports<T>(
    transport: PortHandle<dyn TerminalTransportPort>,
    commands: T,
) -> TerminalPorts
where
    T: RemoteTerminalCommandsTransport,
{
    TerminalPorts::default()
        .with_transport(transport)
        .with_commands(PortHandle::new(RemoteTerminalCommands {
            transport: commands.clone(),
        }))
        .with_command_mutations(PortHandle::new(RemoteTerminalCommands {
            transport: commands,
        }))
        .with_session(PortHandle::new(crate::BrowserTerminalSession))
        .with_renderer(PortHandle::new(crate::DioxusTerminalRenderer))
}

#[async_trait(?Send)]
impl<T: RemoteTerminalCommandsTransport> TerminalCommandsPort for RemoteTerminalCommands<T> {
    async fn list(&self, workspace: &WorkspaceRecord) -> Result<Vec<RunCommand>, AppError> {
        self.transport.list(workspace.id.0.clone()).await
    }

    async fn refresh(&self, workspace: &WorkspaceRecord) -> Result<Vec<RunCommand>, AppError> {
        self.transport.refresh(workspace.id.0.clone()).await
    }
}

#[async_trait(?Send)]
impl<T: RemoteTerminalCommandsTransport> TerminalCommandMutationsPort for RemoteTerminalCommands<T> {
    async fn add(
        &self,
        workspace: &WorkspaceRecord,
        label: &str,
        command: &str,
    ) -> Result<Vec<RunCommand>, AppError> {
        self.transport
            .add(workspace.id.0.clone(), label.to_owned(), command.to_owned())
            .await
    }

    async fn delete(
        &self,
        workspace: &WorkspaceRecord,
        command_id: &str,
    ) -> Result<Vec<RunCommand>, AppError> {
        self.transport
            .delete(workspace.id.0.clone(), command_id.to_owned())
            .await
    }
}
