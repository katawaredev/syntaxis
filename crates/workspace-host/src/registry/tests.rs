use std::fs;

use syntaxis_workspace::{ErrorCode, WorkspaceRegistry};
use tempfile::tempdir;

use crate::{RegistrationPolicy, WorkspaceRegistryStore};

#[test]
fn registry_persists_and_reports_missing_workspaces() {
    let data = tempdir().unwrap();
    let project = tempdir().unwrap();
    let registry = data.path().join("workspaces.json");
    let store = WorkspaceRegistryStore::open(&registry, RegistrationPolicy::Unrestricted).unwrap();
    let registered =
        futures_lite::future::block_on(store.register(project.path().to_str().unwrap())).unwrap();
    drop(store);

    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&registry).unwrap()).unwrap();
    assert_eq!(saved["version"], 1);
    assert_eq!(saved["workspaces"][0]["id"], registered.id.0);
    assert!(saved["workspaces"][0].get("availability").is_none());

    let project_path = project.keep();
    fs::remove_dir_all(project_path).unwrap();
    let reopened =
        WorkspaceRegistryStore::open(&registry, RegistrationPolicy::Unrestricted).unwrap();
    let records = futures_lite::future::block_on(reopened.list()).unwrap();
    assert_eq!(records[0].id, registered.id);
    assert_eq!(
        records[0].availability,
        syntaxis_workspace::WorkspaceAvailability::Missing
    );
}

#[test]
fn refreshing_a_workspace_recomputes_and_persists_its_profile() {
    let data = tempdir().unwrap();
    let project = tempdir().unwrap();
    let registry = data.path().join("workspaces.json");
    let store = WorkspaceRegistryStore::open(&registry, RegistrationPolicy::Unrestricted).unwrap();
    let registered =
        futures_lite::future::block_on(store.register(project.path().to_str().unwrap())).unwrap();
    assert!(registered.profile.languages.is_empty());

    fs::write(project.path().join("main.rs"), "fn main() {}\n").unwrap();
    let refreshed = store.refresh_profile(&registered.id).unwrap();
    assert!(refreshed
        .profile
        .languages
        .iter()
        .any(|language| language.name == "Rust"));
    drop(store);

    let reopened =
        WorkspaceRegistryStore::open(registry, RegistrationPolicy::Unrestricted).unwrap();
    let persisted = futures_lite::future::block_on(reopened.get(&registered.id)).unwrap();
    assert_eq!(persisted.profile, refreshed.profile);
}

#[cfg(unix)]
#[test]
fn allowlist_rejects_parent_and_symlink_escapes() {
    use std::os::unix::fs::symlink;

    let allowed = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let link = allowed.path().join("escape");
    symlink(outside.path(), &link).unwrap();
    let store = WorkspaceRegistryStore::open_in_memory(RegistrationPolicy::Allowlisted {
        roots: vec![allowed.path().to_owned()],
    })
    .unwrap();

    let error = futures_lite::future::block_on(store.register(link.to_str().unwrap())).unwrap_err();
    assert_eq!(error.code, ErrorCode::OutsideAllowedRoot);
    let error = futures_lite::future::block_on(store.register(outside.path().to_str().unwrap()))
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::OutsideAllowedRoot);
}

#[test]
fn allowlisted_registry_hides_rows_created_by_an_unrestricted_runtime() {
    let data = tempdir().unwrap();
    let allowed = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let registry = data.path().join("workspaces.json");
    let unrestricted =
        WorkspaceRegistryStore::open(&registry, RegistrationPolicy::Unrestricted).unwrap();
    let registered =
        futures_lite::future::block_on(unrestricted.register(outside.path().to_str().unwrap()))
            .unwrap();
    drop(unrestricted);

    let remote = WorkspaceRegistryStore::open(
        &registry,
        RegistrationPolicy::Allowlisted {
            roots: vec![allowed.path().to_owned()],
        },
    )
    .unwrap();

    assert!(futures_lite::future::block_on(remote.list())
        .unwrap()
        .is_empty());
    let error = futures_lite::future::block_on(remote.get(&registered.id)).unwrap_err();
    assert_eq!(error.code, ErrorCode::OutsideAllowedRoot);
}
