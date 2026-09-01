//! Browser-native workspace implementations.
//!
//! The initial backend uses the Origin Private File System through `web-sys`.
//! It deliberately contains no handwritten JavaScript bridge.
#[cfg(target_arch = "wasm32")]
mod opfs;
#[cfg(target_arch = "wasm32")]
mod search;
#[cfg(target_arch = "wasm32")]
mod storage;
#[cfg(target_arch = "wasm32")]
pub use opfs::{
    OpfsWorkspaceFiles, SavedDirectory, SelectedDirectory, local_directory_picker_supported,
    restore_local_directory, select_local_directory, use_private_workspace,
};
#[cfg(target_arch = "wasm32")]
pub use search::{BrowserSearchHit, search};
