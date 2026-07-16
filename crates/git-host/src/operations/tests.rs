use std::{fmt::Write as _, fs, path::Path, process::Command};

use syntaxis_git::{
    parse_diff_hunks, BranchRequest, ChangeKind, ClonePhase, CloneRequest, CommitOutcome,
    CommitRequest, ConflictChoice, ConflictRequest, DiffKind, GitErrorCode, GitOperations,
    HunkAction, HunkRequest, MergeOutcome, PushOutcome, TagRequest,
};
use syntaxis_workspace::{
    RelativePath, WorkspaceAvailability, WorkspaceIcon, WorkspaceId, WorkspaceRecord,
};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use crate::HostGit;

#[tokio::test]
async fn initializes_a_workspace_once_with_a_main_branch() {
    let directory = TempDir::new().unwrap();
    let host = HostGit::default();
    let workspace = workspace(directory.path());

    host.initialize(&workspace).await.unwrap();

    assert!(directory.path().join(".git").is_dir());
    assert_eq!(
        host.status(&workspace)
            .await
            .unwrap()
            .branch
            .head
            .as_deref(),
        Some("main")
    );
    assert_eq!(
        host.initialize(&workspace).await.unwrap_err().code,
        GitErrorCode::Conflict
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn hunk_actions_match_git_index_and_worktree_semantics() {
    let repository = init_repository();
    let original = (1..=30).fold(String::new(), |mut output, line| {
        writeln!(&mut output, "line {line}").unwrap();
        output
    });
    fs::write(repository.path().join("file.txt"), &original).unwrap();
    git(repository.path(), &["add", "file.txt"]);
    git(repository.path(), &["commit", "-m", "base"]);
    let changed = original
        .replace("line 2\n", "line two changed\n")
        .replace("line 25\n", "line twenty-five changed\n");
    fs::write(repository.path().join("file.txt"), changed).unwrap();

    let host = HostGit::default();
    let workspace = workspace(repository.path());
    let path = RelativePath::try_from("file.txt").unwrap();
    let worktree = host
        .diff(&workspace, &path, DiffKind::Worktree)
        .await
        .unwrap();
    let first_worktree_hunk = parse_diff_hunks(&worktree.patch).unwrap()[0].fingerprint;
    host.apply_hunk(
        &workspace,
        HunkRequest {
            path: path.clone(),
            kind: DiffKind::Worktree,
            hunk_index: 0,
            expected_fingerprint: first_worktree_hunk,
            action: HunkAction::Stage,
        },
    )
    .await
    .unwrap();
    let staged = host
        .diff(&workspace, &path, DiffKind::Staged)
        .await
        .unwrap();
    assert!(staged.patch.contains("line two changed"));
    assert!(!staged.patch.contains("line twenty-five changed"));
    let unstaged = host
        .diff(&workspace, &path, DiffKind::Worktree)
        .await
        .unwrap();
    assert!(!unstaged.patch.contains("line two changed"));
    assert!(unstaged.patch.contains("line twenty-five changed"));

    let staged_hunk = parse_diff_hunks(&staged.patch).unwrap()[0].fingerprint;
    host.apply_hunk(
        &workspace,
        HunkRequest {
            path: path.clone(),
            kind: DiffKind::Staged,
            hunk_index: 0,
            expected_fingerprint: staged_hunk,
            action: HunkAction::Unstage,
        },
    )
    .await
    .unwrap();
    assert!(host
        .diff(&workspace, &path, DiffKind::Staged)
        .await
        .unwrap()
        .patch
        .is_empty());

    let worktree = host
        .diff(&workspace, &path, DiffKind::Worktree)
        .await
        .unwrap();
    let first_worktree_hunk = parse_diff_hunks(&worktree.patch).unwrap()[0].fingerprint;
    let error = host
        .apply_hunk(
            &workspace,
            HunkRequest {
                path: path.clone(),
                kind: DiffKind::Worktree,
                hunk_index: 0,
                expected_fingerprint: first_worktree_hunk ^ 1,
                action: HunkAction::Discard,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, GitErrorCode::Conflict);

    host.apply_hunk(
        &workspace,
        HunkRequest {
            path: path.clone(),
            kind: DiffKind::Worktree,
            hunk_index: 0,
            expected_fingerprint: first_worktree_hunk,
            action: HunkAction::Discard,
        },
    )
    .await
    .unwrap();
    let contents = fs::read_to_string(repository.path().join("file.txt")).unwrap();
    assert!(contents.contains("line 2\n"));
    assert!(contents.contains("line twenty-five changed\n"));

    let error = host
        .apply_hunk(
            &workspace,
            HunkRequest {
                path,
                kind: DiffKind::Worktree,
                hunk_index: 99,
                expected_fingerprint: 0,
                action: HunkAction::Stage,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, GitErrorCode::Conflict);
}

#[tokio::test]
async fn branch_and_history_operations_use_real_repository_state() {
    let repository = init_repository();
    fs::write(repository.path().join("tracked.txt"), "base\n").unwrap();
    git(repository.path(), &["add", "tracked.txt"]);
    git(repository.path(), &["commit", "-m", "base commit"]);
    let host = HostGit::default();
    let workspace = workspace(repository.path());

    host.create_branch(
        &workspace,
        BranchRequest {
            name: "feature/live-git".into(),
            start_point: None,
        },
    )
    .await
    .unwrap();
    assert!(host
        .branches(&workspace)
        .await
        .unwrap()
        .iter()
        .any(|branch| branch.name == "feature/live-git" && branch.current));
    host.rename_branch(&workspace, "feature/renamed")
        .await
        .unwrap();
    host.switch_branch(&workspace, "main").await.unwrap();
    host.delete_branch(&workspace, "feature/renamed", false)
        .await
        .unwrap();

    fs::write(repository.path().join("tracked.txt"), "base\nsecond\n").unwrap();
    git(repository.path(), &["commit", "-am", "second commit"]);
    let history = host.history(&workspace, 20).await.unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].subject, "second commit");
    let detail = host
        .commit_detail(&workspace, &history[0].oid)
        .await
        .unwrap();
    assert_eq!(detail.commit.subject, "second commit");
    assert_eq!(detail.files_changed, 1);
    assert_eq!(detail.additions, 1);
    assert!(detail.patch.contains("+second"));

    let second_oid = history[0].oid.clone();
    let base_oid = history[1].oid.clone();
    host.checkout_commit(&workspace, &base_oid).await.unwrap();
    assert_eq!(host.status(&workspace).await.unwrap().branch.head, None);
    assert_eq!(
        fs::read_to_string(repository.path().join("tracked.txt")).unwrap(),
        "base\n"
    );

    host.switch_branch(&workspace, "main").await.unwrap();
    host.revert_commit(&workspace, &second_oid).await.unwrap();
    assert_eq!(
        fs::read_to_string(repository.path().join("tracked.txt")).unwrap(),
        "base\n"
    );
    assert_eq!(
        host.history(&workspace, 1).await.unwrap()[0].parents.len(),
        1
    );
}

#[tokio::test]
async fn tag_operations_preserve_lightweight_and_annotated_targets() {
    let repository = init_repository();
    fs::write(repository.path().join("tracked.txt"), "base\n").unwrap();
    git(repository.path(), &["add", "tracked.txt"]);
    git(repository.path(), &["commit", "-m", "base"]);
    let host = HostGit::default();
    let workspace = workspace(repository.path());
    let base_oid = host.history(&workspace, 1).await.unwrap()[0].oid.clone();

    host.create_tag(
        &workspace,
        TagRequest {
            name: "v1.0.0".into(),
            target: None,
            message: None,
        },
    )
    .await
    .unwrap();
    host.create_tag(
        &workspace,
        TagRequest {
            name: "release/annotated".into(),
            target: Some("HEAD".into()),
            message: Some("Release notes".into()),
        },
    )
    .await
    .unwrap();

    let tags = host.tags(&workspace).await.unwrap();
    assert!(tags
        .iter()
        .any(|tag| { tag.name == "v1.0.0" && tag.target_oid == base_oid && !tag.annotated }));
    assert!(tags.iter().any(|tag| {
        tag.name == "release/annotated" && tag.target_oid == base_oid && tag.annotated
    }));

    host.delete_tag(&workspace, "v1.0.0").await.unwrap();
    assert!(!host
        .tags(&workspace)
        .await
        .unwrap()
        .iter()
        .any(|tag| tag.name == "v1.0.0"));
    let error = host
        .create_tag(
            &workspace,
            TagRequest {
                name: "-invalid".into(),
                target: None,
                message: None,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, GitErrorCode::Conflict);
}

#[tokio::test]
async fn comparison_merge_conflict_abort_and_clean_merge_use_real_state() {
    let repository = init_repository();
    fs::write(repository.path().join("shared.txt"), "base\n").unwrap();
    git(repository.path(), &["add", "shared.txt"]);
    git(repository.path(), &["commit", "-m", "base"]);
    git(repository.path(), &["switch", "-c", "feature"]);
    fs::write(repository.path().join("shared.txt"), "feature\n").unwrap();
    git(repository.path(), &["commit", "-am", "feature change"]);
    git(repository.path(), &["switch", "main"]);
    fs::write(repository.path().join("shared.txt"), "main\n").unwrap();
    git(repository.path(), &["commit", "-am", "main change"]);

    let host = HostGit::default();
    let workspace = workspace(repository.path());
    let comparison = host.compare(&workspace, "main", "feature").await.unwrap();
    assert_eq!(comparison.base_only_commits, 1);
    assert_eq!(comparison.head_only_commits, 1);
    assert_eq!(comparison.commits.len(), 1);
    assert!(comparison.patch.contains("+feature"));

    let outcome = host.merge(&workspace, "feature").await.unwrap();
    assert!(matches!(
        outcome,
        MergeOutcome::Conflicts { ref paths }
            if paths.iter().any(|path| path.as_str() == "shared.txt")
    ));
    assert_eq!(host.status(&workspace).await.unwrap().conflict_count(), 1);
    let path = RelativePath::try_from("shared.txt").unwrap();
    let conflict = host.conflict_file(&workspace, &path).await.unwrap();
    assert_eq!(conflict.blocks.len(), 1);
    assert!(conflict.blocks[0].current.contains("main"));
    assert!(conflict.blocks[0].incoming.contains("feature"));
    assert!(host
        .resolve_conflict(
            &workspace,
            ConflictRequest {
                path: path.clone(),
                block_index: 0,
                expected_fingerprint: conflict.blocks[0].fingerprint,
                choice: ConflictChoice::Both,
            },
        )
        .await
        .unwrap());
    let resolved_status = host.status(&workspace).await.unwrap();
    assert_eq!(resolved_status.conflict_count(), 0);
    assert!(resolved_status
        .changes
        .iter()
        .any(|change| { change.path == path && change.index == Some(ChangeKind::Modified) }));
    host.abort_merge(&workspace).await.unwrap();
    assert_eq!(host.status(&workspace).await.unwrap().conflict_count(), 0);
    assert_eq!(
        fs::read_to_string(repository.path().join("shared.txt")).unwrap(),
        "main\n"
    );

    git(repository.path(), &["switch", "-c", "clean"]);
    fs::write(repository.path().join("clean.txt"), "clean\n").unwrap();
    git(repository.path(), &["add", "clean.txt"]);
    git(repository.path(), &["commit", "-m", "clean change"]);
    git(repository.path(), &["switch", "main"]);
    assert!(matches!(
        host.merge(&workspace, "clean").await.unwrap(),
        MergeOutcome::Merged { .. }
    ));
    assert_eq!(
        fs::read_to_string(repository.path().join("clean.txt")).unwrap(),
        "clean\n"
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn current_incoming_and_both_resolve_real_merge_blocks() {
    let repository = init_repository();
    let base = (1..=90).fold(String::new(), |mut output, line| {
        writeln!(&mut output, "line {line}").unwrap();
        output
    });
    fs::write(repository.path().join("blocks.txt"), &base).unwrap();
    git(repository.path(), &["add", "blocks.txt"]);
    git(repository.path(), &["commit", "-m", "base"]);
    git(repository.path(), &["switch", "-c", "feature"]);
    let feature = base
        .replace("line 2\n", "feature 2\n")
        .replace("line 40\n", "feature 40\n")
        .replace("line 80\n", "feature 80\n");
    fs::write(repository.path().join("blocks.txt"), feature).unwrap();
    git(repository.path(), &["commit", "-am", "feature blocks"]);
    git(repository.path(), &["switch", "main"]);
    let main = base
        .replace("line 2\n", "main 2\n")
        .replace("line 40\n", "main 40\n")
        .replace("line 80\n", "main 80\n");
    fs::write(repository.path().join("blocks.txt"), main).unwrap();
    git(repository.path(), &["commit", "-am", "main blocks"]);

    let host = HostGit::default();
    let workspace = workspace(repository.path());
    let path = RelativePath::try_from("blocks.txt").unwrap();
    assert!(matches!(
        host.merge(&workspace, "feature").await.unwrap(),
        MergeOutcome::Conflicts { .. }
    ));
    let first = host.conflict_file(&workspace, &path).await.unwrap();
    assert_eq!(first.blocks.len(), 3);
    assert!(!host
        .resolve_conflict(
            &workspace,
            ConflictRequest {
                path: path.clone(),
                block_index: 0,
                expected_fingerprint: first.blocks[0].fingerprint,
                choice: ConflictChoice::Current,
            },
        )
        .await
        .unwrap());
    let second = host.conflict_file(&workspace, &path).await.unwrap();
    assert!(!host
        .resolve_conflict(
            &workspace,
            ConflictRequest {
                path: path.clone(),
                block_index: 0,
                expected_fingerprint: second.blocks[0].fingerprint,
                choice: ConflictChoice::Incoming,
            },
        )
        .await
        .unwrap());
    let third = host.conflict_file(&workspace, &path).await.unwrap();
    assert!(host
        .resolve_conflict(
            &workspace,
            ConflictRequest {
                path: path.clone(),
                block_index: 0,
                expected_fingerprint: third.blocks[0].fingerprint,
                choice: ConflictChoice::Both,
            },
        )
        .await
        .unwrap());
    let contents = fs::read_to_string(repository.path().join("blocks.txt")).unwrap();
    assert!(contents.contains("main 2\n"));
    assert!(!contents.contains("feature 2\n"));
    assert!(contents.contains("feature 40\n"));
    assert!(!contents.contains("main 40\n"));
    assert!(contents.contains("main 80\nfeature 80\n"));
    assert_eq!(host.status(&workspace).await.unwrap().conflict_count(), 0);
}

#[tokio::test]
async fn fetch_push_and_force_with_lease_follow_real_remote_state() {
    let repository = init_repository();
    fs::write(repository.path().join("tracked.txt"), "base\n").unwrap();
    git(repository.path(), &["add", "tracked.txt"]);
    git(repository.path(), &["commit", "-m", "base"]);
    let remote_parent = TempDir::new().unwrap();
    git(remote_parent.path(), &["init", "--bare", "remote.git"]);
    let remote = remote_parent.path().join("remote.git");
    git(
        repository.path(),
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(repository.path(), &["push", "-u", "origin", "main"]);
    git(&remote, &["symbolic-ref", "HEAD", "refs/heads/main"]);

    let other_parent = TempDir::new().unwrap();
    git(
        other_parent.path(),
        &["clone", remote.to_str().unwrap(), "other"],
    );
    let other = other_parent.path().join("other");
    git(&other, &["config", "user.name", "Other Test"]);
    git(&other, &["config", "user.email", "other@example.invalid"]);
    git(&other, &["config", "commit.gpgsign", "false"]);
    fs::write(other.join("remote.txt"), "remote\n").unwrap();
    git(&other, &["add", "remote.txt"]);
    git(&other, &["commit", "-m", "remote change"]);
    git(&other, &["push"]);

    let host = HostGit::default();
    let workspace = workspace(repository.path());
    host.fetch(&workspace).await.unwrap();
    fs::write(repository.path().join("local.txt"), "local\n").unwrap();
    git(repository.path(), &["add", "local.txt"]);
    git(repository.path(), &["commit", "-m", "local change"]);
    assert!(matches!(
        host.push(&workspace, false).await.unwrap(),
        PushOutcome::ForceWithLeaseRequired { .. }
    ));
    assert!(matches!(
        host.push(&workspace, true).await.unwrap(),
        PushOutcome::Pushed { .. }
    ));
}

#[tokio::test]
async fn clones_from_a_real_git_transport_into_a_new_destination() {
    let server_root = TempDir::new().unwrap();
    let source = server_root.path().join("source");
    fs::create_dir(&source).unwrap();
    git(&source, &["init", "-b", "main"]);
    git(&source, &["config", "user.name", "Syntaxis Test"]);
    git(
        &source,
        &["config", "user.email", "syntaxis@example.invalid"],
    );
    git(&source, &["config", "commit.gpgsign", "false"]);
    fs::write(source.join("README.md"), "clone fixture\n").unwrap();
    git(&source, &["add", "README.md"]);
    git(&source, &["commit", "-m", "fixture"]);
    git(
        server_root.path(),
        &["clone", "--bare", source.to_str().unwrap(), "remote.git"],
    );

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut daemon = Command::new("git")
        .args([
            "daemon".to_owned(),
            "--reuseaddr".to_owned(),
            "--export-all".to_owned(),
            "--listen=127.0.0.1".to_owned(),
            format!("--port={port}"),
            format!("--base-path={}", server_root.path().display()),
            server_root.path().to_string_lossy().into_owned(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    for _ in 0..20 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let projects = TempDir::new().unwrap();
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel(64);
    let result = HostGit::default()
        .clone_repository_with_progress(
            CloneRequest {
                url: format!("git://127.0.0.1:{port}/remote.git"),
                destination_parent: projects.path().to_string_lossy().into_owned(),
                directory_name: None,
            },
            CancellationToken::new(),
            progress_tx,
        )
        .await
        .unwrap();
    let _ = daemon.kill();
    let _ = daemon.wait();

    assert_eq!(
        fs::read_to_string(Path::new(&result.absolute_path).join("README.md")).unwrap(),
        "clone fixture\n"
    );
    let mut phases = Vec::new();
    while let Ok(update) = progress_rx.try_recv() {
        phases.push(update.phase);
    }
    assert!(phases.contains(&ClonePhase::Preparing));
    assert!(phases.contains(&ClonePhase::Finalizing));
}

#[tokio::test]
async fn cancelled_clone_removes_its_partial_destination() {
    let projects = TempDir::new().unwrap();
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let (progress, _progress_rx) = tokio::sync::mpsc::channel(4);
    let error = HostGit::default()
        .clone_repository_with_progress(
            CloneRequest {
                url: "git://127.0.0.1:9/repository.git".into(),
                destination_parent: projects.path().to_string_lossy().into_owned(),
                directory_name: Some("cancelled".into()),
            },
            cancellation,
            progress,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, GitErrorCode::Cancelled);
    assert!(!projects.path().join("cancelled").exists());
}

#[tokio::test]
async fn diff_stage_unstage_discard_and_commit_match_real_git_state() {
    let repository = init_repository();
    fs::write(repository.path().join("tracked.txt"), "base\n").unwrap();
    git(repository.path(), &["add", "tracked.txt"]);
    git(repository.path(), &["commit", "-m", "base"]);
    fs::write(repository.path().join("tracked.txt"), "changed\n").unwrap();
    fs::write(repository.path().join("new.txt"), "new\n").unwrap();

    let host = HostGit::default();
    let workspace = workspace(repository.path());
    let tracked = RelativePath::try_from("tracked.txt").unwrap();
    let new = RelativePath::try_from("new.txt").unwrap();
    let diff = host
        .diff(&workspace, &tracked, DiffKind::Worktree)
        .await
        .unwrap();
    assert!(diff.patch.contains("-base"));
    assert!(diff.patch.contains("+changed"));
    assert_eq!(diff.original.as_deref(), Some("base\n"));
    assert_eq!(diff.current.as_deref(), Some("changed\n"));
    let untracked_diff = host
        .diff(&workspace, &new, DiffKind::Worktree)
        .await
        .unwrap();
    assert_eq!(untracked_diff.original.as_deref(), Some(""));
    assert_eq!(untracked_diff.current.as_deref(), Some("new\n"));

    host.stage(&workspace, &[tracked.clone(), new.clone()])
        .await
        .unwrap();
    let status = host.status(&workspace).await.unwrap();
    assert_eq!(status.staged_count(), 2);
    let staged_new_diff = host.diff(&workspace, &new, DiffKind::Staged).await.unwrap();
    assert!(staged_new_diff.patch.contains("+new"));
    assert_eq!(staged_new_diff.original.as_deref(), Some(""));
    assert_eq!(staged_new_diff.current.as_deref(), Some("new\n"));

    host.unstage(&workspace, std::slice::from_ref(&tracked))
        .await
        .unwrap();
    host.discard(&workspace, std::slice::from_ref(&tracked))
        .await
        .unwrap();
    assert_eq!(
        fs::read_to_string(repository.path().join("tracked.txt")).unwrap(),
        "base\n"
    );

    let outcome = host
        .commit(
            &workspace,
            CommitRequest {
                message: "add new file".into(),
                amend: false,
                signing_passphrase: None,
            },
        )
        .await
        .unwrap();
    let CommitOutcome::Committed { commit } = outcome else {
        panic!("commit unexpectedly requested a signing passphrase");
    };
    assert_eq!(commit.summary, "add new file");
    assert_eq!(commit.oid.len(), 40);

    fs::write(repository.path().join("temporary.txt"), "remove me\n").unwrap();
    let temporary = RelativePath::try_from("temporary.txt").unwrap();
    host.discard(&workspace, std::slice::from_ref(&temporary))
        .await
        .unwrap();
    assert!(!repository.path().join("temporary.txt").exists());
    assert!(!host
        .status(&workspace)
        .await
        .unwrap()
        .changes
        .iter()
        .any(|change| change.path == temporary && change.worktree == Some(ChangeKind::Untracked)));
}

#[cfg(unix)]
#[tokio::test]
async fn signing_failure_becomes_an_in_app_retry_outcome() {
    use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt};

    let repository = init_repository();
    fs::write(repository.path().join("tracked.txt"), "base\n").unwrap();
    git(repository.path(), &["add", "tracked.txt"]);
    git(repository.path(), &["commit", "-m", "base"]);
    fs::write(repository.path().join("tracked.txt"), "changed\n").unwrap();
    git(repository.path(), &["add", "tracked.txt"]);
    let signer = repository.path().join("signing-fails");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .open(&signer)
        .unwrap();
    file.write_all(b"#!/bin/sh\necho 'gpg failed to sign the data' >&2\nexit 2\n")
        .unwrap();
    drop(file);
    git(
        repository.path(),
        &["config", "gpg.program", signer.to_str().unwrap()],
    );
    git(repository.path(), &["config", "commit.gpgsign", "true"]);

    let outcome = HostGit::default()
        .commit(
            &workspace(repository.path()),
            CommitRequest {
                message: "signed change".into(),
                amend: false,
                signing_passphrase: None,
            },
        )
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        CommitOutcome::SigningPassphraseRequired { .. }
    ));
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
