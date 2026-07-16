use super::*;

#[test]
fn command_markers_are_removed_across_chunk_boundaries() {
    let mut parser = CommandMarkerParser::default();
    let (visible, markers) = parser.push(b"before\x1b]777;synt");
    assert_eq!(visible, b"before");
    assert!(markers.is_empty());
    let (visible, markers) =
        parser.push(b"axis;command-start\x07during\x1b]777;syntaxis;command-end;7\x07after");
    assert_eq!(visible, b"duringafter");
    assert_eq!(
        markers,
        vec![CommandMarker::Started, CommandMarker::Finished(7)]
    );
}
use std::time::Duration;
use syntaxis_workspace::{WorkspaceAvailability, WorkspaceIcon};
fn workspace(root: &Path) -> WorkspaceRecord {
    WorkspaceRecord {
        id: WorkspaceId::new("workspace"),
        slug: "workspace".into(),
        name: "Workspace".into(),
        root: root.to_string_lossy().into_owned(),
        icon: WorkspaceIcon::default(),
        profile: syntaxis_workspace::WorkspaceProfile::default(),
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
fn detached_foreground_command_pauses_and_then_restarts_expiry() {
    let directory = tempfile::tempdir().unwrap();
    let mut workspace = workspace(directory.path());
    workspace.id = WorkspaceId::new("command-expiry-workspace");
    workspace.slug = "command-expiry-workspace".into();
    let manager = HostTerminalManager::new(TerminalHostConfig {
        detached_timeout: Duration::from_millis(150),
        cleanup_interval: Duration::from_secs(10),
        ..TerminalHostConfig::default()
    });
    let session = manager
        .create(&workspace, Some("long command"), TerminalSize::DEFAULT)
        .unwrap();
    let target = NotificationTarget::Terminal {
        session_id: session.id.0.clone(),
    };
    notifications().clear(&workspace.id.0, &target);
    let (_, attachment) = manager.attach(&workspace.id, &session.id).unwrap();
    thread::sleep(Duration::from_millis(100));
    manager
        .write(&workspace.id, &session.id, b"sleep 1\n")
        .unwrap();

    let active_deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if lock(&manager.session(&workspace.id, &session.id).unwrap().state)
            .unwrap()
            .command_active
        {
            break;
        }
        assert!(
            Instant::now() < active_deadline,
            "Bash command marker was not received"
        );
        thread::sleep(Duration::from_millis(20));
    }
    drop(attachment);
    thread::sleep(Duration::from_millis(200));
    manager.cleanup().unwrap();
    assert_eq!(manager.list(&workspace.id).unwrap().len(), 1);

    let completion_deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if notifications().snapshot().iter().any(|notification| {
            notification.workspace_id == workspace.id.0 && notification.target == target
        }) {
            break;
        }
        assert!(
            Instant::now() < completion_deadline,
            "command completion notification was not published"
        );
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(manager.list(&workspace.id).unwrap().len(), 1);
    thread::sleep(Duration::from_millis(200));
    let terminal_session = manager.session(&workspace.id, &session.id).unwrap();
    let command_active = lock(&terminal_session.state).unwrap().command_active;
    let attached = terminal_session.attached.load(Ordering::Relaxed);
    let detached_for =
        Instant::now().duration_since(*lock(&terminal_session.last_detached).unwrap());
    manager.cleanup().unwrap();
    assert!(
        manager.list(&workspace.id).unwrap().is_empty(),
        "active={command_active} attached={attached} detached_for={detached_for:?}"
    );
    assert!(!notifications()
        .snapshot()
        .iter()
        .any(|notification| notification.workspace_id == workspace.id.0
            && notification.target == target));
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
