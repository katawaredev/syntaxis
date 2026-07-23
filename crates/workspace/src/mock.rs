use std::sync::Mutex;

use async_trait::async_trait;

use crate::{
    ErrorCode, WorkspaceAvailability, WorkspaceError, WorkspaceIcon, WorkspaceIconSymbol,
    WorkspaceId, WorkspaceRecord, WorkspaceRegistry, WorkspaceResult,
};

/// In-memory registry for UI previews and operation-boundary tests.
#[derive(Default)]
pub struct MockWorkspaceRegistry {
    records: Mutex<Vec<WorkspaceRecord>>,
}

impl MockWorkspaceRegistry {
    pub fn with_records(records: Vec<WorkspaceRecord>) -> Self {
        Self {
            records: Mutex::new(records),
        }
    }

    fn lock(&self) -> WorkspaceResult<std::sync::MutexGuard<'_, Vec<WorkspaceRecord>>> {
        self.records
            .lock()
            .map_err(|_poison_error| WorkspaceError::internal())
    }
}

#[async_trait(?Send)]
impl WorkspaceRegistry for MockWorkspaceRegistry {
    async fn list(&self) -> WorkspaceResult<Vec<WorkspaceRecord>> {
        let mut records = self.lock()?.clone();
        records.sort_by_key(|record| std::cmp::Reverse(record.last_opened_unix_ms));
        Ok(records)
    }

    async fn get(&self, id: &WorkspaceId) -> WorkspaceResult<WorkspaceRecord> {
        self.lock()?
            .iter()
            .find(|record| &record.id == id)
            .cloned()
            .ok_or_else(not_found)
    }

    async fn register(&self, absolute_path: &str) -> WorkspaceResult<WorkspaceRecord> {
        let name = std::path::Path::new(absolute_path)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| WorkspaceError::invalid_path("The mock workspace needs a name."))?;
        let mut records = self.lock()?;
        if let Some(existing) = records.iter().find(|record| record.root == absolute_path) {
            return Ok(existing.clone());
        }
        let sequence = records.len() + 1;
        let record = WorkspaceRecord {
            id: WorkspaceId::new(format!("mock-{sequence}")),
            slug: name.to_lowercase().replace(' ', "-"),
            name: name.to_owned(),
            root: absolute_path.to_owned(),
            icon: WorkspaceIcon::Symbol {
                name: WorkspaceIconSymbol::Folder,
            },
            profile: crate::WorkspaceProfile::default(),
            registered_at_unix_ms: i64::try_from(sequence).unwrap_or(i64::MAX),
            last_opened_unix_ms: i64::try_from(sequence).unwrap_or(i64::MAX),
            availability: WorkspaceAvailability::Available,
        };
        records.push(record.clone());
        Ok(record)
    }

    async fn touch(&self, id: &WorkspaceId) -> WorkspaceResult<()> {
        let mut records = self.lock()?;
        let next = records
            .iter()
            .map(|record| record.last_opened_unix_ms)
            .max()
            .unwrap_or_default()
            .saturating_add(1);
        let record = records
            .iter_mut()
            .find(|record| &record.id == id)
            .ok_or_else(not_found)?;
        record.last_opened_unix_ms = next;
        Ok(())
    }

    async fn remove(&self, id: &WorkspaceId) -> WorkspaceResult<()> {
        let mut records = self.lock()?;
        let original_length = records.len();
        records.retain(|record| &record.id != id);
        if records.len() == original_length {
            Err(not_found())
        } else {
            Ok(())
        }
    }
}

fn not_found() -> WorkspaceError {
    WorkspaceError::new(ErrorCode::NotFound, "The mock workspace was not found.")
}

#[cfg(test)]
mod tests {
    use futures_lite::future::block_on;

    use crate::WorkspaceRegistry;

    use super::MockWorkspaceRegistry;

    #[test]
    fn mock_registry_exercises_the_same_contract() {
        let registry = MockWorkspaceRegistry::default();
        let registered = block_on(registry.register("/mock/Project One"))
            .expect("mock workspace should register");
        assert_eq!(registered.slug, "project-one");
        assert_eq!(
            block_on(registry.list()).expect("mock workspaces should list"),
            vec![registered.clone()]
        );
        block_on(registry.remove(&registered.id)).expect("mock workspace should be removed");
        assert!(block_on(registry.list())
            .expect("mock workspaces should list")
            .is_empty());
    }
}
