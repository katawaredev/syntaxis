use std::fs;

use syntaxis_workspace::{ErrorCode, WorkspaceRegistry};
use tempfile::tempdir;

use crate::{RegistrationPolicy, WorkspaceRegistryStore};

#[test]
fn registry_persists_and_reports_missing_workspaces() {
    let data = tempdir().unwrap();
    let project = tempdir().unwrap();
    let database = data.path().join("workspaces.sqlite3");
    let store = WorkspaceRegistryStore::open(&database, RegistrationPolicy::Local).unwrap();
    let registered =
        futures_lite::future::block_on(store.register(project.path().to_str().unwrap())).unwrap();
    drop(store);

    let project_path = project.keep();
    fs::remove_dir_all(project_path).unwrap();
    let reopened = WorkspaceRegistryStore::open(&database, RegistrationPolicy::Local).unwrap();
    let records = futures_lite::future::block_on(reopened.list()).unwrap();
    assert_eq!(records[0].id, registered.id);
    assert_eq!(
        records[0].availability,
        syntaxis_workspace::WorkspaceAvailability::Missing
    );
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
fn allowlisted_registry_hides_rows_created_by_a_local_runtime() {
    let data = tempdir().unwrap();
    let allowed = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let database = data.path().join("workspaces.sqlite3");
    let local = WorkspaceRegistryStore::open(&database, RegistrationPolicy::Local).unwrap();
    let registered =
        futures_lite::future::block_on(local.register(outside.path().to_str().unwrap())).unwrap();
    drop(local);

    let remote = WorkspaceRegistryStore::open(
        &database,
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
