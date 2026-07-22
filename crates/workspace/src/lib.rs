//! Platform-neutral workspace models and operation boundaries.

mod browser;
mod error;
mod events;
mod filesystem;
mod mock;
mod mock_browser;
mod mock_files;
mod operations;
mod runtime;
mod session;
mod workspace;

pub use browser::{BrowseDirectory, BrowseRoot, WorkspaceBrowser};
pub use error::{ErrorCode, WorkspaceError, WorkspaceResult};
pub use events::{ChangeKind, EventBatch, WorkspaceChange};
pub use filesystem::{BinaryFile, EntryKind, FileEntry, FileVersion, RelativePath, TextFile};
pub use mock::MockWorkspaceRegistry;
pub use mock_browser::MockWorkspaceBrowser;
pub use mock_files::MockWorkspaceFiles;
pub use operations::{WorkspaceFiles, WorkspaceRegistry};
pub use runtime::{
    ExecutionLocation, RuntimeCapabilities, RuntimeCapability, RuntimeIdentity, RuntimeState,
};
pub use session::{FileSession, WorkspaceSession};
pub use workspace::{
    WorkspaceAvailability, WorkspaceCleanupEntry, WorkspaceIcon, WorkspaceIconSymbol, WorkspaceId,
    WorkspaceLanguage, WorkspaceProfile, WorkspaceRecord, WorkspaceTechnology,
};
