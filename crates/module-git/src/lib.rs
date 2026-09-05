//! Canonical Git UI, controller, and capability-grouped ports.

mod operations;
mod ports;
mod repository;
mod view;

pub use ports::{
    GitBranchPort, GitCheckoutPort, GitHistoryPort, GitHunkPort, GitMergePort, GitNetworkPort,
    GitPorts, GitRebasePort, GitRepositoryPort, GitRevertPort, GitTagPort, GitWorktreePort,
};
pub use view::GitView;
