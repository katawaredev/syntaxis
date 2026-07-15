use super::*;
use std::time::Duration;
use syntaxis_workspace::{WorkspaceAvailability, WorkspaceIcon};
fn workspace(root: &Path) -> WorkspaceRecord {
    WorkspaceRecord {
        id: WorkspaceId::new("workspace"),
        slug: "workspace".into(),
        name: "Workspace".into(),
        root: root.to_string_lossy().into_owned(),
        icon: WorkspaceIcon::default(),
        registered_at_unix_ms: 0,
        last_opened_unix_ms: 0,
        availability: WorkspaceAvailability::Available,
    }
}
#[test]
fn shell_runs_in_workspace_and_replays_output() {
    let directory = tempfile::tempdir().unwrap();
    let manager = HostTerminalManager::default();
    let session = manager
        .create(
            &workspace(directory.path()),
            Some("test"),
            TerminalSize::DEFAULT,
        )
        .unwrap();
    let (_, mut attachment) = manager
        .attach(&WorkspaceId::new("workspace"), &session.id)
        .unwrap();
    manager
        .write(
            &WorkspaceId::new("workspace"),
            &session.id,
            b"printf 'syntaxis-pty\\n'\n",
        )
        .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let event = runtime
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(3), attachment.events.recv()).await
        })
        .unwrap()
        .unwrap();
    let HostTerminalEvent::Output { data, .. } = event else {
        panic!("expected output event");
    };
    assert!(String::from_utf8_lossy(&data).contains("syntaxis-pty"));
    manager
        .close(&WorkspaceId::new("workspace"), &session.id)
        .unwrap();
}
#[test]
fn sessions_are_scoped_to_their_workspace() {
    let directory = tempfile::tempdir().unwrap();
    let manager = HostTerminalManager::default();
    let session = manager
        .create(&workspace(directory.path()), None, TerminalSize::DEFAULT)
        .unwrap();
    let error = manager
        .write(&WorkspaceId::new("different"), &session.id, b"echo no\n")
        .unwrap_err();
    assert_eq!(error.code, TerminalErrorCode::PermissionDenied);
    manager
        .close(&WorkspaceId::new("workspace"), &session.id)
        .unwrap();
}
#[test]
fn session_names_are_unique_within_a_workspace() {
    let directory = tempfile::tempdir().unwrap();
    let manager = HostTerminalManager::default();
    let workspace = workspace(directory.path());
    let first = manager
        .create(&workspace, Some("server"), TerminalSize::DEFAULT)
        .unwrap();
    let duplicate = manager
        .create(&workspace, Some("SERVER"), TerminalSize::DEFAULT)
        .unwrap_err();
    assert_eq!(duplicate.code, TerminalErrorCode::InvalidRequest);
    assert_eq!(duplicate.message, "Name already in use.");
    manager.close(&workspace.id, &first.id).unwrap();
}
#[test]
fn automatic_names_use_the_first_available_workspace_number() {
    let directory = tempfile::tempdir().unwrap();
    let manager = HostTerminalManager::default();
    let workspace = workspace(directory.path());
    let first = manager
        .create(&workspace, None, TerminalSize::DEFAULT)
        .unwrap();
    let second = manager
        .create(&workspace, None, TerminalSize::DEFAULT)
        .unwrap();
    assert_eq!(first.name, "shell 1");
    assert_eq!(second.name, "shell 2");
    manager.close(&workspace.id, &first.id).unwrap();
    manager.close(&workspace.id, &second.id).unwrap();
}
#[test]
fn detached_sessions_are_cleaned_after_the_configured_timeout() {
    let directory = tempfile::tempdir().unwrap();
    let manager = HostTerminalManager::new(TerminalHostConfig {
        detached_timeout: Duration::ZERO,
        ..TerminalHostConfig::default()
    });
    let session = manager
        .create(&workspace(directory.path()), None, TerminalSize::DEFAULT)
        .unwrap();
    let (_, attachment) = manager
        .attach(&WorkspaceId::new("workspace"), &session.id)
        .unwrap();
    assert_eq!(
        manager.list(&WorkspaceId::new("workspace")).unwrap().len(),
        1
    );
    drop(attachment);
    manager.cleanup().unwrap();
    assert!(manager
        .list(&WorkspaceId::new("workspace"))
        .unwrap()
        .is_empty());
}

#[test]
fn exited_sessions_can_be_closed_after_the_monitor_reaps_them() {
    let directory = tempfile::tempdir().unwrap();
    let manager = HostTerminalManager::default();
    let workspace = workspace(directory.path());
    let session = manager
        .create(&workspace, None, TerminalSize::DEFAULT)
        .unwrap();
    manager
        .write(&workspace.id, &session.id, b"exit 0\n")
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let lifecycle = manager.list(&workspace.id).unwrap()[0].lifecycle;
        if lifecycle == Lifecycle::Exited {
            break;
        }
        assert!(Instant::now() < deadline, "terminal did not exit in time");
        thread::sleep(Duration::from_millis(20));
    }

    manager.close(&workspace.id, &session.id).unwrap();
    assert!(manager.list(&workspace.id).unwrap().is_empty());
}

#[test]
fn workspace_session_limit_is_enforced_without_spawning_another_pty() {
    let directory = tempfile::tempdir().unwrap();
    let manager = HostTerminalManager::new(TerminalHostConfig {
        max_sessions: 2,
        max_sessions_per_workspace: 1,
        ..TerminalHostConfig::default()
    });
    let workspace = workspace(directory.path());
    let first = manager
        .create(&workspace, None, TerminalSize::DEFAULT)
        .unwrap();
    let error = manager
        .create(&workspace, None, TerminalSize::DEFAULT)
        .unwrap_err();
    assert_eq!(error.code, TerminalErrorCode::Unavailable);
    assert_eq!(
        error.message,
        "The workspace terminal session limit has been reached"
    );
    manager.close(&workspace.id, &first.id).unwrap();
}
