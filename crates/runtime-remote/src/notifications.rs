#![allow(
    clippy::as_conversions,
    reason = "The runtime bridges a concrete socket into its object-safe port"
)]

use async_trait::async_trait;
use dioxus::fullstack::{WebSocketOptions, Websocket};
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, PortHandle, RetryAdvice};
use syntaxis_app_shell::{NotificationPort, NotificationSocket};
use syntaxis_notifications::{NotificationClientMessage, NotificationServerMessage};

#[derive(Clone, Copy, Debug, Default)]
struct DioxusNotifications;

pub(crate) fn notification_port() -> PortHandle<dyn NotificationPort> {
    PortHandle::new(DioxusNotifications)
}

#[async_trait(?Send)]
impl NotificationPort for DioxusNotifications {
    async fn connect(&self) -> Result<Box<dyn NotificationSocket>, AppError> {
        crate::ai::api::notification_socket(WebSocketOptions::new())
            .await
            .map(|socket| {
                Box::new(DioxusNotificationSocket { socket }) as Box<dyn NotificationSocket>
            })
            .map_err(notification_error)
    }
}

struct DioxusNotificationSocket {
    socket: Websocket<
        NotificationClientMessage,
        NotificationServerMessage,
        crate::ai::api::AgentEncoding,
    >,
}

#[async_trait(?Send)]
impl NotificationSocket for DioxusNotificationSocket {
    async fn send(&self, message: NotificationClientMessage) -> Result<(), AppError> {
        self.socket.send(message).await.map_err(notification_error)
    }

    async fn receive(&self) -> Result<NotificationServerMessage, AppError> {
        self.socket.recv().await.map_err(notification_error)
    }
}

fn notification_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        AppErrorCode::Offline,
        error.to_string(),
        RetryAdvice::Backoff,
        ErrorSource::Application,
    )
}
