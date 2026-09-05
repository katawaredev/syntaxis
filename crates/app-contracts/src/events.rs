use std::fmt;

use futures_channel::mpsc;
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use syntaxis_workspace::{WorkspaceChange, WorkspaceId};

use crate::PortHandle;

const DEFAULT_SUBSCRIBER_CAPACITY: usize = 64;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct OperationId(pub String);

impl OperationId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeOrigin {
    Files,
    Terminal,
    Git,
    Ai,
    External,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkspaceEventKind {
    Changes { changes: Vec<WorkspaceChange> },
    ResyncRequired,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceEvent {
    pub workspace_id: WorkspaceId,
    pub sequence: u64,
    pub operation_id: Option<OperationId>,
    pub origin: ChangeOrigin,
    pub kind: WorkspaceEventKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspaceEventDelivery {
    Event(WorkspaceEvent),
    ResyncRequired { missed_events: usize },
    Closed,
}

#[cfg(target_arch = "wasm32")]
type BusState = std::cell::RefCell<BusInner>;
#[cfg(not(target_arch = "wasm32"))]
type BusState = std::sync::Mutex<BusInner>;

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Default)]
struct LagState(PortHandle<std::cell::Cell<usize>>);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Default)]
struct LagState(PortHandle<std::sync::atomic::AtomicUsize>);

impl LagState {
    #[cfg(target_arch = "wasm32")]
    fn add(&self, amount: usize) {
        self.0.set(self.0.get().saturating_add(amount));
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn add(&self, amount: usize) {
        self.0
            .fetch_add(amount, std::sync::atomic::Ordering::Relaxed);
    }

    #[cfg(target_arch = "wasm32")]
    fn take(&self) -> usize {
        self.0.replace(0)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn take(&self) -> usize {
        self.0.swap(0, std::sync::atomic::Ordering::Relaxed)
    }

    #[cfg(target_arch = "wasm32")]
    fn is_lagged(&self) -> bool {
        self.0.get() > 0
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn is_lagged(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed) > 0
    }
}

struct Subscriber {
    sender: mpsc::Sender<WorkspaceEvent>,
    lag: LagState,
}

struct BusInner {
    next_sequence: u64,
    subscribers: Vec<Subscriber>,
}

#[derive(Clone)]
pub struct WorkspaceEventBus {
    inner: PortHandle<BusState>,
    subscriber_capacity: usize,
}

impl WorkspaceEventBus {
    pub fn new(subscriber_capacity: usize) -> Self {
        Self {
            inner: PortHandle::new(BusState::new(BusInner {
                next_sequence: 1,
                subscribers: Vec::new(),
            })),
            subscriber_capacity: subscriber_capacity.max(1),
        }
    }

    pub fn subscribe(&self) -> WorkspaceEventSubscription {
        let (sender, receiver) = mpsc::channel(self.subscriber_capacity);
        let lag = LagState::default();
        self.with_inner(|inner| {
            inner.subscribers.push(Subscriber {
                sender,
                lag: lag.clone(),
            });
        });
        WorkspaceEventSubscription { receiver, lag }
    }

    /// Publishes exact changes to every active subscriber.
    ///
    /// # Errors
    ///
    /// Returns an error when the change list is empty or contains a change for a
    /// different workspace.
    pub fn publish_changes(
        &self,
        workspace_id: WorkspaceId,
        operation_id: Option<OperationId>,
        origin: ChangeOrigin,
        changes: Vec<WorkspaceChange>,
    ) -> Result<WorkspaceEvent, WorkspaceEventPublishError> {
        if changes.is_empty() {
            return Err(WorkspaceEventPublishError::EmptyChanges);
        }
        if changes
            .iter()
            .any(|change| change.workspace_id != workspace_id)
        {
            return Err(WorkspaceEventPublishError::WorkspaceMismatch);
        }
        Ok(self.publish(
            workspace_id,
            operation_id,
            origin,
            WorkspaceEventKind::Changes { changes },
        ))
    }

    pub fn publish_resync(
        &self,
        workspace_id: WorkspaceId,
        operation_id: Option<OperationId>,
        origin: ChangeOrigin,
    ) -> WorkspaceEvent {
        self.publish(
            workspace_id,
            operation_id,
            origin,
            WorkspaceEventKind::ResyncRequired,
        )
    }

    fn publish(
        &self,
        workspace_id: WorkspaceId,
        operation_id: Option<OperationId>,
        origin: ChangeOrigin,
        kind: WorkspaceEventKind,
    ) -> WorkspaceEvent {
        self.with_inner(|inner| {
            let event = WorkspaceEvent {
                workspace_id,
                sequence: inner.next_sequence,
                operation_id,
                origin,
                kind,
            };
            inner.next_sequence = inner.next_sequence.saturating_add(1);
            inner.subscribers.retain_mut(|subscriber| {
                if subscriber.sender.is_closed() {
                    return false;
                }
                if subscriber.lag.is_lagged() {
                    subscriber.lag.add(1);
                    return true;
                }
                match subscriber.sender.try_send(event.clone()) {
                    Ok(()) => true,
                    Err(error) if error.is_full() => {
                        subscriber.lag.add(1);
                        true
                    }
                    Err(_) => false,
                }
            });
            event
        })
    }

    #[cfg(target_arch = "wasm32")]
    fn with_inner<T>(&self, operation: impl FnOnce(&mut BusInner) -> T) -> T {
        operation(&mut self.inner.borrow_mut())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn with_inner<T>(&self, operation: impl FnOnce(&mut BusInner) -> T) -> T {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        operation(&mut inner)
    }
}

impl Default for WorkspaceEventBus {
    fn default() -> Self {
        Self::new(DEFAULT_SUBSCRIBER_CAPACITY)
    }
}

pub struct WorkspaceEventSubscription {
    receiver: mpsc::Receiver<WorkspaceEvent>,
    lag: LagState,
}

impl WorkspaceEventSubscription {
    pub async fn next(&mut self) -> WorkspaceEventDelivery {
        if let Some(delivery) = self.take_resync() {
            return delivery;
        }
        let event = std::future::poll_fn(|context| {
            std::pin::Pin::new(&mut self.receiver).poll_next(context)
        })
        .await;
        if let Some(delivery) = self.take_resync() {
            return delivery;
        }
        event.map_or(
            WorkspaceEventDelivery::Closed,
            WorkspaceEventDelivery::Event,
        )
    }

    fn take_resync(&mut self) -> Option<WorkspaceEventDelivery> {
        let missed_events = self.lag.take();
        if missed_events == 0 {
            return None;
        }
        while self.receiver.try_recv().is_ok() {}
        Some(WorkspaceEventDelivery::ResyncRequired { missed_events })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceEventPublishError {
    EmptyChanges,
    WorkspaceMismatch,
}

impl fmt::Display for WorkspaceEventPublishError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptyChanges => "workspace change events cannot be empty",
            Self::WorkspaceMismatch => "all changes must belong to the event workspace",
        })
    }
}

impl std::error::Error for WorkspaceEventPublishError {}

#[cfg(test)]
mod tests {
    use futures_lite::future::block_on;
    use syntaxis_workspace::{ChangeKind, RelativePath};

    use super::*;

    fn change(workspace_id: &WorkspaceId, path: &str) -> WorkspaceChange {
        WorkspaceChange {
            workspace_id: workspace_id.clone(),
            path: RelativePath::try_from(path).expect("test path should be valid"),
            kind: ChangeKind::Modified,
        }
    }

    #[test]
    fn events_are_sequenced_once_for_every_subscriber() {
        let bus = WorkspaceEventBus::new(2);
        let workspace_id = WorkspaceId::new("workspace-1");
        let mut first = bus.subscribe();
        let mut second = bus.subscribe();
        let published = bus
            .publish_changes(
                workspace_id.clone(),
                None,
                ChangeOrigin::Files,
                vec![change(&workspace_id, "src/main.rs")],
            )
            .expect("valid event should publish");

        assert_eq!(published.sequence, 1);
        assert_eq!(
            block_on(first.next()),
            WorkspaceEventDelivery::Event(published.clone())
        );
        assert_eq!(
            block_on(second.next()),
            WorkspaceEventDelivery::Event(published)
        );
    }

    #[test]
    fn lagged_subscribers_receive_an_explicit_resync() {
        let bus = WorkspaceEventBus::new(1);
        let workspace_id = WorkspaceId::new("workspace-1");
        let mut subscription = bus.subscribe();
        for path in ["one.rs", "two.rs", "three.rs", "four.rs"] {
            bus.publish_changes(
                workspace_id.clone(),
                None,
                ChangeOrigin::External,
                vec![change(&workspace_id, path)],
            )
            .expect("valid event should publish");
        }

        assert_eq!(
            block_on(subscription.next()),
            WorkspaceEventDelivery::ResyncRequired { missed_events: 2 }
        );
    }

    #[test]
    fn mixed_workspace_changes_are_rejected() {
        let bus = WorkspaceEventBus::default();
        let expected = WorkspaceId::new("workspace-1");
        let different = WorkspaceId::new("workspace-2");
        assert_eq!(
            bus.publish_changes(
                expected,
                None,
                ChangeOrigin::Git,
                vec![change(&different, "src/main.rs")],
            ),
            Err(WorkspaceEventPublishError::WorkspaceMismatch)
        );
    }
}
