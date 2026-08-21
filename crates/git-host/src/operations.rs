use std::{ffi::OsString, path::Path};

use async_trait::async_trait;
use syntaxis_git::{
    BranchComparison, BranchInfo, BranchRequest, CloneMode, ClonePhase, CloneProgress,
    CloneRequest, CloneResult, CommitDetail, CommitInfo, CommitOutcome, CommitRequest,
    CommitResult, ConflictFile, ConflictRequest, DiffKind, GitError, GitErrorCode, GitOperations,
    GitResult, HunkAction, HunkRequest, MergeOutcome, PushOutcome, RemoteInfo, RemoteRequest,
    RemoteResult, RepositoryStatus, TagInfo, TagRequest, UnifiedDiff, parse_conflict_file,
    parse_diff_hunks, resolve_conflict_block,
};
use syntaxis_workspace::{
    ErrorCode as WorkspaceErrorCode, RelativePath, WorkspaceFiles, WorkspaceRecord,
};
use syntaxis_workspace_host::HostWorkspaceFiles;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::runner::{HostGit, validated_root};

mod network;
mod parsing;
mod refs_history;
mod refs_tags;
mod signing;
mod validation;

#[cfg(test)]
use network::{GITHUB_SSH_PUSH_REWRITE, push_arguments};
use parsing::{
    apply_path_stats, apply_untracked_stats, parse_branches, parse_commit_record,
    parse_comparison_counts, parse_error, parse_history, parse_numstat, parse_path_numstat,
    parse_utf8, trim_ascii_end,
};
use signing::signing_wrapper;
use validation::{
    canonical_clone_parent, clone_directory_name, validate_clone_directory_name,
    validate_clone_url, validate_commit_request, validate_remote_request, validate_revision,
};

const MAX_CONFLICT_FILE_BYTES: u64 = 8 * 1024 * 1024;

impl HostGit {
    /// Returns ignored, untracked paths using Git's complete exclude rules.
    ///
    /// # Errors
    ///
    /// Returns an error if the workspace is invalid, Git fails, or an ignored
    /// path cannot be represented as UTF-8.
    pub async fn ignored_paths(&self, workspace: &WorkspaceRecord) -> GitResult<Vec<String>> {
        let root = validated_root(workspace)?;
        let arguments = [
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
            "-z",
        ]
        .map(OsString::from);
        let output = self
            .run(&root, &arguments, None, &[], &[0], CancellationToken::new())
            .await?;
        output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| {
                std::str::from_utf8(path)
                    .map(|path| path.trim_end_matches('/').to_owned())
                    .map_err(|_| {
                        GitError::new(
                            GitErrorCode::Parse,
                            "Git returned a non-UTF-8 ignored path.",
                        )
                    })
            })
            .collect()
    }
}

#[async_trait(?Send)]
impl GitOperations for HostGit {
    async fn clone_repository(&self, request: CloneRequest) -> GitResult<CloneResult> {
        let (progress, _progress_receiver) = mpsc::channel(1);
        self.clone_repository_with_progress(request, CancellationToken::new(), progress)
            .await
    }

    async fn initialize(&self, workspace: &WorkspaceRecord) -> GitResult<()> {
        match self.status(workspace).await {
            Ok(_) => {
                return Err(GitError::new(
                    GitErrorCode::Conflict,
                    "This workspace is already a Git repository.",
                ));
            }
            Err(error) if error.code == GitErrorCode::NotRepository => {}
            Err(error) => return Err(error),
        }
        let root = validated_root(workspace)?;
        self.run_default(&root, &["init".into(), "-b".into(), "main".into()])
            .await?;
        Ok(())
    }

    async fn status(&self, workspace: &WorkspaceRecord) -> GitResult<RepositoryStatus> {
        let mut status = self
            .status_with_cancellation(workspace, CancellationToken::new())
            .await?;
        let root = validated_root(workspace)?;
        for (staged, arguments) in [
            (
                false,
                vec![
                    "diff".into(),
                    "--numstat".into(),
                    "--no-ext-diff".into(),
                    "--no-color".into(),
                    "-z".into(),
                    "--".into(),
                ],
            ),
            (
                true,
                vec![
                    "diff".into(),
                    "--cached".into(),
                    "--numstat".into(),
                    "--no-ext-diff".into(),
                    "--no-color".into(),
                    "-z".into(),
                    "--".into(),
                ],
            ),
        ] {
            let output = self.run_default(&root, &arguments).await?;
            apply_path_stats(&mut status, &parse_path_numstat(&output.stdout)?, staged);
        }
        apply_untracked_stats(&root, &mut status, self.config.max_output_bytes);
        Ok(status)
    }

    async fn diff(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        kind: DiffKind,
    ) -> GitResult<UnifiedDiff> {
        repository_diff_with_context(self, workspace, path, kind, 3).await
    }

    async fn stage(&self, workspace: &WorkspaceRecord, paths: &[RelativePath]) -> GitResult<()> {
        self.run_paths(workspace, &["add"], paths).await
    }

    async fn unstage(&self, workspace: &WorkspaceRecord, paths: &[RelativePath]) -> GitResult<()> {
        self.run_paths(workspace, &["reset", "--quiet", "HEAD"], paths)
            .await
    }

    async fn discard(&self, workspace: &WorkspaceRecord, paths: &[RelativePath]) -> GitResult<()> {
        require_paths(paths)?;
        let status = self.status(workspace).await?;
        let mut tracked = Vec::new();
        let mut untracked = Vec::new();
        for path in paths {
            if status.changes.iter().any(|change| {
                change.path == *path && change.worktree == Some(syntaxis_git::ChangeKind::Untracked)
            }) {
                untracked.push(path.clone());
            } else {
                tracked.push(path.clone());
            }
        }
        if !tracked.is_empty() {
            self.run_paths(workspace, &["restore", "--worktree"], &tracked)
                .await?;
        }
        if !untracked.is_empty() {
            self.run_paths(workspace, &["clean", "-f", "-d"], &untracked)
                .await?;
        }
        Ok(())
    }

    async fn apply_hunk(&self, workspace: &WorkspaceRecord, request: HunkRequest) -> GitResult<()> {
        let root = validated_root(workspace)?;
        let diff = self.diff(workspace, &request.path, request.kind).await?;
        let hunks = parse_diff_hunks(&diff.patch)?;
        let hunk = hunks.get(request.hunk_index).ok_or_else(|| {
            GitError::new(
                GitErrorCode::Conflict,
                "The selected hunk no longer exists. Refresh the repository and try again.",
            )
        })?;
        if hunk.fingerprint != request.expected_fingerprint {
            return Err(GitError::new(
                GitErrorCode::Conflict,
                "The selected hunk changed. Refresh the repository and review it again.",
            ));
        }
        let mode = match (request.action, request.kind) {
            (HunkAction::Stage, DiffKind::Worktree) => (true, false),
            (HunkAction::Unstage, DiffKind::Staged) => (true, true),
            (HunkAction::Discard, DiffKind::Worktree) => (false, true),
            _ => {
                return Err(GitError::new(
                    GitErrorCode::Conflict,
                    "That hunk action does not match the selected diff.",
                ));
            }
        };
        let mut arguments = vec![
            "apply".into(),
            "--recount".into(),
            "--whitespace=nowarn".into(),
        ];
        if mode.0 {
            arguments.push("--cached".into());
        }
        if mode.1 {
            arguments.push("--reverse".into());
        }
        arguments.push("-".into());
        self.run(
            &root,
            &arguments,
            Some(hunk.patch.as_bytes()),
            &[],
            &[0],
            CancellationToken::new(),
        )
        .await?;
        Ok(())
    }

    async fn commit(
        &self,
        workspace: &WorkspaceRecord,
        mut request: CommitRequest,
    ) -> GitResult<CommitOutcome> {
        validate_commit_request(&request)?;
        let root = validated_root(workspace)?;
        let mut arguments = vec!["commit".into(), "-m".into(), request.message.clone().into()];
        if request.amend {
            arguments.push("--amend".into());
        }
        if request.skip_hooks {
            arguments.push("--no-verify".into());
        }

        let passphrase = request.signing_passphrase.take().map(Zeroizing::new);
        let signing_configured = self
            .git_config(&root, "commit.gpgsign")
            .await?
            .as_deref()
            .is_some_and(config_enabled);
        let signing_requested = signing_configured || passphrase.is_some();
        let mut signing_directory = None;
        let mut environment = Vec::new();
        if signing_requested {
            let configured_format = self.git_config(&root, "gpg.format").await?;
            if configured_format
                .as_deref()
                .is_some_and(|format| !format.is_empty() && format != "openpgp")
            {
                return Err(GitError::new(
                    GitErrorCode::Unavailable,
                    "In-app passphrase retry currently supports OpenPGP signing keys only.",
                ));
            }
            let configured_program = self
                .git_config(&root, "gpg.program")
                .await?
                .unwrap_or_else(|| "gpg".into());
            let configured_program = if std::env::var_os("SYNTAXIS_GPG_WRAPPER").as_deref()
                == Some(Path::new(&configured_program).as_os_str())
            {
                std::env::var_os("SYNTAXIS_GPG_PROGRAM").unwrap_or_else(|| "gpg".into())
            } else {
                configured_program.into()
            };
            let passphrase = passphrase
                .as_ref()
                .map_or(&[][..], |value| value.as_bytes());
            let (directory, wrapper, passphrase_file) = signing_wrapper(passphrase)?;
            arguments.splice(
                0..0,
                [
                    "-c".into(),
                    format!("gpg.program={}", wrapper.to_string_lossy()).into(),
                ],
            );
            arguments.push("--gpg-sign".into());
            environment.push(("SYNTAXIS_GPG_PROGRAM", configured_program));
            environment.push(("SYNTAXIS_GPG_WRAPPER", wrapper.into_os_string()));
            environment.push(("SYNTAXIS_GPG_PASSPHRASE_FILE", passphrase_file.into()));
            signing_directory = Some(directory);
        }

        let mut commit_host = self.clone();
        commit_host.config.timeout = commit_host.config.commit_timeout;
        let result = commit_host
            .run(
                &root,
                &arguments,
                None,
                &environment,
                &[0],
                CancellationToken::new(),
            )
            .await;
        drop(signing_directory);
        match result {
            Ok(_) => {}
            Err(error) if error.code == GitErrorCode::SigningPassphraseRequired => {
                return Ok(CommitOutcome::SigningPassphraseRequired {
                    message: error.message,
                });
            }
            Err(error) if error.code == GitErrorCode::CommandFailed => {
                return Err(GitError::new(
                    GitErrorCode::CommandFailed,
                    "Git or a configured commit hook rejected the commit.",
                )
                .with_exit_code(error.exit_code));
            }
            Err(error) => return Err(error),
        }

        let oid = self.rev_parse_head(&root).await?;
        let summary = request
            .message
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned();
        Ok(CommitOutcome::Committed {
            commit: CommitResult { oid, summary },
        })
    }

    async fn branches(&self, workspace: &WorkspaceRecord) -> GitResult<Vec<BranchInfo>> {
        let root = validated_root(workspace)?;
        let arguments = [
            "for-each-ref".into(),
            "--format=%(refname:short)%00%(HEAD)%00%(upstream:short)%00%(refname)".into(),
            "refs/heads".into(),
            "refs/remotes".into(),
        ];
        let output = self.run_default(&root, &arguments).await?;
        parse_branches(&output.stdout)
    }

    async fn create_branch(
        &self,
        workspace: &WorkspaceRecord,
        request: BranchRequest,
    ) -> GitResult<()> {
        let root = validated_root(workspace)?;
        self.validate_branch_name(&root, &request.name).await?;
        let mut arguments = vec!["switch".into(), "-c".into(), request.name.into()];
        if let Some(start_point) = request.start_point {
            validate_revision(&start_point)?;
            arguments.push(start_point.into());
        }
        self.run_default(&root, &arguments).await?;
        Ok(())
    }

    async fn switch_branch(&self, workspace: &WorkspaceRecord, name: &str) -> GitResult<()> {
        let root = validated_root(workspace)?;
        self.validate_branch_name(&root, name).await?;
        let remote_ref = format!("refs/remotes/{name}");
        let check_arguments = [
            "show-ref".into(),
            "--verify".into(),
            "--quiet".into(),
            remote_ref.into(),
        ];
        let remote = self
            .run(
                &root,
                &check_arguments,
                None,
                &[],
                &[0, 1],
                CancellationToken::new(),
            )
            .await?
            .status
            .success();
        let arguments = if remote {
            let local_name = name.split_once('/').map_or(name, |(_, local)| local);
            let local_ref = format!("refs/heads/{local_name}");
            let local_check = [
                "show-ref".into(),
                "--verify".into(),
                "--quiet".into(),
                local_ref.into(),
            ];
            let local_exists = self
                .run(
                    &root,
                    &local_check,
                    None,
                    &[],
                    &[0, 1],
                    CancellationToken::new(),
                )
                .await?
                .status
                .success();
            if local_exists {
                vec!["switch".into(), "--".into(), local_name.into()]
            } else {
                vec!["switch".into(), "--track".into(), name.into()]
            }
        } else {
            vec!["switch".into(), "--".into(), name.into()]
        };
        self.run_default(&root, &arguments).await?;
        Ok(())
    }

    async fn rename_branch(&self, workspace: &WorkspaceRecord, name: &str) -> GitResult<()> {
        let root = validated_root(workspace)?;
        self.validate_branch_name(&root, name).await?;
        let arguments = ["branch".into(), "-m".into(), name.into()];
        self.run_default(&root, &arguments).await?;
        Ok(())
    }

    async fn delete_branch(
        &self,
        workspace: &WorkspaceRecord,
        name: &str,
        force: bool,
    ) -> GitResult<()> {
        let root = validated_root(workspace)?;
        self.validate_branch_name(&root, name).await?;
        let arguments = [
            "branch".into(),
            if force { "-D".into() } else { "-d".into() },
            "--".into(),
            name.into(),
        ];
        self.run_default(&root, &arguments).await?;
        Ok(())
    }

    async fn remotes(&self, workspace: &WorkspaceRecord) -> GitResult<Vec<RemoteInfo>> {
        let root = validated_root(workspace)?;
        let output = self.run_default(&root, &["remote".into()]).await?;
        let mut remotes = Vec::new();
        for name in parse_utf8(&output.stdout)?
            .lines()
            .filter(|name| !name.is_empty())
        {
            let fetch = self
                .run_default(
                    &root,
                    &["remote".into(), "get-url".into(), "--".into(), name.into()],
                )
                .await?;
            let push = self
                .run_default(
                    &root,
                    &[
                        "remote".into(),
                        "get-url".into(),
                        "--push".into(),
                        "--".into(),
                        name.into(),
                    ],
                )
                .await?;
            remotes.push(RemoteInfo {
                name: name.to_owned(),
                fetch_url: parse_utf8(&fetch.stdout)?.trim_end().to_owned(),
                push_url: parse_utf8(&push.stdout)?.trim_end().to_owned(),
            });
        }
        Ok(remotes)
    }

    async fn add_remote(
        &self,
        workspace: &WorkspaceRecord,
        request: RemoteRequest,
    ) -> GitResult<()> {
        validate_remote_request(&request)?;
        let root = validated_root(workspace)?;
        self.validate_branch_name(&root, &request.name).await?;
        self.run_default(
            &root,
            &[
                "remote".into(),
                "add".into(),
                "--".into(),
                request.name.clone().into(),
                request.fetch_url.clone().into(),
            ],
        )
        .await?;
        self.set_remote_push_url(&root, &request.name, &request)
            .await
    }

    async fn update_remote(
        &self,
        workspace: &WorkspaceRecord,
        previous_name: &str,
        request: RemoteRequest,
    ) -> GitResult<()> {
        validate_remote_request(&request)?;
        let root = validated_root(workspace)?;
        self.validate_branch_name(&root, previous_name).await?;
        self.validate_branch_name(&root, &request.name).await?;
        if previous_name != request.name {
            self.run_default(
                &root,
                &[
                    "remote".into(),
                    "rename".into(),
                    "--".into(),
                    previous_name.into(),
                    request.name.clone().into(),
                ],
            )
            .await?;
        }
        self.run_default(
            &root,
            &[
                "remote".into(),
                "set-url".into(),
                "--".into(),
                request.name.clone().into(),
                request.fetch_url.clone().into(),
            ],
        )
        .await?;
        self.set_remote_push_url(&root, &request.name, &request)
            .await
    }

    async fn remove_remote(&self, workspace: &WorkspaceRecord, name: &str) -> GitResult<()> {
        let root = validated_root(workspace)?;
        self.validate_branch_name(&root, name).await?;
        self.run_default(
            &root,
            &["remote".into(), "remove".into(), "--".into(), name.into()],
        )
        .await?;
        Ok(())
    }

    async fn fetch_remote(
        &self,
        workspace: &WorkspaceRecord,
        name: &str,
    ) -> GitResult<RemoteResult> {
        let root = validated_root(workspace)?;
        self.validate_branch_name(&root, name).await?;
        self.run(
            &root,
            &["fetch".into(), "--prune".into(), "--".into(), name.into()],
            None,
            &[("GIT_TERMINAL_PROMPT", "0".into())],
            &[0],
            CancellationToken::new(),
        )
        .await?;
        Ok(RemoteResult {
            message: format!("Fetched {name}."),
        })
    }

    async fn tags(&self, workspace: &WorkspaceRecord) -> GitResult<Vec<TagInfo>> {
        self.list_tags(workspace).await
    }

    async fn create_tag(&self, workspace: &WorkspaceRecord, request: TagRequest) -> GitResult<()> {
        self.create_repository_tag(workspace, request).await
    }

    async fn delete_tag(&self, workspace: &WorkspaceRecord, name: &str) -> GitResult<()> {
        self.delete_repository_tag(workspace, name).await
    }

    async fn compare(
        &self,
        workspace: &WorkspaceRecord,
        base: &str,
        head: &str,
    ) -> GitResult<BranchComparison> {
        validate_revision(base)?;
        validate_revision(head)?;
        let root = validated_root(workspace)?;
        self.require_commit(&root, base).await?;
        self.require_commit(&root, head).await?;

        let range = format!("{base}...{head}");
        let count_arguments = [
            "rev-list".into(),
            "--left-right".into(),
            "--count".into(),
            range.clone().into(),
        ];
        let counts = self.run_default(&root, &count_arguments).await?;
        let (base_only_commits, head_only_commits) = parse_comparison_counts(&counts.stdout)?;
        let log_arguments = [
            "log".into(),
            "-z".into(),
            "--no-show-signature".into(),
            "--format=%H%x1f%h%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%s".into(),
            "-n200".into(),
            format!("{base}..{head}").into(),
        ];
        let commits = parse_history(&self.run_default(&root, &log_arguments).await?.stdout)?;
        let patch_arguments = [
            "diff".into(),
            "--no-ext-diff".into(),
            "--no-color".into(),
            "--binary".into(),
            "--unified=3".into(),
            range.clone().into(),
            "--".into(),
        ];
        let patch = String::from_utf8(self.run_default(&root, &patch_arguments).await?.stdout)
            .map_err(|_| parse_error())?;
        let stats_arguments = ["diff".into(), "--numstat".into(), range.into(), "--".into()];
        let stats = self.run_default(&root, &stats_arguments).await?;
        let (files_changed, additions, deletions) = parse_numstat(&stats.stdout)?;
        Ok(BranchComparison {
            base: base.to_owned(),
            head: head.to_owned(),
            base_only_commits,
            head_only_commits,
            commits,
            patch,
            files_changed,
            additions,
            deletions,
        })
    }

    async fn merge(&self, workspace: &WorkspaceRecord, branch: &str) -> GitResult<MergeOutcome> {
        let root = validated_root(workspace)?;
        self.validate_branch_name(&root, branch).await?;
        self.require_commit(&root, branch).await?;
        let arguments = [
            "merge".into(),
            "--no-edit".into(),
            "--".into(),
            branch.into(),
        ];
        match self.run_default(&root, &arguments).await {
            Ok(_) => Ok(MergeOutcome::Merged {
                message: format!("Merged {branch}."),
            }),
            Err(error) => {
                let status = self.status(workspace).await?;
                let paths = status
                    .changes
                    .into_iter()
                    .filter(|change| change.conflicted)
                    .map(|change| change.path)
                    .collect::<Vec<_>>();
                if paths.is_empty() {
                    Err(error)
                } else {
                    Ok(MergeOutcome::Conflicts { paths })
                }
            }
        }
    }

    async fn abort_merge(&self, workspace: &WorkspaceRecord) -> GitResult<()> {
        let root = validated_root(workspace)?;
        let arguments = ["merge".into(), "--abort".into()];
        self.run_default(&root, &arguments).await?;
        Ok(())
    }

    async fn conflict_file(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> GitResult<ConflictFile> {
        require_conflicted_path(self, workspace, path).await?;
        let file = HostWorkspaceFiles
            .read_text(workspace, path, MAX_CONFLICT_FILE_BYTES)
            .await
            .map_err(map_workspace_error)?;
        parse_conflict_file(path.clone(), &file.content)
    }

    async fn resolve_conflict(
        &self,
        workspace: &WorkspaceRecord,
        request: ConflictRequest,
    ) -> GitResult<bool> {
        require_conflicted_path(self, workspace, &request.path).await?;
        let files = HostWorkspaceFiles;
        let file = files
            .read_text(workspace, &request.path, MAX_CONFLICT_FILE_BYTES)
            .await
            .map_err(map_workspace_error)?;
        let resolved = resolve_conflict_block(
            &file.content,
            request.block_index,
            request.expected_fingerprint,
            request.choice,
        )?;
        files
            .write_text(
                workspace,
                &request.path,
                &resolved.content,
                Some(&file.version),
                MAX_CONFLICT_FILE_BYTES,
            )
            .await
            .map_err(map_workspace_error)?;
        if resolved.complete {
            self.stage(workspace, std::slice::from_ref(&request.path))
                .await?;
        }
        Ok(resolved.complete)
    }

    async fn history(&self, workspace: &WorkspaceRecord, limit: u32) -> GitResult<Vec<CommitInfo>> {
        self.load_history(workspace, limit).await
    }

    async fn commit_message(
        &self,
        workspace: &WorkspaceRecord,
        revision: &str,
    ) -> GitResult<String> {
        self.load_commit_message(workspace, revision).await
    }

    async fn commit_detail(
        &self,
        workspace: &WorkspaceRecord,
        revision: &str,
    ) -> GitResult<CommitDetail> {
        self.load_commit_detail(workspace, revision).await
    }

    async fn checkout_commit(&self, workspace: &WorkspaceRecord, revision: &str) -> GitResult<()> {
        validate_revision(revision)?;
        let root = validated_root(workspace)?;
        self.require_commit(&root, revision).await?;
        let arguments = ["switch".into(), "--detach".into(), revision.into()];
        self.run_default(&root, &arguments).await?;
        Ok(())
    }

    async fn revert_commit(&self, workspace: &WorkspaceRecord, revision: &str) -> GitResult<()> {
        validate_revision(revision)?;
        let root = validated_root(workspace)?;
        self.require_commit(&root, revision).await?;
        let arguments = [
            "revert".into(),
            "--no-edit".into(),
            "--".into(),
            revision.into(),
        ];
        match self.run_default(&root, &arguments).await {
            Ok(_) => Ok(()),
            Err(error) => {
                if self
                    .status(workspace)
                    .await
                    .is_ok_and(|status| status.conflict_count() > 0)
                {
                    Err(GitError::new(
                        GitErrorCode::Conflict,
                        "The revert stopped on conflicts. Resolve them before continuing.",
                    ))
                } else {
                    Err(error)
                }
            }
        }
    }

    async fn fetch(&self, workspace: &WorkspaceRecord) -> GitResult<RemoteResult> {
        self.fetch_all(workspace).await
    }

    async fn pull(&self, workspace: &WorkspaceRecord) -> GitResult<RemoteResult> {
        self.pull_fast_forward(workspace).await
    }

    async fn publish_branch(
        &self,
        workspace: &WorkspaceRecord,
        remote: &str,
    ) -> GitResult<RemoteResult> {
        self.publish_current_branch(workspace, remote).await
    }

    async fn push(
        &self,
        workspace: &WorkspaceRecord,
        force_with_lease: bool,
    ) -> GitResult<PushOutcome> {
        self.push_upstream(workspace, force_with_lease).await
    }
}

impl HostGit {
    /// Returns a Git patch with the requested number of unchanged context lines.
    ///
    /// # Errors
    ///
    /// Returns a structured Git error when the workspace, path, or diff output is invalid.
    pub async fn diff_with_context(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        kind: DiffKind,
        context_lines: u32,
    ) -> GitResult<UnifiedDiff> {
        repository_diff_with_context(self, workspace, path, kind, context_lines).await
    }

    /// Clones a repository while reporting recognized Git progress updates.
    ///
    /// # Errors
    ///
    /// Returns a structured Git error when validation, cloning, cancellation,
    /// destination verification, or output limits fail.
    pub async fn clone_repository_with_progress(
        &self,
        request: CloneRequest,
        cancellation: CancellationToken,
        progress: mpsc::Sender<CloneProgress>,
    ) -> GitResult<CloneResult> {
        validate_clone_url(&request.url)?;
        let parent = canonical_clone_parent(&request.destination_parent)?;
        let directory_name = request
            .directory_name
            .map_or_else(|| clone_directory_name(&request.url), Ok)?;
        validate_clone_directory_name(&directory_name)?;
        let destination = parent.join(&directory_name);
        if destination.exists() {
            return Err(GitError::new(
                GitErrorCode::Conflict,
                "A file or directory already exists at the clone destination.",
            ));
        }
        let _ = progress.try_send(CloneProgress {
            phase: ClonePhase::Preparing,
            percent: None,
        });
        let mut arguments: Vec<OsString> = vec!["clone".into(), "--progress".into()];
        match request.mode {
            CloneMode::Full => {}
            CloneMode::Blobless => arguments.push("--filter=blob:none".into()),
            CloneMode::Shallow => arguments.extend(["--depth".into(), "1".into()]),
        }
        arguments.extend(["--".into(), request.url.into(), directory_name.into()]);
        let mut clone_runner = self.clone();
        clone_runner.config.timeout = clone_runner.config.clone_timeout;
        if let Err(error) = clone_runner
            .run_with_progress(
                &parent,
                &arguments,
                &[("GIT_TERMINAL_PROMPT", "0".into())],
                cancellation,
                &progress,
            )
            .await
        {
            cleanup_clone_destination(&destination);
            return Err(error);
        }
        let _ = progress.try_send(CloneProgress {
            phase: ClonePhase::Finalizing,
            percent: None,
        });
        let canonical = destination.canonicalize().map_err(|_| {
            cleanup_clone_destination(&destination);
            internal_error()
        })?;
        if !canonical.starts_with(&parent) || !canonical.is_dir() {
            cleanup_clone_destination(&destination);
            return Err(GitError::new(
                GitErrorCode::InvalidWorkspace,
                "The cloned repository resolved outside the selected destination.",
            ));
        }
        Ok(CloneResult {
            absolute_path: canonical.to_string_lossy().into_owned(),
        })
    }

    async fn run_default(
        &self,
        root: &Path,
        arguments: &[OsString],
    ) -> GitResult<crate::runner::GitOutput> {
        self.run(root, arguments, None, &[], &[0], CancellationToken::new())
            .await
    }

    async fn set_remote_push_url(
        &self,
        root: &Path,
        name: &str,
        request: &RemoteRequest,
    ) -> GitResult<()> {
        let push_url = request
            .push_url
            .as_deref()
            .filter(|url| !url.trim().is_empty())
            .unwrap_or(&request.fetch_url);
        self.run_default(
            root,
            &[
                "remote".into(),
                "set-url".into(),
                "--push".into(),
                "--".into(),
                name.into(),
                push_url.into(),
            ],
        )
        .await?;
        Ok(())
    }

    async fn validate_branch_name(&self, root: &Path, name: &str) -> GitResult<()> {
        validate_revision(name)?;
        let arguments = ["check-ref-format".into(), "--branch".into(), name.into()];
        self.run_default(root, &arguments).await.map_err(|error| {
            if error.code == GitErrorCode::CommandFailed {
                GitError::new(GitErrorCode::Conflict, "Enter a valid Git branch name.")
            } else {
                error
            }
        })?;
        Ok(())
    }

    async fn commit_info(&self, root: &Path, revision: &str) -> GitResult<CommitInfo> {
        let arguments = [
            "show".into(),
            "-s".into(),
            "--no-show-signature".into(),
            "--format=%H%x1f%h%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%s".into(),
            revision.into(),
        ];
        let output = self.run_default(root, &arguments).await?;
        parse_commit_record(trim_ascii_end(&output.stdout))
    }

    async fn require_commit(&self, root: &Path, revision: &str) -> GitResult<()> {
        let arguments = [
            "rev-parse".into(),
            "--verify".into(),
            "--quiet".into(),
            format!("{revision}^{{commit}}").into(),
        ];
        self.run(
            root,
            &arguments,
            None,
            &[],
            &[0, 1],
            CancellationToken::new(),
        )
        .await
        .and_then(|output| {
            if output.status.success() {
                Ok(())
            } else {
                Err(GitError::new(
                    GitErrorCode::Conflict,
                    "The selected commit no longer exists.",
                ))
            }
        })
    }

    async fn run_paths(
        &self,
        workspace: &WorkspaceRecord,
        command: &[&str],
        paths: &[RelativePath],
    ) -> GitResult<()> {
        require_paths(paths)?;
        let root = validated_root(workspace)?;
        let mut arguments = command.iter().map(OsString::from).collect::<Vec<_>>();
        arguments.push("--".into());
        arguments.extend(paths.iter().map(|path| path.as_str().into()));
        self.run(&root, &arguments, None, &[], &[0], CancellationToken::new())
            .await?;
        Ok(())
    }

    async fn git_config(&self, root: &Path, name: &str) -> GitResult<Option<String>> {
        let arguments = ["config".into(), "--get".into(), name.into()];
        let output = self
            .run(
                root,
                &arguments,
                None,
                &[],
                &[0, 1],
                CancellationToken::new(),
            )
            .await?;
        if output.status.code() == Some(1) {
            return Ok(None);
        }
        String::from_utf8(output.stdout)
            .map(|value| Some(value.trim().to_owned()))
            .map_err(|_| GitError::new(GitErrorCode::Parse, "Git configuration is not UTF-8."))
    }

    async fn rev_parse_head(&self, root: &Path) -> GitResult<String> {
        let arguments = ["rev-parse".into(), "--verify".into(), "HEAD".into()];
        let output = self
            .run(root, &arguments, None, &[], &[0], CancellationToken::new())
            .await?;
        String::from_utf8(output.stdout)
            .map(|value| value.trim().to_owned())
            .map_err(|_| GitError::new(GitErrorCode::Parse, "Git returned an invalid commit ID."))
    }
}

async fn require_conflicted_path(
    host: &HostGit,
    workspace: &WorkspaceRecord,
    path: &RelativePath,
) -> GitResult<()> {
    if host
        .status(workspace)
        .await?
        .changes
        .iter()
        .any(|change| change.conflicted && change.path == *path)
    {
        Ok(())
    } else {
        Err(GitError::new(
            GitErrorCode::Conflict,
            "The selected file is no longer conflicted.",
        ))
    }
}

fn map_workspace_error(error: syntaxis_workspace::WorkspaceError) -> GitError {
    let code = match error.code {
        WorkspaceErrorCode::InvalidPath
        | WorkspaceErrorCode::OutsideAllowedRoot
        | WorkspaceErrorCode::RootOperationRejected => GitErrorCode::InvalidWorkspace,
        WorkspaceErrorCode::NotFound | WorkspaceErrorCode::Conflict => GitErrorCode::Conflict,
        WorkspaceErrorCode::TooLarge => GitErrorCode::OutputTooLarge,
        WorkspaceErrorCode::UnsupportedEncoding => GitErrorCode::Unsupported,
        WorkspaceErrorCode::PermissionDenied
        | WorkspaceErrorCode::Unavailable
        | WorkspaceErrorCode::AlreadyExists => GitErrorCode::Unavailable,
        WorkspaceErrorCode::Internal => GitErrorCode::Internal,
    };
    GitError::new(code, error.message)
}

fn require_paths(paths: &[RelativePath]) -> GitResult<()> {
    if paths.is_empty() || paths.iter().any(RelativePath::is_root) {
        Err(GitError::new(
            GitErrorCode::InvalidWorkspace,
            "At least one workspace-relative file path is required.",
        ))
    } else {
        Ok(())
    }
}

fn cleanup_clone_destination(destination: &Path) {
    let Ok(metadata) = std::fs::symlink_metadata(destination) else {
        return;
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        let _ = std::fs::remove_dir_all(destination);
    } else {
        let _ = std::fs::remove_file(destination);
    }
}

fn config_enabled(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "true" | "yes" | "on" | "1"
    )
}

async fn repository_diff_with_context(
    host: &HostGit,
    workspace: &WorkspaceRecord,
    path: &RelativePath,
    kind: DiffKind,
    context_lines: u32,
) -> GitResult<UnifiedDiff> {
    let root = validated_root(workspace)?;
    let untracked = kind == DiffKind::Worktree
        && host
            .status_with_cancellation(workspace, CancellationToken::new())
            .await?
            .changes
            .iter()
            .any(|change| {
                change.path == *path && change.worktree == Some(syntaxis_git::ChangeKind::Untracked)
            });
    let context = format!("--unified={context_lines}");
    let arguments = if untracked {
        vec![
            "diff".into(),
            "--no-index".into(),
            "--no-ext-diff".into(),
            "--no-color".into(),
            "--binary".into(),
            context.into(),
            "--".into(),
            "/dev/null".into(),
            path.as_str().into(),
        ]
    } else {
        let mut arguments = vec![
            "diff".into(),
            "--no-ext-diff".into(),
            "--no-color".into(),
            "--binary".into(),
            context.into(),
        ];
        if kind == DiffKind::Staged {
            arguments.push("--cached".into());
        }
        arguments.extend(["--".into(), path.as_str().into()]);
        arguments
    };
    let output = host
        .run(
            &root,
            &arguments,
            None,
            &[],
            if untracked { &[0, 1] } else { &[0] },
            CancellationToken::new(),
        )
        .await?;
    let diff_text = String::from_utf8(output.stdout).map_err(|_| {
        GitError::new(
            GitErrorCode::Parse,
            "Git returned a diff that could not be displayed as text.",
        )
    })?;
    let binary = diff_text.contains("GIT binary patch") || diff_text.contains("Binary files ");
    let original = if binary {
        None
    } else {
        original_diff_contents(host, &root, path, kind, untracked).await?
    };
    let current = if binary {
        None
    } else {
        current_diff_contents(host, workspace, &root, path, kind).await?
    };
    Ok(UnifiedDiff {
        path: path.clone(),
        kind,
        patch: diff_text,
        binary,
        original,
        current,
    })
}

async fn original_diff_contents(
    host: &HostGit,
    root: &Path,
    path: &RelativePath,
    kind: DiffKind,
    untracked: bool,
) -> GitResult<Option<String>> {
    if untracked {
        return Ok(Some(String::new()));
    }
    let revision = match kind {
        DiffKind::Worktree => format!(":{}", path.as_str()),
        DiffKind::Staged => format!("HEAD:{}", path.as_str()),
    };
    git_revision_contents(host, root, revision).await
}

async fn current_diff_contents(
    host: &HostGit,
    workspace: &WorkspaceRecord,
    root: &Path,
    path: &RelativePath,
    kind: DiffKind,
) -> GitResult<Option<String>> {
    if kind == DiffKind::Staged {
        return git_revision_contents(host, root, format!(":{}", path.as_str())).await;
    }
    match HostWorkspaceFiles
        .read_text(workspace, path, MAX_CONFLICT_FILE_BYTES)
        .await
    {
        Ok(file) => Ok(Some(file.content)),
        Err(error) if error.code == WorkspaceErrorCode::NotFound => Ok(Some(String::new())),
        Err(_) => Ok(None),
    }
}

async fn git_revision_contents(
    host: &HostGit,
    root: &Path,
    revision: String,
) -> GitResult<Option<String>> {
    let arguments = [
        "show".into(),
        "--no-color".into(),
        "--no-textconv".into(),
        "--format=".into(),
        revision.into(),
    ];
    let output = host
        .run(
            root,
            &arguments,
            None,
            &[],
            &[0, 128],
            CancellationToken::new(),
        )
        .await?;
    if !output.status.success() {
        return Ok(Some(String::new()));
    }
    Ok(String::from_utf8(output.stdout).ok())
}

fn internal_error() -> GitError {
    GitError::new(
        GitErrorCode::Internal,
        "The Git operation could not be completed.",
    )
}

#[cfg(test)]
mod tests;
