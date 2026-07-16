use std::{fs, path::Path, process::Command};

use syntaxis_git::{ChangeKind, ClonePhase, GitErrorCode, GitOperations};
use syntaxis_workspace::{WorkspaceAvailability, WorkspaceIcon, WorkspaceId, WorkspaceRecord};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use super::{parse_clone_progress, HostGit, HostGitConfig};

#[test]
fn parses_carriage_return_git_clone_progress_without_forwarding_errors() {
    let receiving = parse_clone_progress("Receiving objects: 42% (42/100)").unwrap();
    assert_eq!(receiving.phase, ClonePhase::Receiving);
    assert_eq!(receiving.percent, Some(42));
    assert_eq!(
        parse_clone_progress("remote: Counting objects: 100% (10/10)")
            .unwrap()
            .phase,
        ClonePhase::Counting
    );
    assert!(parse_clone_progress("fatal: authentication failed for secret").is_none());
}

#[tokio::test]
async fn reads_real_repository_staged_unstaged_untracked_and_renamed_changes() {
    let repository = init_repository();
    fs::write(repository.path().join("tracked.txt"), "initial\n").unwrap();
    git(repository.path(), &["add", "tracked.txt"]);
    git(repository.path(), &["commit", "-m", "initial"]);

    fs::write(repository.path().join("tracked.txt"), "changed\n").unwrap();
    fs::write(repository.path().join("staged.txt"), "staged\n").unwrap();
    fs::write(repository.path().join("untracked.txt"), "untracked\n").unwrap();
    git(repository.path(), &["add", "staged.txt"]);
    git(repository.path(), &["mv", "tracked.txt", "renamed.txt"]);

    let status = HostGit::default()
        .status(&workspace(repository.path()))
        .await
        .unwrap();

    assert_eq!(status.branch.head.as_deref(), Some("main"));
    assert!(status.changes.iter().any(|change| {
        change.path.as_str() == "renamed.txt"
            && change
                .original_path
                .as_ref()
                .map(syntaxis_workspace::RelativePath::as_str)
                == Some("tracked.txt")
            && change.index == Some(ChangeKind::Renamed)
    }));
    assert!(status.changes.iter().any(|change| {
        change.path.as_str() == "staged.txt" && change.index == Some(ChangeKind::Added)
    }));
    assert!(status.changes.iter().any(|change| {
        change.path.as_str() == "untracked.txt" && change.worktree == Some(ChangeKind::Untracked)
    }));
}

#[tokio::test]
async fn reports_non_repository_without_exposing_command_output() {
    let directory = TempDir::new().unwrap();
    let error = HostGit::default()
        .status(&workspace(directory.path()))
        .await
        .unwrap_err();

    assert_eq!(error.code, GitErrorCode::NotRepository);
    assert_eq!(error.message, "This workspace is not a Git repository.");
}

#[tokio::test]
async fn honors_cancellation_before_spawning_useful_work() {
    let repository = init_repository();
    let cancellation = CancellationToken::new();
    cancellation.cancel();

    let error = HostGit::default()
        .status_with_cancellation(&workspace(repository.path()), cancellation)
        .await
        .unwrap_err();

    assert_eq!(error.code, GitErrorCode::Cancelled);
}

#[tokio::test]
async fn rejects_output_over_the_configured_cap() {
    let repository = init_repository();
    let git = HostGit::new(HostGitConfig {
        max_output_bytes: 1,
        ..HostGitConfig::default()
    });

    let error = git.status(&workspace(repository.path())).await.unwrap_err();

    assert_eq!(error.code, GitErrorCode::OutputTooLarge);
}

#[tokio::test]
async fn parses_conflicts_created_by_a_real_merge() {
    let repository = init_repository();
    fs::write(repository.path().join("conflict.txt"), "base\n").unwrap();
    git(repository.path(), &["add", "conflict.txt"]);
    git(repository.path(), &["commit", "-m", "base"]);
    git(repository.path(), &["checkout", "-b", "incoming"]);
    fs::write(repository.path().join("conflict.txt"), "incoming\n").unwrap();
    git(repository.path(), &["commit", "-am", "incoming"]);
    git(repository.path(), &["checkout", "main"]);
    fs::write(repository.path().join("conflict.txt"), "current\n").unwrap();
    git(repository.path(), &["commit", "-am", "current"]);
    let merge = Command::new("git")
        .args(["merge", "incoming"])
        .current_dir(repository.path())
        .output()
        .unwrap();
    assert!(!merge.status.success());

    let status = HostGit::default()
        .status(&workspace(repository.path()))
        .await
        .unwrap();

    assert_eq!(status.conflict_count(), 1);
    assert!(status.changes.iter().any(|change| {
        change.path.as_str() == "conflict.txt"
            && change.index == Some(ChangeKind::Unmerged)
            && change.worktree == Some(ChangeKind::Unmerged)
            && change.conflicted
    }));
}

fn init_repository() -> TempDir {
    let directory = TempDir::new().unwrap();
    git(directory.path(), &["init", "-b", "main"]);
    git(directory.path(), &["config", "user.name", "Syntaxis Test"]);
    git(
        directory.path(),
        &["config", "user.email", "syntaxis@example.invalid"],
    );
    git(directory.path(), &["config", "commit.gpgsign", "false"]);
    directory
}

fn git(root: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn workspace(root: &Path) -> WorkspaceRecord {
    WorkspaceRecord {
        id: WorkspaceId::new("test"),
        slug: "test".into(),
        name: "Test".into(),
        root: root.to_string_lossy().into_owned(),
        icon: WorkspaceIcon::default(),
        profile: syntaxis_workspace::WorkspaceProfile::default(),
        registered_at_unix_ms: 0,
        last_opened_unix_ms: 0,
        availability: WorkspaceAvailability::Available,
    }
}
