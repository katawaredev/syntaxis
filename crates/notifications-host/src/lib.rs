//! Host-side application notification storage and fan-out.
#![cfg(not(target_arch = "wasm32"))]

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, OnceLock},
};
use syntaxis_notifications::{AppNotification, NotificationServerMessage, NotificationTarget};
use tokio::sync::broadcast;

const EVENT_CAPACITY: usize = 512;
static NOTIFICATIONS: OnceLock<HostNotificationHub> = OnceLock::new();

pub fn notifications() -> &'static HostNotificationHub {
    NOTIFICATIONS.get_or_init(HostNotificationHub::default)
}

#[derive(Clone)]
pub struct HostNotificationHub {
    items: Arc<Mutex<HashMap<(String, NotificationTarget), AppNotification>>>,
    events: broadcast::Sender<NotificationServerMessage>,
}

impl Default for HostNotificationHub {
    fn default() -> Self {
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        Self {
            items: Arc::new(Mutex::new(HashMap::new())),
            events,
        }
    }
}

impl HostNotificationHub {
    pub fn snapshot(&self) -> Vec<AppNotification> {
        let mut items = lock(&self.items).values().cloned().collect::<Vec<_>>();
        items.sort_by_key(|notification| std::cmp::Reverse(notification.created_at_ms));
        items
    }

    pub fn subscribe(&self) -> broadcast::Receiver<NotificationServerMessage> {
        self.events.subscribe()
    }

    pub fn clear(&self, workspace_id: &str, target: &NotificationTarget) {
        let key = (workspace_id.to_owned(), target.clone());
        if lock(&self.items).remove(&key).is_some() {
            let _ = self.events.send(NotificationServerMessage::Removed {
                workspace_id: workspace_id.to_owned(),
                target: target.clone(),
            });
        }
    }

    pub fn clear_workspace(&self, workspace_id: &str) {
        let removed = {
            let mut items = lock(&self.items);
            let removed = items
                .keys()
                .filter(|(candidate, _)| candidate == workspace_id)
                .cloned()
                .collect::<Vec<_>>();
            for key in &removed {
                items.remove(key);
            }
            removed
        };
        for (workspace_id, target) in removed {
            let _ = self.events.send(NotificationServerMessage::Removed {
                workspace_id,
                target,
            });
        }
    }

    pub fn publish(&self, notification: AppNotification) {
        let key = (
            notification.workspace_id.clone(),
            notification.target.clone(),
        );
        lock(&self.items).insert(key, notification.clone());
        let _ = self
            .events
            .send(NotificationServerMessage::Upsert { notification });
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use syntaxis_notifications::{NotificationKind, NotificationTarget};

    fn notification(target: NotificationTarget, created_at_ms: u64) -> AppNotification {
        AppNotification {
            workspace_id: "workspace".into(),
            workspace_slug: "project".into(),
            workspace_name: "Project".into(),
            target,
            title: "Task".into(),
            kind: NotificationKind::Completed,
            message: "Finished".into(),
            created_at_ms,
        }
    }

    #[test]
    fn publish_replaces_only_the_same_target() {
        let hub = HostNotificationHub::default();
        let target = NotificationTarget::Terminal {
            session_id: "one".into(),
        };
        hub.publish(notification(target.clone(), 1));
        hub.publish(notification(target, 2));
        hub.publish(notification(
            NotificationTarget::Agent {
                session_id: "one".into(),
            },
            3,
        ));
        let snapshot = hub.snapshot();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].created_at_ms, 3);
        assert_eq!(snapshot[1].created_at_ms, 2);
    }
}
