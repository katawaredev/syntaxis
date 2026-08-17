use std::time::{SystemTime, UNIX_EPOCH};

use dioxus::prelude::*;
use syntaxis_workspace::{WorkspaceRecord, WorkspaceSection};

use super::client::{list_workspace_availability, list_workspaces};

/// Route-stable workspace data with stale-while-revalidate behavior.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct WorkspaceListCache {
    records: Signal<Vec<WorkspaceRecord>>,
    error: Signal<Option<String>>,
    loaded: Signal<bool>,
    loading: Signal<bool>,
    request_revision: Signal<u64>,
}

impl WorkspaceListCache {
    pub(crate) fn records(self) -> Vec<WorkspaceRecord> {
        (self.records)()
    }

    pub(crate) fn error(self) -> Option<String> {
        (self.error)()
    }

    pub(crate) fn is_loaded(self) -> bool {
        (self.loaded)()
    }

    pub(crate) fn ensure(self) {
        if !(self.loaded)() && !(self.loading)() {
            self.load();
        }
    }

    pub(crate) fn refresh(self) {
        if !(self.loading)() {
            self.load();
        }
    }

    pub(crate) fn touch(mut self, workspace_id: &str) {
        let mut records = self.records.write();
        let Some(index) = records
            .iter()
            .position(|workspace| workspace.id.0 == workspace_id)
        else {
            return;
        };
        let mut workspace = records.remove(index);
        if let Ok(elapsed) = SystemTime::now().duration_since(UNIX_EPOCH) {
            workspace.last_opened_unix_ms =
                i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX);
        }
        records.insert(0, workspace);
    }

    pub(crate) fn set_last_section(
        mut self,
        workspace_id: &str,
        section: WorkspaceSection,
    ) {
        if let Some(workspace) = self
            .records
            .write()
            .iter_mut()
            .find(|workspace| workspace.id.0 == workspace_id)
        {
            workspace.last_section = section;
        }
    }

    fn load(mut self) {
        self.loading.set(true);
        self.error.set(None);
        let revision = (self.request_revision)().saturating_add(1);
        self.request_revision.set(revision);
        dioxus::core::spawn_forever(async move {
            let result = list_workspaces().await;
            if (self.request_revision)() != revision {
                return;
            }
            let records = match result {
                Ok(records) => records,
                Err(message) => {
                    self.error.set(Some(message));
                    self.loaded.set(true);
                    self.loading.set(false);
                    return;
                }
            };
            self.records.set(records);
            self.loaded.set(true);
            self.loading.set(false);

            // Filesystem probes can block indefinitely on unavailable mounts. Resolve them
            // after metadata is visible, and discard stale results from superseded requests.
            if let Ok(records) = list_workspace_availability().await
                && (self.request_revision)() == revision
            {
                self.records.set(records);
            }
        });
    }
}

pub(crate) fn use_workspace_list_cache() -> WorkspaceListCache {
    WorkspaceListCache {
        records: use_signal(Vec::new),
        error: use_signal(|| None),
        loaded: use_signal(|| false),
        loading: use_signal(|| false),
        request_revision: use_signal(|| 0),
    }
}
