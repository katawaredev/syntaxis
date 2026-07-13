use std::{fs, path::Path, time::Duration};

use syntaxis_workspace::WorkspaceId;
use tempfile::tempdir;

use super::{is_ignored_path, WorkspaceWatcher};

#[test]
fn generated_and_vcs_trees_are_ignored_by_component() {
    assert!(is_ignored_path(Path::new("project/.git/objects/pack")));
    assert!(is_ignored_path(Path::new(
        "project/node_modules/package/index.js"
    )));
    assert!(is_ignored_path(Path::new("project/target/debug/app")));
    assert!(!is_ignored_path(Path::new("project/src/target.rs")));
    assert!(!is_ignored_path(Path::new("project/git/config.rs")));
}

#[test]
fn watcher_batches_workspace_relative_changes() {
    let directory = tempdir().unwrap();
    let mut watcher = WorkspaceWatcher::start(
        WorkspaceId::new("watched"),
        directory.path(),
        Duration::from_millis(30),
    )
    .unwrap();
    fs::write(directory.path().join("new-file.txt"), "content").unwrap();

    let batch = watcher.receive_batch(Duration::from_secs(3)).unwrap();
    assert!(batch
        .changes
        .iter()
        .any(|change| change.path.as_str() == "new-file.txt"));
}

#[test]
fn watcher_does_not_descend_into_ignored_trees() {
    let directory = tempdir().unwrap();
    let ignored = directory.path().join("node_modules");
    fs::create_dir(&ignored).unwrap();
    let mut watcher = WorkspaceWatcher::start(
        WorkspaceId::new("watched"),
        directory.path(),
        Duration::from_millis(20),
    )
    .unwrap();

    fs::write(ignored.join("package.js"), "ignored").unwrap();
    let batch = watcher.receive_batch(Duration::from_millis(150)).unwrap();
    assert!(batch.changes.is_empty());
}
