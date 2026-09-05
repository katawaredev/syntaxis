//! Main application service composition.

#[cfg(any(feature = "host", test))]
mod files;
mod remote;

use syntaxis_app_contracts::WorkspaceEventBus;
use syntaxis_app_shell::AppServices;

#[cfg(any(feature = "host", test))]
pub use files::{HostFilesSession, HostWorkspaceSearch, host_workspace_registry};
pub use remote::{
    RemoteFilesTransport, RemoteWorkspaceFiles, app_error_from_workspace, remote_files_ports,
};

pub fn services() -> AppServices {
    let services = AppServices::new(WorkspaceEventBus::default());
    #[cfg(feature = "host")]
    {
        use std::sync::Arc;

        use syntaxis_app_contracts::PortHandle;
        use syntaxis_module_files::FilesPorts;
        use syntaxis_workspace::WorkspaceFiles;
        use syntaxis_workspace_host::HostWorkspaceFiles;

        let files: PortHandle<dyn WorkspaceFiles> = Arc::new(HostWorkspaceFiles);
        let search = Arc::new(HostWorkspaceSearch::default());
        let session = Arc::new(HostFilesSession);
        return services.with_files(FilesPorts::new(files, search, session));
    }
    #[cfg(not(feature = "host"))]
    services
}
