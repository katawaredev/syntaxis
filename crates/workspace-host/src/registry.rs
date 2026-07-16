use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use async_trait::async_trait;
use rusqlite::{params, Connection};
use syntaxis_workspace::{
    ErrorCode, WorkspaceAvailability, WorkspaceError, WorkspaceId, WorkspaceRecord,
    WorkspaceRegistry, WorkspaceResult,
};
use uuid::Uuid;

use crate::{
    error::{map_database_error, map_io_error},
    icon::detect_workspace_icon,
    policy::RegistrationPolicy,
    profile::detect_workspace_profile,
    record::{require_changed, row_to_record, slugify, unix_milliseconds},
};

pub struct WorkspaceRegistryStore {
    connection: Mutex<Connection>,
    policy: RegistrationPolicy,
}

impl WorkspaceRegistryStore {
    pub fn permits_workspace_root(&self, root: impl AsRef<Path>) -> bool {
        root.as_ref()
            .canonicalize()
            .is_ok_and(|canonical| canonical.is_dir() && self.policy.permits(&canonical))
    }

    /// Opens or creates a registry and applies pending schema migrations.
    ///
    /// # Errors
    ///
    /// Returns an error when the database or registration policy cannot be initialized.
    pub fn open(
        database_path: impl AsRef<Path>,
        policy: RegistrationPolicy,
    ) -> WorkspaceResult<Self> {
        let connection = Connection::open(database_path).map_err(map_database_error)?;
        let store = Self {
            connection: Mutex::new(connection),
            policy: policy.canonicalize()?,
        };
        store.migrate()?;
        Ok(store)
    }

    /// Creates a migrated in-memory registry for temporary runtimes and tests.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` or the registration policy cannot be initialized.
    pub fn open_in_memory(policy: RegistrationPolicy) -> WorkspaceResult<Self> {
        let connection = Connection::open_in_memory().map_err(map_database_error)?;
        let store = Self {
            connection: Mutex::new(connection),
            policy: policy.canonicalize()?,
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> WorkspaceResult<()> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceError::internal())?;
        connection
            .execute_batch(
                r#"BEGIN;
                 CREATE TABLE IF NOT EXISTS workspaces (
                    id TEXT PRIMARY KEY NOT NULL,
                    slug TEXT NOT NULL,
                    name TEXT NOT NULL,
                    root TEXT NOT NULL UNIQUE,
                    icon TEXT NOT NULL,
                    registered_at_unix_ms INTEGER NOT NULL,
                    last_opened_unix_ms INTEGER NOT NULL,
                    profile TEXT NOT NULL DEFAULT '{"technologies":[],"languages":[]}'
                 );
                 COMMIT;"#,
            )
            .map_err(map_database_error)?;
        let has_profile = {
            let mut statement = connection
                .prepare("PRAGMA table_info(workspaces)")
                .map_err(map_database_error)?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(map_database_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(map_database_error)?;
            columns.iter().any(|column| column == "profile")
        };
        if !has_profile {
            connection
                .execute(
                    "ALTER TABLE workspaces ADD COLUMN profile TEXT NOT NULL DEFAULT '{\"technologies\":[],\"languages\":[]}'",
                    [],
                )
                .map_err(map_database_error)?;
        }
        connection
            .pragma_update(None, "user_version", 2)
            .map_err(map_database_error)
    }

    fn list_records(&self) -> WorkspaceResult<Vec<WorkspaceRecord>> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceError::internal())?;
        let mut statement = connection
            .prepare(
                "SELECT id, slug, name, root, icon, registered_at_unix_ms,
                        last_opened_unix_ms, profile
                 FROM workspaces
                 ORDER BY last_opened_unix_ms DESC, name ASC",
            )
            .map_err(map_database_error)?;
        let records = statement
            .query_map([], row_to_record)
            .map_err(map_database_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_database_error)?
            .into_iter()
            .filter(|record| self.policy.permits_registered_root(Path::new(&record.root)))
            .collect();
        Ok(records)
    }

    fn get_record(&self, id: &WorkspaceId) -> WorkspaceResult<WorkspaceRecord> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceError::internal())?;
        let record = connection
            .query_row(
                "SELECT id, slug, name, root, icon, registered_at_unix_ms,
                        last_opened_unix_ms, profile FROM workspaces WHERE id = ?1",
                params![id.0],
                row_to_record,
            )
            .map_err(map_database_error)?;
        self.require_permitted_record(record)
    }

    fn register_path(&self, absolute_path: &str) -> WorkspaceResult<WorkspaceRecord> {
        let supplied = Path::new(absolute_path);
        if !supplied.is_absolute() {
            return Err(WorkspaceError::invalid_path(
                "Choose an absolute workspace directory.",
            ));
        }
        let canonical = supplied.canonicalize().map_err(map_io_error)?;
        if !canonical.is_dir() {
            return Err(WorkspaceError::invalid_path(
                "The workspace path must be a directory.",
            ));
        }
        if !self.policy.permits(&canonical) {
            return Err(WorkspaceError::new(
                ErrorCode::OutsideAllowedRoot,
                "That directory is outside the roots exposed by this runtime.",
            ));
        }

        let name = canonical
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| WorkspaceError::invalid_path("The workspace needs a valid name."))?
            .to_owned();
        let timestamp = unix_milliseconds()?;
        let record = WorkspaceRecord {
            id: WorkspaceId::new(Uuid::new_v4().to_string()),
            slug: slugify(&name),
            icon: detect_workspace_icon(&canonical),
            profile: detect_workspace_profile(&canonical),
            name,
            root: canonical.to_string_lossy().into_owned(),
            registered_at_unix_ms: timestamp,
            last_opened_unix_ms: timestamp,
            availability: WorkspaceAvailability::Available,
        };

        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceError::internal())?;
        let icon = serde_json::to_string(&record.icon).map_err(|_| WorkspaceError::internal())?;
        let profile =
            serde_json::to_string(&record.profile).map_err(|_| WorkspaceError::internal())?;
        connection
            .execute(
                "INSERT INTO workspaces (
                    id, slug, name, root, icon, registered_at_unix_ms, last_opened_unix_ms, profile
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(root) DO UPDATE SET last_opened_unix_ms = excluded.last_opened_unix_ms",
                params![
                    record.id.0,
                    record.slug,
                    record.name,
                    record.root,
                    icon,
                    record.registered_at_unix_ms,
                    record.last_opened_unix_ms,
                    profile,
                ],
            )
            .map_err(map_database_error)?;
        drop(connection);

        self.record_by_root(&record.root)
    }

    fn record_by_root(&self, root: &str) -> WorkspaceResult<WorkspaceRecord> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceError::internal())?;
        let record = connection
            .query_row(
                "SELECT id, slug, name, root, icon, registered_at_unix_ms,
                        last_opened_unix_ms, profile FROM workspaces WHERE root = ?1",
                params![root],
                row_to_record,
            )
            .map_err(map_database_error)?;
        self.require_permitted_record(record)
    }

    fn require_permitted_record(
        &self,
        record: WorkspaceRecord,
    ) -> WorkspaceResult<WorkspaceRecord> {
        if self.policy.permits_registered_root(Path::new(&record.root)) {
            Ok(record)
        } else {
            Err(WorkspaceError::new(
                ErrorCode::OutsideAllowedRoot,
                "That workspace is outside the roots exposed by this runtime.",
            ))
        }
    }

    fn touch_record(&self, id: &WorkspaceId) -> WorkspaceResult<()> {
        let timestamp = unix_milliseconds()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceError::internal())?;
        let changed = connection
            .execute(
                "UPDATE workspaces SET last_opened_unix_ms = ?1 WHERE id = ?2",
                params![timestamp, id.0],
            )
            .map_err(map_database_error)?;
        require_changed(changed)
    }

    /// Re-scans the workspace icon, technologies, and programming languages.
    ///
    /// # Errors
    ///
    /// Returns an error when the workspace is unavailable or its registry row cannot be updated.
    pub fn refresh_profile(&self, id: &WorkspaceId) -> WorkspaceResult<WorkspaceRecord> {
        let record = self.get_record(id)?;
        let root = Path::new(&record.root);
        let icon = detect_workspace_icon(root);
        let profile = detect_workspace_profile(root);
        let encoded_icon = serde_json::to_string(&icon).map_err(|_| WorkspaceError::internal())?;
        let encoded_profile =
            serde_json::to_string(&profile).map_err(|_| WorkspaceError::internal())?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceError::internal())?;
        let changed = connection
            .execute(
                "UPDATE workspaces SET icon = ?1, profile = ?2 WHERE id = ?3",
                params![encoded_icon, encoded_profile, id.0],
            )
            .map_err(map_database_error)?;
        require_changed(changed)?;
        drop(connection);
        self.get_record(id)
    }

    fn remove_record(&self, id: &WorkspaceId) -> WorkspaceResult<()> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceError::internal())?;
        let changed = connection
            .execute("DELETE FROM workspaces WHERE id = ?1", params![id.0])
            .map_err(map_database_error)?;
        require_changed(changed)
    }

    /// Permanently deletes a registered workspace directory after validating it again.
    ///
    /// # Errors
    ///
    /// Returns an error without deleting anything when confirmation or root validation fails.
    pub fn delete_project_files(
        &self,
        id: &WorkspaceId,
        explicitly_confirmed: bool,
    ) -> WorkspaceResult<()> {
        if !explicitly_confirmed {
            return Err(WorkspaceError::new(
                ErrorCode::PermissionDenied,
                "Deleting project files requires explicit confirmation.",
            ));
        }
        let record = self.get_record(id)?;
        let root = PathBuf::from(&record.root);
        let canonical = root.canonicalize().map_err(map_io_error)?;
        if canonical != root || canonical.parent().is_none() || !self.policy.permits(&canonical) {
            return Err(WorkspaceError::new(
                ErrorCode::RootOperationRejected,
                "The registered workspace root could not be validated for deletion.",
            ));
        }
        std::fs::remove_dir_all(canonical).map_err(map_io_error)
    }
}

#[async_trait(?Send)]
impl WorkspaceRegistry for WorkspaceRegistryStore {
    async fn list(&self) -> WorkspaceResult<Vec<WorkspaceRecord>> {
        self.list_records()
    }

    async fn get(&self, id: &WorkspaceId) -> WorkspaceResult<WorkspaceRecord> {
        self.get_record(id)
    }

    async fn register(&self, absolute_path: &str) -> WorkspaceResult<WorkspaceRecord> {
        self.register_path(absolute_path)
    }

    async fn touch(&self, id: &WorkspaceId) -> WorkspaceResult<()> {
        self.touch_record(id)
    }

    async fn remove(&self, id: &WorkspaceId) -> WorkspaceResult<()> {
        self.remove_record(id)
    }
}

#[cfg(test)]
mod tests;
