use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use syntaxis_git::{
    GitError, GitErrorCode, GitResult, WorktreeCreateRequest, WorktreeInfo, WorktreeKind,
    WorktreeOperations,
};
use syntaxis_workspace::{WorkspaceAvailability, WorkspaceId, WorkspaceRecord};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{runner::validated_root, HostGit};

#[derive(Debug)]
struct PorcelainWorktree {
    root: PathBuf,
    head: String,
    branch: Option<String>,
    prunable: bool,
}

#[async_trait(?Send)]
impl WorktreeOperations for HostGit {
    async fn worktrees(&self, workspace: &WorkspaceRecord) -> GitResult<Vec<WorktreeInfo>> {
        let workspace_root = validated_root(workspace)?;
        let repository_root = self.repository_root(&workspace_root).await?;
        let workspace_relative = workspace_root
            .strip_prefix(&repository_root)
            .map_err(|_| invalid_repository())?;
        let managed_root = managed_worktrees_root(&workspace_root);
        let arguments = ["worktree", "list", "--porcelain", "-z"].map(OsString::from);
        let output = self
            .run(
                &workspace_root,
                &arguments,
                None,
                &[],
                &[0],
                CancellationToken::new(),
            )
            .await?;
        let records = parse_worktree_porcelain(&output.stdout)?;
        let canonical_workspace = canonical(&repository_root)?;
        let mut result = Vec::new();

        for record in records.into_iter().filter(|record| !record.prunable) {
            let record_root = canonical(&record.root)?;
            let effective_root = record_root.join(workspace_relative);
            if !effective_root.is_dir() {
                continue;
            }
            let primary = record_root == canonical_workspace;
            let kind = if primary {
                WorktreeKind::Primary
            } else if record_root.starts_with(&managed_root) {
                WorktreeKind::Managed
            } else {
                WorktreeKind::External
            };
            let target = if primary {
                workspace.clone()
            } else {
                worktree_workspace(workspace, &effective_root)?
            };
            result.push(WorktreeInfo {
                workspace: target,
                branch: record.branch,
                head: record.head,
                kind,
            });
        }

        result.sort_by(|left, right| {
            right
                .is_primary()
                .cmp(&left.is_primary())
                .then_with(|| left.label().cmp(&right.label()))
        });
        if result.first().is_none_or(|worktree| !worktree.is_primary()) {
            return Err(invalid_repository());
        }
        Ok(result)
    }

    async fn create_worktree(
        &self,
        workspace: &WorkspaceRecord,
        request: WorktreeCreateRequest,
    ) -> GitResult<WorktreeInfo> {
        let workspace_root = validated_root(workspace)?;
        self.repository_root(&workspace_root).await?;
        let (branch, start_point) = self
            .validate_worktree_creation(workspace, &workspace_root, &request)
            .await?;
        let target = managed_worktrees_root(&workspace_root).join(sanitize_directory(&branch));
        if target.exists() {
            return Err(invalid_request(
                "A managed worktree directory already exists for that branch.",
            ));
        }
        let parent = target.parent().ok_or_else(invalid_repository)?;
        std::fs::create_dir_all(parent).map_err(|_| {
            GitError::new(
                GitErrorCode::Unavailable,
                "The managed worktree directory could not be created.",
            )
        })?;
        self.ensure_managed_worktrees_ignored(&workspace_root)
            .await?;
        let mut arguments = vec![OsString::from("worktree"), OsString::from("add")];
        if let Some(start_point) = start_point {
            arguments.extend([
                OsString::from("-b"),
                OsString::from(&branch),
                target.as_os_str().to_owned(),
                OsString::from(start_point),
            ]);
        } else {
            arguments.extend([target.as_os_str().to_owned(), OsString::from(&branch)]);
        }
        self.run(
            &workspace_root,
            &arguments,
            None,
            &[],
            &[0],
            CancellationToken::new(),
        )
        .await?;

        self.worktrees(workspace)
            .await?
            .into_iter()
            .find(|worktree| worktree.branch.as_deref() == Some(branch.as_str()))
            .ok_or_else(|| GitError::new(GitErrorCode::Internal, "The new worktree was not found."))
    }

    async fn remove_worktree(
        &self,
        workspace: &WorkspaceRecord,
        worktree_workspace_id: &str,
        force: bool,
    ) -> GitResult<()> {
        let workspace_root = validated_root(workspace)?;
        let worktree = self
            .worktrees(workspace)
            .await?
            .into_iter()
            .find(|worktree| worktree.workspace.id.0 == worktree_workspace_id)
            .ok_or_else(|| invalid_request("The worktree no longer exists."))?;
        if !worktree.is_managed() {
            return Err(invalid_request(
                "Only worktrees managed by Syntaxis can be removed here.",
            ));
        }
        let target_root = self
            .repository_root(Path::new(&worktree.workspace.root))
            .await?;
        let mut arguments = vec![OsString::from("worktree"), OsString::from("remove")];
        if force {
            arguments.push(OsString::from("--force"));
        }
        arguments.push(target_root.into_os_string());
        self.run(
            &workspace_root,
            &arguments,
            None,
            &[],
            &[0],
            CancellationToken::new(),
        )
        .await?;
        let prune = ["worktree", "prune"].map(OsString::from);
        let _ = self
            .run(
                &workspace_root,
                &prune,
                None,
                &[],
                &[0],
                CancellationToken::new(),
            )
            .await;
        Ok(())
    }
}

impl HostGit {
    async fn validate_worktree_creation(
        &self,
        workspace: &WorkspaceRecord,
        workspace_root: &Path,
        request: &WorktreeCreateRequest,
    ) -> GitResult<(String, Option<String>)> {
        let branch = validate_branch(&request.branch)?.to_owned();
        if self
            .worktrees(workspace)
            .await?
            .iter()
            .any(|worktree| worktree.branch.as_deref() == Some(branch.as_str()))
        {
            return Err(invalid_request(
                "That branch is already checked out in another worktree.",
            ));
        }
        let branch_exists = self.local_branch_exists(workspace_root, &branch).await?;
        if request.create_branch && branch_exists {
            return Err(invalid_request(
                "That branch already exists. Choose it in the branch menu and select Open in new worktree.",
            ));
        }
        if !request.create_branch && !branch_exists {
            return Err(invalid_request(
                "That branch no longer exists. Refresh the branch menu and try again.",
            ));
        }
        if !request.create_branch {
            return Ok((branch, None));
        }

        let start_point = validate_revision(request.start_point.as_deref().unwrap_or("HEAD"))?;
        if !self.commit_exists(workspace_root, start_point).await? {
            if !self.commit_exists(workspace_root, "HEAD").await? {
                return Err(invalid_request(
                    "This repository has no commits yet. Create its first commit before creating a worktree.",
                ));
            }
            return Err(invalid_request(
                "The starting branch, tag, or commit could not be found.",
            ));
        }
        Ok((branch, Some(start_point.to_owned())))
    }

    async fn local_branch_exists(&self, root: &Path, branch: &str) -> GitResult<bool> {
        let arguments = [
            OsString::from("show-ref"),
            OsString::from("--verify"),
            OsString::from("--quiet"),
            OsString::from(format!("refs/heads/{branch}")),
        ];
        Ok(self
            .run(
                root,
                &arguments,
                None,
                &[],
                &[0, 1],
                CancellationToken::new(),
            )
            .await?
            .status
            .success())
    }

    async fn commit_exists(&self, root: &Path, revision: &str) -> GitResult<bool> {
        let arguments = [
            OsString::from("rev-parse"),
            OsString::from("--verify"),
            OsString::from("--quiet"),
            OsString::from("--end-of-options"),
            OsString::from(format!("{revision}^{{commit}}")),
        ];
        Ok(self
            .run(
                root,
                &arguments,
                None,
                &[],
                &[0, 1],
                CancellationToken::new(),
            )
            .await?
            .status
            .success())
    }

    async fn repository_root(&self, root: &Path) -> GitResult<PathBuf> {
        let arguments = ["rev-parse", "--show-toplevel"].map(OsString::from);
        let output = self
            .run(root, &arguments, None, &[], &[0], CancellationToken::new())
            .await?;
        let value = String::from_utf8(output.stdout).map_err(|_| invalid_repository())?;
        canonical(Path::new(value.trim()))
    }

    async fn ensure_managed_worktrees_ignored(&self, root: &Path) -> GitResult<()> {
        let arguments = ["rev-parse", "--git-path", "info/exclude"].map(OsString::from);
        let output = self
            .run(root, &arguments, None, &[], &[0], CancellationToken::new())
            .await?;
        let value = String::from_utf8(output.stdout).map_err(|_| invalid_repository())?;
        let path = PathBuf::from(value.trim());
        let path = if path.is_absolute() {
            path
        } else {
            root.join(path)
        };
        let mut existing = std::fs::read_to_string(&path).unwrap_or_default();
        let rule = ".syntaxis-worktrees/";
        if existing.lines().any(|line| line.trim() == rule) {
            return Ok(());
        }
        if !existing.is_empty() && !existing.ends_with('\n') {
            existing.push('\n');
        }
        existing.push_str("# Syntaxis managed worktrees\n");
        existing.push_str(rule);
        existing.push('\n');
        std::fs::write(path, existing).map_err(|_| {
            GitError::new(
                GitErrorCode::Unavailable,
                "The local Git exclude file could not be updated.",
            )
        })
    }
}

fn parse_worktree_porcelain(output: &[u8]) -> GitResult<Vec<PorcelainWorktree>> {
    let mut records = Vec::new();
    let mut current: Option<PorcelainWorktree> = None;
    for field in output.split(|byte| *byte == 0) {
        if field.is_empty() {
            if let Some(record) = current.take() {
                records.push(record);
            }
            continue;
        }
        let field = String::from_utf8(field.to_vec()).map_err(|_| invalid_repository())?;
        if let Some(root) = field.strip_prefix("worktree ") {
            if let Some(record) = current.take() {
                records.push(record);
            }
            current = Some(PorcelainWorktree {
                root: PathBuf::from(root),
                head: String::new(),
                branch: None,
                prunable: false,
            });
        } else if let Some(record) = current.as_mut() {
            if let Some(head) = field.strip_prefix("HEAD ") {
                head.clone_into(&mut record.head);
            } else if let Some(branch) = field.strip_prefix("branch refs/heads/") {
                record.branch = Some(branch.to_owned());
            } else if field.starts_with("prunable") {
                record.prunable = true;
            }
        }
    }
    if let Some(record) = current {
        records.push(record);
    }
    if records.iter().any(|record| record.head.is_empty()) {
        return Err(invalid_repository());
    }
    Ok(records)
}

fn worktree_workspace(base: &WorkspaceRecord, root: &Path) -> GitResult<WorkspaceRecord> {
    let root = canonical(root)?;
    let namespace = Uuid::new_v5(&Uuid::NAMESPACE_URL, base.id.0.as_bytes());
    let target_id = Uuid::new_v5(&namespace, root.as_os_str().as_encoded_bytes());
    let mut workspace = base.clone();
    workspace.id = WorkspaceId::new(format!("{}:worktree:{target_id}", base.id.0));
    workspace.root = root.to_string_lossy().into_owned();
    workspace.availability = WorkspaceAvailability::Available;
    Ok(workspace)
}

fn managed_worktrees_root(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".syntaxis-worktrees")
}

fn validate_branch(value: &str) -> GitResult<&str> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.len() <= 200
        && !value.chars().any(char::is_control)
        && !value.starts_with('-')
        && !value.contains("..")
        && !value.contains("@{")
        && !value.ends_with(['.', '/'])
        && !value.contains([' ', '~', '^', ':', '?', '*', '[', '\\']);
    if valid {
        Ok(value)
    } else {
        Err(invalid_request("Choose a valid Git branch name."))
    }
}

fn validate_revision(value: &str) -> GitResult<&str> {
    let value = value.trim();
    if !value.is_empty()
        && value.len() <= 500
        && !value.starts_with('-')
        && !value.chars().any(char::is_control)
    {
        Ok(value)
    } else {
        Err(invalid_request("Choose a valid starting revision."))
    }
}

fn sanitize_directory(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    sanitized.trim_matches('-').to_owned()
}

fn canonical(path: &Path) -> GitResult<PathBuf> {
    path.canonicalize().map_err(|_| invalid_repository())
}

fn invalid_request(message: &'static str) -> GitError {
    GitError::new(GitErrorCode::Unsupported, message)
}

fn invalid_repository() -> GitError {
    GitError::new(
        GitErrorCode::InvalidWorkspace,
        "The Git worktree layout could not be resolved.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use syntaxis_workspace::WorkspaceIcon;

    #[test]
    fn parses_nul_delimited_porcelain_records() {
        let output = b"worktree /repo\0HEAD abcdef123\0branch refs/heads/main\0\0worktree /other\0HEAD 123456789\0detached\0\0";
        let parsed = parse_worktree_porcelain(output).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].branch.as_deref(), Some("main"));
        assert_eq!(parsed[1].branch, None);
    }

    #[test]
    fn rejects_dangerous_branch_names() {
        for branch in ["", "-force", "feature name", "feature..oops", "topic@{1}"] {
            assert!(validate_branch(branch).is_err(), "{branch}");
        }
        assert_eq!(validate_branch("agent/issue-42").unwrap(), "agent/issue-42");
    }

    #[tokio::test]
    async fn creates_lists_and_removes_an_isolated_worktree() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        std::fs::create_dir(&root).unwrap();
        git(&root, &["init", "-b", "main"]);
        git(&root, &["config", "user.name", "Syntaxis Test"]);
        git(&root, &["config", "user.email", "syntaxis@example.test"]);
        git(&root, &["config", "commit.gpgsign", "false"]);
        std::fs::create_dir(root.join("app")).unwrap();
        std::fs::write(root.join("app/README.md"), "root\n").unwrap();
        git(&root, &["add", "app/README.md"]);
        git(&root, &["commit", "-m", "initial"]);
        let workspace = WorkspaceRecord {
            id: WorkspaceId::new("workspace-id"),
            slug: "project".into(),
            name: "Project".into(),
            root: root.join("app").to_string_lossy().into_owned(),
            icon: WorkspaceIcon::default(),
            registered_at_unix_ms: 0,
            last_opened_unix_ms: 0,
            availability: WorkspaceAvailability::Available,
        };
        let host = HostGit::default();

        let initial = host.worktrees(&workspace).await.unwrap();
        assert_eq!(initial.len(), 1);
        assert!(initial[0].is_primary());

        let created = host
            .create_worktree(
                &workspace,
                WorktreeCreateRequest {
                    branch: "agent/issue-42".into(),
                    start_point: None,
                    create_branch: true,
                },
            )
            .await
            .unwrap();
        assert!(created.is_managed());
        assert_eq!(created.branch.as_deref(), Some("agent/issue-42"));
        assert_ne!(created.workspace.id, workspace.id);
        assert!(Path::new(&created.workspace.root).is_dir());
        assert!(created.workspace.root.ends_with("agent-issue-42/app"));
        assert!(command_output(&root, &["status", "--porcelain"]).is_empty());

        let listed = host.worktrees(&workspace).await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[1].workspace.id, created.workspace.id);

        host.remove_worktree(&workspace, &created.workspace.id.0, false)
            .await
            .unwrap();
        assert!(!Path::new(&created.workspace.root).exists());
        assert_eq!(host.worktrees(&workspace).await.unwrap().len(), 1);
        let branches = command_output(&root, &["branch", "--list", "agent/issue-42"]);
        assert!(branches.contains("agent/issue-42"));

        let duplicate = host
            .create_worktree(
                &workspace,
                WorktreeCreateRequest {
                    branch: "agent/issue-42".into(),
                    start_point: None,
                    create_branch: true,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(duplicate.code, GitErrorCode::Unsupported);
        assert!(duplicate.message.contains("already exists"));

        let reopened = host
            .create_worktree(
                &workspace,
                WorktreeCreateRequest {
                    branch: "agent/issue-42".into(),
                    start_point: None,
                    create_branch: false,
                },
            )
            .await
            .unwrap();
        assert_eq!(reopened.branch.as_deref(), Some("agent/issue-42"));
        assert!(Path::new(&reopened.workspace.root).is_dir());
        host.remove_worktree(&workspace, &reopened.workspace.id.0, false)
            .await
            .unwrap();

        let missing_start = host
            .create_worktree(
                &workspace,
                WorktreeCreateRequest {
                    branch: "agent/missing-start".into(),
                    start_point: Some("missing-revision".into()),
                    create_branch: true,
                },
            )
            .await
            .unwrap_err();
        assert!(missing_start.message.contains("could not be found"));
    }

    #[tokio::test]
    async fn explains_that_an_unborn_repository_cannot_create_a_worktree() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        std::fs::create_dir(&root).unwrap();
        git(&root, &["init", "-b", "main"]);
        let workspace = WorkspaceRecord {
            id: WorkspaceId::new("unborn-workspace"),
            slug: "unborn".into(),
            name: "Unborn".into(),
            root: root.to_string_lossy().into_owned(),
            icon: WorkspaceIcon::default(),
            registered_at_unix_ms: 0,
            last_opened_unix_ms: 0,
            availability: WorkspaceAvailability::Available,
        };

        let error = HostGit::default()
            .create_worktree(
                &workspace,
                WorktreeCreateRequest {
                    branch: "feature/first".into(),
                    start_point: Some("main".into()),
                    create_branch: true,
                },
            )
            .await
            .unwrap_err();

        assert!(error.message.contains("no commits yet"));
        assert!(error.message.contains("first commit"));
    }

    fn git(root: &Path, arguments: &[&str]) {
        let status = Command::new("git")
            .args(arguments)
            .current_dir(root)
            .status()
            .unwrap();
        assert!(status.success(), "git {arguments:?}");
    }

    fn command_output(root: &Path, arguments: &[&str]) -> String {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {arguments:?}");
        String::from_utf8(output.stdout).unwrap()
    }
}
