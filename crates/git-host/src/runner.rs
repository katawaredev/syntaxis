use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use syntaxis_git::{
    ClonePhase, CloneProgress, GitError, GitErrorCode, GitResult, RepositoryStatus,
};
use syntaxis_workspace::{WorkspaceAvailability, WorkspaceRecord};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::mpsc,
};
use tokio_util::sync::CancellationToken;

use crate::parser::parse_status;

#[derive(Clone, Debug)]
pub struct HostGitConfig {
    pub timeout: Duration,
    pub commit_timeout: Duration,
    pub clone_timeout: Duration,
    pub max_output_bytes: usize,
}

impl Default for HostGitConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            commit_timeout: Duration::from_mins(10),
            clone_timeout: Duration::from_mins(5),
            max_output_bytes: 8 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct HostGit {
    pub(crate) config: HostGitConfig,
}

impl HostGit {
    pub fn new(config: HostGitConfig) -> Self {
        Self { config }
    }

    /// Reads repository status, stopping early when `cancellation` is triggered.
    ///
    /// # Errors
    ///
    /// Returns a structured error when the workspace is invalid, Git is
    /// unavailable, the command is cancelled or exceeds its limits, or its
    /// machine-readable output cannot be parsed.
    pub async fn status_with_cancellation(
        &self,
        workspace: &WorkspaceRecord,
        cancellation: CancellationToken,
    ) -> GitResult<RepositoryStatus> {
        let root = validated_root(workspace)?;
        let arguments = [
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=all",
        ]
        .map(OsString::from);
        let output = self
            .run(&root, &arguments, None, &[], &[0], cancellation)
            .await?;
        parse_status(&output.stdout)
    }

    pub(crate) async fn run(
        &self,
        root: &Path,
        arguments: &[OsString],
        stdin: Option<&[u8]>,
        environment: &[(&str, OsString)],
        accepted_exit_codes: &[i32],
        cancellation: CancellationToken,
    ) -> GitResult<GitOutput> {
        let mut command = Command::new("git");
        command
            .args(arguments)
            .current_dir(root)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        clear_inherited_command_config(&mut command);
        for (name, value) in environment {
            command.env(name, value);
        }
        let mut child = command.spawn().map_err(|error| map_spawn_error(&error))?;

        if let Some(input) = stdin {
            let mut child_stdin = child.stdin.take().ok_or_else(internal_error)?;
            child_stdin
                .write_all(input)
                .await
                .map_err(|_| internal_error())?;
            drop(child_stdin);
        }

        let stdout = child.stdout.take().ok_or_else(internal_error)?;
        let stderr = child.stderr.take().ok_or_else(internal_error)?;
        let limit = self.config.max_output_bytes;
        let collect = async {
            let wait = async { child.wait().await.map_err(|_| internal_error()) };
            let (status, stdout, stderr) = tokio::try_join!(
                wait,
                read_limited(stdout, limit),
                read_limited(stderr, limit)
            )?;
            Ok::<_, GitError>(GitOutput {
                status,
                stdout,
                stderr,
            })
        };

        let result = tokio::select! {
            biased;
            () = cancellation.cancelled() => Err(GitError::new(
                GitErrorCode::Cancelled,
                "The Git operation was cancelled.",
            )),
            result = tokio::time::timeout(self.config.timeout, collect) => {
                result.unwrap_or_else(|_| Err(GitError::new(
                    GitErrorCode::TimedOut,
                    "The Git operation timed out.",
                )))
            }
        };

        let output = result?;
        if output
            .status
            .code()
            .is_some_and(|code| accepted_exit_codes.contains(&code))
        {
            Ok(output)
        } else {
            Err(command_error(&output))
        }
    }

    pub(crate) async fn run_with_progress(
        &self,
        root: &Path,
        arguments: &[OsString],
        environment: &[(&str, OsString)],
        cancellation: CancellationToken,
        progress: &mpsc::Sender<CloneProgress>,
    ) -> GitResult<GitOutput> {
        let mut command = Command::new("git");
        command
            .args(arguments)
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        clear_inherited_command_config(&mut command);
        for (name, value) in environment {
            command.env(name, value);
        }
        let mut child = command.spawn().map_err(|error| map_spawn_error(&error))?;
        let stdout = child.stdout.take().ok_or_else(internal_error)?;
        let stderr = child.stderr.take().ok_or_else(internal_error)?;
        let limit = self.config.max_output_bytes;
        let collect = async {
            let wait = async { child.wait().await.map_err(|_| internal_error()) };
            let (status, stdout, stderr) = tokio::try_join!(
                wait,
                read_limited(stdout, limit),
                read_clone_progress(stderr, limit, progress)
            )?;
            Ok::<_, GitError>(GitOutput {
                status,
                stdout,
                stderr,
            })
        };
        let result = tokio::select! {
            biased;
            () = cancellation.cancelled() => Err(GitError::new(
                GitErrorCode::Cancelled,
                "The Git operation was cancelled.",
            )),
            result = tokio::time::timeout(self.config.timeout, collect) => {
                result.unwrap_or_else(|_| Err(GitError::new(
                    GitErrorCode::TimedOut,
                    "The Git operation timed out.",
                )))
            }
        }?;
        if result.status.success() {
            Ok(result)
        } else {
            Err(command_error(&result))
        }
    }
}

fn clear_inherited_command_config(command: &mut Command) {
    for (name, _) in std::env::vars_os() {
        let name_text = name.to_string_lossy();
        if name_text == "GIT_CONFIG_PARAMETERS"
            || name_text == "GIT_CONFIG_COUNT"
            || name_text.starts_with("GIT_CONFIG_KEY_")
            || name_text.starts_with("GIT_CONFIG_VALUE_")
        {
            command.env_remove(name);
        }
    }
}

pub(crate) struct GitOutput {
    pub(crate) status: std::process::ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

async fn read_limited(mut reader: impl AsyncRead + Unpin, max_bytes: usize) -> GitResult<Vec<u8>> {
    let mut output = Vec::with_capacity(max_bytes.min(64 * 1024));
    let mut buffer = vec![0_u8; 8192];
    loop {
        let count = reader
            .read(&mut buffer)
            .await
            .map_err(|_| internal_error())?;
        if count == 0 {
            return Ok(output);
        }
        if output.len().saturating_add(count) > max_bytes {
            return Err(GitError::new(
                GitErrorCode::OutputTooLarge,
                "Git produced more output than this operation allows.",
            ));
        }
        output.extend_from_slice(&buffer[..count]);
    }
}

async fn read_clone_progress(
    mut reader: impl AsyncRead + Unpin,
    max_bytes: usize,
    progress: &mpsc::Sender<CloneProgress>,
) -> GitResult<Vec<u8>> {
    let mut output = Vec::with_capacity(max_bytes.min(64 * 1024));
    let mut pending = Vec::new();
    let mut buffer = vec![0_u8; 8192];
    loop {
        let count = reader
            .read(&mut buffer)
            .await
            .map_err(|_| internal_error())?;
        if count == 0 {
            forward_progress_line(&pending, progress);
            return Ok(output);
        }
        if output.len().saturating_add(count) > max_bytes {
            return Err(GitError::new(
                GitErrorCode::OutputTooLarge,
                "Git produced more output than this operation allows.",
            ));
        }
        output.extend_from_slice(&buffer[..count]);
        pending.extend_from_slice(&buffer[..count]);
        while let Some(separator) = pending
            .iter()
            .position(|byte| matches!(byte, b'\r' | b'\n'))
        {
            let line = pending.drain(..=separator).collect::<Vec<_>>();
            forward_progress_line(&line, progress);
        }
    }
}

fn forward_progress_line(line: &[u8], progress: &mpsc::Sender<CloneProgress>) {
    if let Some(update) = parse_clone_progress(&String::from_utf8_lossy(line)) {
        let _ = progress.try_send(update);
    }
}

fn parse_clone_progress(line: &str) -> Option<CloneProgress> {
    let phase = if line.contains("Cloning into") {
        ClonePhase::Preparing
    } else if line.contains("Counting objects") {
        ClonePhase::Counting
    } else if line.contains("Compressing objects") {
        ClonePhase::Compressing
    } else if line.contains("Receiving objects") {
        ClonePhase::Receiving
    } else if line.contains("Resolving deltas") {
        ClonePhase::Resolving
    } else if line.contains("Checking out files") || line.contains("Updating files") {
        ClonePhase::CheckingOut
    } else {
        return None;
    };
    Some(CloneProgress {
        phase,
        percent: parse_percent(line),
    })
}

fn parse_percent(line: &str) -> Option<u8> {
    let percent = line.find('%')?;
    let digits = line[..percent]
        .rsplit(|character: char| !character.is_ascii_digit())
        .next()?;
    digits.parse::<u8>().ok().filter(|value| *value <= 100)
}

pub(crate) fn validated_root(workspace: &WorkspaceRecord) -> GitResult<PathBuf> {
    if workspace.availability != WorkspaceAvailability::Available {
        return Err(GitError::new(
            GitErrorCode::InvalidWorkspace,
            "The workspace is not currently available.",
        ));
    }
    let registered = PathBuf::from(&workspace.root);
    if !registered.is_absolute() {
        return Err(invalid_workspace());
    }
    let canonical = registered.canonicalize().map_err(|_| invalid_workspace())?;
    if canonical != registered || !canonical.is_dir() {
        return Err(invalid_workspace());
    }
    Ok(canonical)
}

fn invalid_workspace() -> GitError {
    GitError::new(
        GitErrorCode::InvalidWorkspace,
        "The workspace root is unavailable or has changed.",
    )
}

fn map_spawn_error(error: &std::io::Error) -> GitError {
    if error.kind() == std::io::ErrorKind::NotFound {
        GitError::new(
            GitErrorCode::Unavailable,
            "The system Git executable is unavailable.",
        )
    } else {
        internal_error()
    }
}

fn command_error(output: &GitOutput) -> GitError {
    let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    let (code, message) = if stderr.contains("not a git repository") {
        (
            GitErrorCode::NotRepository,
            "This workspace is not a Git repository.",
        )
    } else if stderr.contains("authentication failed")
        || stderr.contains("could not read username")
        || stderr.contains("permission denied (publickey)")
    {
        (
            GitErrorCode::Authentication,
            "Git authentication failed. Check the host credential helper or SSH agent.",
        )
    } else if stderr.contains("non-fast-forward")
        || stderr.contains("fetch first")
        || stderr.contains("[rejected]")
    {
        (
            GitErrorCode::NonFastForward,
            "The remote rejected this non-fast-forward push.",
        )
    } else if stderr.contains("gpg failed to sign")
        || stderr.contains("signing failed")
        || stderr.contains("failed to write commit object")
    {
        (
            GitErrorCode::SigningPassphraseRequired,
            "Git could not unlock the configured signing key.",
        )
    } else if stderr.contains("index.lock") {
        (
            GitErrorCode::Conflict,
            "The repository is locked by another Git operation.",
        )
    } else {
        (
            GitErrorCode::CommandFailed,
            "The Git command could not be completed.",
        )
    };
    GitError::new(code, message).with_exit_code(output.status.code())
}

fn internal_error() -> GitError {
    GitError::new(
        GitErrorCode::Internal,
        "The Git operation could not be completed.",
    )
}

#[cfg(test)]
mod tests {
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
            change.path.as_str() == "untracked.txt"
                && change.worktree == Some(ChangeKind::Untracked)
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
            registered_at_unix_ms: 0,
            last_opened_unix_ms: 0,
            availability: WorkspaceAvailability::Available,
        }
    }
}
