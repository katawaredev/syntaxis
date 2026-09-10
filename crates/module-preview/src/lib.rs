//! Canonical Preview UI and runtime-neutral preview lifecycle ports.

mod models;
mod ports;
mod process_view;
mod static_view;

pub use models::{
    PreviewCandidate, PreviewConfig, PreviewLease, PreviewLifecycle, PreviewProcessStatus,
    PreviewSession, PreviewShare, PreviewTarget,
};
pub use ports::{
    PreviewConfigPort, PreviewPort, PreviewPorts, PreviewProcessPort, PreviewSharePort,
};
pub use static_view::PreviewView;
