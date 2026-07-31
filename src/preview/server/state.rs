use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Mutex, MutexGuard, OnceLock},
};

use dioxus::prelude::ServerFnError;
use serde::{Deserialize, Serialize};
use syntaxis_workspace::WorkspaceId;
use url::Url;

use super::{internal, request_error};
use crate::preview::{PreviewConfig, PreviewTarget};

#[derive(Clone)]
pub(super) struct Lease {
    pub(super) workspace_id: WorkspaceId,
    pub(super) upstream: Url,
    pub(super) target_label: String,
    pub(super) share_token: Option<String>,
    pub(super) gateway_base: Url,
    pub(super) public_authority: String,
    pub(super) public_origin: String,
    pub(super) parent_origin: String,
    pub(super) secure: bool,
}

#[derive(Default, Deserialize, Serialize)]
struct ConfigFile {
    workspaces: HashMap<String, PreviewConfig>,
}

pub(super) struct ConfigStore {
    path: PathBuf,
    file: ConfigFile,
}

impl ConfigStore {
    fn open(path: PathBuf) -> Result<Self, String> {
        let file = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| format!("Could not read saved preview targets: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => ConfigFile::default(),
            Err(error) => return Err(format!("Could not read saved preview targets: {error}")),
        };
        Ok(Self { path, file })
    }

    pub(super) fn save(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&self.file)
            .map_err(|error| format!("Could not encode preview targets: {error}"))?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, bytes)
            .and_then(|()| fs::rename(&temporary, &self.path))
            .map_err(|error| format!("Could not save the preview target: {error}"))
    }

    pub(super) fn remove_workspace(&mut self, workspace_id: &str) -> Result<(), String> {
        let Some(previous) = self.file.workspaces.remove(workspace_id) else {
            return Ok(());
        };
        if let Err(error) = self.save() {
            self.file
                .workspaces
                .insert(workspace_id.to_owned(), previous);
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn workspace_config(&self, workspace_id: &str) -> PreviewConfig {
        self.file
            .workspaces
            .get(workspace_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(super) fn replace_workspace_config(
        &mut self,
        workspace_id: String,
        config: PreviewConfig,
    ) -> Option<PreviewConfig> {
        self.file.workspaces.insert(workspace_id, config)
    }

    pub(super) fn restore_workspace_config(
        &mut self,
        workspace_id: String,
        previous: Option<PreviewConfig>,
    ) {
        if let Some(previous) = previous {
            self.file.workspaces.insert(workspace_id, previous);
        } else {
            self.file.workspaces.remove(&workspace_id);
        }
    }
}

static LEASES: OnceLock<Mutex<HashMap<String, Lease>>> = OnceLock::new();
static CONFIGS: OnceLock<Result<Mutex<ConfigStore>, String>> = OnceLock::new();

pub(super) fn normalize_config(mut config: PreviewConfig) -> PreviewConfig {
    if config.target.is_none() {
        config.target = config
            .port
            .take()
            .map(|port| PreviewTarget::Loopback { port });
    }
    config
}

pub(super) fn configs() -> Result<MutexGuard<'static, ConfigStore>, ServerFnError> {
    CONFIGS
        .get_or_init(|| {
            ConfigStore::open(
                crate::workspace::api::server::data_directory().join("preview-targets.json"),
            )
            .map(Mutex::new)
        })
        .as_ref()
        .map_err(|error| internal(error.clone()))?
        .lock()
        .map_err(|_| internal("The preview target store is unavailable."))
}

pub(super) fn leases() -> Result<MutexGuard<'static, HashMap<String, Lease>>, ServerFnError> {
    LEASES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| internal("The preview session store is unavailable."))
}

pub(super) fn workspace_lease_mut<'a>(
    leases: &'a mut HashMap<String, Lease>,
    workspace_id: &WorkspaceId,
    lease_id: &str,
) -> Result<&'a mut Lease, ServerFnError> {
    leases
        .get_mut(lease_id)
        .filter(|lease| lease.workspace_id == *workspace_id)
        .ok_or_else(|| request_error("The preview session is no longer active.", 404))
}

pub(super) fn replace_workspace_lease(
    leases: &mut HashMap<String, Lease>,
    lease_id: String,
    lease: Lease,
) {
    leases.retain(|_, existing| existing.workspace_id != lease.workspace_id);
    leases.insert(lease_id, lease);
}

pub(super) fn invalidate_lease(lease_id: &str) {
    if let Ok(mut leases) = leases() {
        remove_lease(&mut leases, lease_id);
    }
}

fn remove_lease(leases: &mut HashMap<String, Lease>, lease_id: &str) -> bool {
    leases.remove(lease_id).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER_ID: &str = "0123456789abcdef0123456789abcdef";

    fn test_lease(upstream: &str) -> Lease {
        Lease {
            workspace_id: WorkspaceId::new("workspace"),
            upstream: Url::parse(upstream).unwrap(),
            target_label: upstream.into(),
            share_token: None,
            gateway_base: Url::parse("https://preview.example.test/").unwrap(),
            public_authority: format!("p-{OWNER_ID}.preview.example.test"),
            public_origin: format!("https://p-{OWNER_ID}.preview.example.test"),
            parent_origin: "https://syntaxis.example.test".into(),
            secure: true,
        }
    }

    #[test]
    fn removing_workspace_purges_its_saved_preview_target() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("preview-targets.json");
        let mut store = ConfigStore::open(path.clone()).unwrap();
        store.file.workspaces.insert(
            "workspace".into(),
            PreviewConfig {
                target: Some(PreviewTarget::Loopback { port: 3000 }),
                port: None,
            },
        );
        store.save().unwrap();

        store.remove_workspace("workspace").unwrap();

        let reopened = ConfigStore::open(path).unwrap();
        assert!(!reopened.file.workspaces.contains_key("workspace"));
    }

    #[test]
    fn reconnecting_replaces_only_that_workspaces_lease() {
        let mut leases = HashMap::new();
        let first = test_lease("http://127.0.0.1:3000/");
        let mut other = test_lease("http://127.0.0.1:4000/");
        other.workspace_id = WorkspaceId::new("other-workspace");
        leases.insert("first".into(), first);
        leases.insert("other".into(), other);

        replace_workspace_lease(
            &mut leases,
            "replacement".into(),
            test_lease("http://127.0.0.1:5000/"),
        );

        assert!(!leases.contains_key("first"));
        assert!(leases.contains_key("replacement"));
        assert!(leases.contains_key("other"));
    }

    #[test]
    fn upstream_failure_removes_the_whole_preview_session() {
        let mut leases = HashMap::new();
        let mut active = test_lease("http://127.0.0.1:3000/");
        active.share_token = Some("shared-access".into());
        let mut other = test_lease("http://127.0.0.1:4000/");
        other.workspace_id = WorkspaceId::new("other-workspace");
        leases.insert("active".into(), active);
        leases.insert("other".into(), other);

        assert!(remove_lease(&mut leases, "active"));
        assert!(!leases.contains_key("active"));
        assert!(leases.contains_key("other"));
        assert!(!remove_lease(&mut leases, "active"));
    }

    #[test]
    fn legacy_saved_ports_become_loopback_targets() {
        let legacy = serde_json::from_str::<PreviewConfig>(r#"{"port":5173}"#).unwrap();

        assert_eq!(
            normalize_config(legacy),
            PreviewConfig {
                target: Some(PreviewTarget::Loopback { port: 5_173 }),
                port: None,
            }
        );
    }
}
