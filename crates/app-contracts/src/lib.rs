//! Runtime-neutral application contracts shared by the shell and feature modules.

mod error;
mod events;
mod navigation;

#[cfg(target_arch = "wasm32")]
pub type PortHandle<T> = std::rc::Rc<T>;

#[cfg(not(target_arch = "wasm32"))]
pub type PortHandle<T> = std::sync::Arc<T>;

pub use error::{AppError, AppErrorCode, ErrorSource, RetryAdvice};
pub use events::{
    ChangeOrigin, OperationId, WorkspaceEvent, WorkspaceEventBus, WorkspaceEventDelivery,
    WorkspaceEventKind, WorkspaceEventPublishError, WorkspaceEventSubscription,
};
pub use navigation::{AiSettingsSection, FileLocation, NavigationIntent};
