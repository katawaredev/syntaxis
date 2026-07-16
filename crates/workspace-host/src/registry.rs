use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use syntaxis_workspace::{
    ErrorCode, WorkspaceAvailability, WorkspaceError, WorkspaceIcon, WorkspaceId, WorkspaceProfile,
    WorkspaceRecord, WorkspaceRegistry, WorkspaceResult,
};
use uuid::Uuid;

use crate::{
    error::map_io_error,
    icon::detect_workspace_icon,
    policy::RegistrationPolicy,
    profile::detect_workspace_profile,
    record::{slugify, unix_milliseconds},
};

const REGISTRY_VERSION: u32 = 1;

#[derive(Clone, Deserialize, Serialize)]
struct RegistryFile {
    #[serde(default = "registry_version")]
    version: u32,
    #[serde(default)]
    workspaces: Vec<StoredWorkspace>,
}

impl Default for RegistryFile {
    fn default() -> Self {
        Self {
            version: REGISTRY_VERSION,
            workspaces: Vec::new(),
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
struct StoredWorkspace {
    id: WorkspaceId,
    slug: String,
    name: String,
    root: String,
    icon: WorkspaceIcon,
    #[serde(default)]
    profile: WorkspaceProfile,
    registered_at_unix_ms: i64,
    last_opened_unix_ms: i64,
}

impl StoredWorkspace {
    fn into_record(self) -> WorkspaceRecord {
        let availability = if Path::new(&self.root).is_dir() {
            WorkspaceAvailability::Available
        } else {
            WorkspaceAvailability::Missing
        };
        WorkspaceRecord {
            id: self.id,
            slug: self.slug,
            name: self.name,
            root: self.root,
            icon: self.icon,
            profile: self.profile,
            registered_at_unix_ms: self.registered_at_unix_ms,
            last_opened_unix_ms: self.last_opened_unix_ms,
            availability,
        }
    }
}

impl From<WorkspaceRecord> for StoredWorkspace {
    fn from(record: WorkspaceRecord) -> Self {
        Self {
            id: record.id,
            slug: record.slug,
            name: record.name,
            root: record.root,
            icon: record.icon,
            profile: record.profile,
            registered_at_unix_ms: record.registered_at_unix_ms,
            last_opened_unix_ms: record.last_opened_unix_ms,
        }
    }
}

const fn registry_version() -> u32 {
    REGISTRY_VERSION
}

pub struct WorkspaceRegistryStore {
    file: Mutex<RegistryFile>,
    path: Option<PathBuf>,
    policy: RegistrationPolicy,
}

impl WorkspaceRegistryStore {
    pub fn permits_workspace_root(&self, root: impl AsRef<Path>) -> bool {
        root.as_ref()
            .canonicalize()
            .is_ok_and(|canonical| canonical.is_dir() && self.policy.permits(&canonical))
    }

    /// Opens or creates a JSON-backed workspace registry.
    ///
    /// # Errors
    ///
    /// Returns an error when the registry file or registration policy cannot be initialized.
    pub fn open(
        registry_path: impl AsRef<Path>,
        policy: RegistrationPolicy,
    ) -> WorkspaceResult<Self> {
        let path = registry_path.as_ref().to_owned();
        let file = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<RegistryFile>(&bytes)
                .map_err(|_| WorkspaceError::internal())?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => RegistryFile::default(),
            Err(error) => return Err(map_io_error(error)),
        };
        if file.version != REGISTRY_VERSION {
            return Err(WorkspaceError::internal());
        }
        Ok(Self {
            file: Mutex::new(file),
            path: Some(path),
            policy: policy.canonicalize()?,
        })
    }

    /// Creates an in-memory registry for temporary runtimes and tests.
    ///
    /// # Errors
    ///
    /// Returns an error when the registration policy cannot be initialized.
    pub fn open_in_memory(policy: RegistrationPolicy) -> WorkspaceResult<Self> {
        Ok(Self {
            file: Mutex::new(RegistryFile::default()),
            path: None,
            policy: policy.canonicalize()?,
        })
    }

    fn lock_file(&self) -> WorkspaceResult<MutexGuard<'_, RegistryFile>> {
        self.file.lock().map_err(|_| WorkspaceError::internal())
    }

    fn save(&self, file: &RegistryFile) -> WorkspaceResult<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let bytes = serde_json::to_vec_pretty(file).map_err(|_| WorkspaceError::internal())?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, bytes)
            .and_then(|()| fs::rename(&temporary, path))
            .map_err(map_io_error)
    }

    fn update_file<T>(
        &self,
        update: impl FnOnce(&mut RegistryFile) -> WorkspaceResult<T>,
    ) -> WorkspaceResult<T> {
        let mut current = self.lock_file()?;
        let mut next = current.clone();
        let result = update(&mut next)?;
        self.save(&next)?;
        *current = next;
        Ok(result)
    }

    fn list_records(&self) -> WorkspaceResult<Vec<WorkspaceRecord>> {
        let file = self.lock_file()?;
        let mut records = file
            .workspaces
            .iter()
            .filter(|record| self.policy.permits_registered_root(Path::new(&record.root)))
            .cloned()
            .map(StoredWorkspace::into_record)
            .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            right
                .last_opened_unix_ms
                .cmp(&left.last_opened_unix_ms)
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(records)
    }

    fn get_record(&self, id: &WorkspaceId) -> WorkspaceResult<WorkspaceRecord> {
        let record = self
            .lock_file()?
            .workspaces
            .iter()
            .find(|record| record.id == *id)
            .cloned()
            .ok_or_else(workspace_not_found)?
            .into_record();
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

        let root = canonical.to_string_lossy().into_owned();
        let timestamp = unix_milliseconds()?;
        if self
            .lock_file()?
            .workspaces
            .iter()
            .any(|record| record.root == root)
        {
            self.update_file(|file| {
                let record = file
                    .workspaces
                    .iter_mut()
                    .find(|record| record.root == root)
                    .ok_or_else(workspace_not_found)?;
                record.last_opened_unix_ms = timestamp;
                Ok(())
            })?;
            return self.record_by_root(&root);
        }

        let name = canonical
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| WorkspaceError::invalid_path("The workspace needs a valid name."))?
            .to_owned();
        let record = WorkspaceRecord {
            id: WorkspaceId::new(Uuid::new_v4().to_string()),
            slug: slugify(&name),
            icon: detect_workspace_icon(&canonical),
            profile: detect_workspace_profile(&canonical),
            name,
            root,
            registered_at_unix_ms: timestamp,
            last_opened_unix_ms: timestamp,
            availability: WorkspaceAvailability::Available,
        };
        self.update_file(|file| {
            if let Some(existing) = file
                .workspaces
                .iter_mut()
                .find(|existing| existing.root == record.root)
            {
                existing.last_opened_unix_ms = timestamp;
            } else {
                file.workspaces.push(record.clone().into());
            }
            Ok(())
        })?;
        self.record_by_root(&record.root)
    }

    fn record_by_root(&self, root: &str) -> WorkspaceResult<WorkspaceRecord> {
        let record = self
            .lock_file()?
            .workspaces
            .iter()
            .find(|record| record.root == root)
            .cloned()
            .ok_or_else(workspace_not_found)?
            .into_record();
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
        self.update_file(|file| {
            let record = file
                .workspaces
                .iter_mut()
                .find(|record| record.id == *id)
                .ok_or_else(workspace_not_found)?;
            record.last_opened_unix_ms = timestamp;
            Ok(())
        })
    }

    /// Re-scans the workspace icon, technologies, and programming languages.
    ///
    /// # Errors
    ///
    /// Returns an error when the workspace is unavailable or its registry entry cannot be updated.
    pub fn refresh_profile(&self, id: &WorkspaceId) -> WorkspaceResult<WorkspaceRecord> {
        let record = self.get_record(id)?;
        let root = Path::new(&record.root);
        let icon = detect_workspace_icon(root);
        let profile = detect_workspace_profile(root);
        self.update_file(|file| {
            let record = file
                .workspaces
                .iter_mut()
                .find(|record| record.id == *id)
                .ok_or_else(workspace_not_found)?;
            record.icon = icon;
            record.profile = profile;
            Ok(())
        })?;
        self.get_record(id)
    }

    fn remove_record(&self, id: &WorkspaceId) -> WorkspaceResult<()> {
        self.update_file(|file| {
            let index = file
                .workspaces
                .iter()
                .position(|record| record.id == *id)
                .ok_or_else(workspace_not_found)?;
            file.workspaces.remove(index);
            Ok(())
        })
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
        fs::remove_dir_all(canonical).map_err(map_io_error)
    }
}

fn workspace_not_found() -> WorkspaceError {
    WorkspaceError::new(ErrorCode::NotFound, "The workspace was not found.")
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
