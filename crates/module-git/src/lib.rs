//! Canonical Git UI, controller, and capability-grouped ports.

mod connection;
mod operations;
mod ports;
mod repository;
mod view;

pub use connection::GitConnectionForm;
pub use ports::{
    GitBranchPort, GitCheckoutPort, GitCommitCapabilities, GitConflictPort, GitConnectionPort,
    GitConnectionSettings, GitHistoryPort, GitHunkPort, GitMergePort, GitNetworkPort, GitPorts,
    GitRebasePort, GitRepositoryPort, GitRevertPort, GitTagPort, GitWorktreePort,
};
pub use view::GitView;
