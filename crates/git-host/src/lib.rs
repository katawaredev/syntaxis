//! Host implementation of Git operations using the system `git` executable.

#![cfg(not(target_arch = "wasm32"))]

mod operations;
mod parser;
mod runner;
mod worktrees;

pub use runner::{HostGit, HostGitConfig};
