use dioxus::prelude::*;
use syntaxis_workspace::WorkspaceRecord;

use super::client::list_workspaces;

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

    pub(crate) fn is_loading(self) -> bool {
        (self.loading)()
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

    fn load(mut self) {
        self.loading.set(true);
        self.error.set(None);
        let revision = (self.request_revision)().saturating_add(1);
        self.request_revision.set(revision);
        spawn(async move {
            let result = list_workspaces().await;
            if (self.request_revision)() != revision {
                return;
            }
            match result {
                Ok(records) => self.records.set(records),
                Err(message) => self.error.set(Some(message)),
            }
            self.loaded.set(true);
            self.loading.set(false);
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
