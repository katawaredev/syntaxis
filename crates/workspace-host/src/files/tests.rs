use std::{fs, thread, time::Duration};

use futures_lite::future::block_on;
use syntaxis_workspace::{
    ErrorCode, RelativePath, WorkspaceAvailability, WorkspaceFiles, WorkspaceIcon,
    WorkspaceIconSymbol, WorkspaceId, WorkspaceRecord,
};
use tempfile::tempdir;

use super::HostWorkspaceFiles;

fn record(root: &std::path::Path) -> WorkspaceRecord {
    WorkspaceRecord {
        id: WorkspaceId::new("test"),
        slug: "test".into(),
        name: "Test".into(),
        root: root.canonicalize().unwrap().to_string_lossy().into_owned(),
        icon: WorkspaceIcon::Symbol {
            name: WorkspaceIconSymbol::Folder,
        },
        profile: syntaxis_workspace::WorkspaceProfile::default(),
        registered_at_unix_ms: 0,
        last_opened_unix_ms: 0,
        availability: WorkspaceAvailability::Available,
    }
}

#[test]
fn atomic_write_detects_external_version_conflicts() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("notes.txt");
    fs::write(&file, "first").unwrap();
    let workspace = record(directory.path());
    let path = RelativePath::try_from("notes.txt").unwrap();
    let service = HostWorkspaceFiles;
    let original = block_on(service.read_text(&workspace, &path, 1024)).unwrap();

    thread::sleep(Duration::from_millis(2));
    fs::write(&file, "external change").unwrap();
    let error = block_on(service.write_text(
        &workspace,
        &path,
        "our change",
        Some(&original.version),
        1024,
    ))
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::Conflict);
    assert_eq!(fs::read_to_string(file).unwrap(), "external change");
}

#[test]
fn atomic_write_replaces_content_and_advances_the_version() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("notes.txt");
    fs::write(&file, "before").unwrap();
    let workspace = record(directory.path());
    let path = RelativePath::try_from("notes.txt").unwrap();
    let files = HostWorkspaceFiles;
    let before = block_on(files.read_text(&workspace, &path, 1024)).unwrap();

    let after = block_on(files.write_text(
        &workspace,
        &path,
        "after replacement",
        Some(&before.version),
        1024,
    ))
    .unwrap();

    assert_ne!(after, before.version);
    assert_eq!(fs::read_to_string(&file).unwrap(), "after replacement");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn rejected_write_preserves_the_existing_file() {
    let directory = tempdir().unwrap();
    let file = directory.path().join("notes.txt");
    fs::write(&file, "preserve me").unwrap();
    let workspace = record(directory.path());
    let path = RelativePath::try_from("notes.txt").unwrap();

    let error = block_on(HostWorkspaceFiles.write_text(&workspace, &path, "too large", None, 2))
        .unwrap_err();

    assert_eq!(error.code, ErrorCode::TooLarge);
    assert_eq!(fs::read_to_string(file).unwrap(), "preserve me");
}

#[test]
fn destructive_operations_reject_the_workspace_root() {
    let directory = tempdir().unwrap();
    let workspace = record(directory.path());
    let error = block_on(HostWorkspaceFiles.delete(&workspace, &RelativePath::root())).unwrap_err();
    assert_eq!(error.code, ErrorCode::RootOperationRejected);
}

#[cfg(unix)]
#[test]
fn file_operations_reject_symlink_escapes() {
    use std::os::unix::fs::symlink;

    let directory = tempdir().unwrap();
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    symlink(outside.path(), directory.path().join("escape")).unwrap();
    let workspace = record(directory.path());
    let path = RelativePath::try_from("escape/secret.txt").unwrap();
    let error = block_on(HostWorkspaceFiles.read_text(&workspace, &path, 1024)).unwrap_err();
    assert_eq!(error.code, ErrorCode::OutsideAllowedRoot);
}
