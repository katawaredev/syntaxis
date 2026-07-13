//! Server/host PTY implementation for the shared terminal protocol.
#![cfg(not(target_arch = "wasm32"))]
mod manager;
mod replay;
pub use manager::{HostTerminalEvent, HostTerminalManager, SessionAttachment, TerminalHostConfig};
