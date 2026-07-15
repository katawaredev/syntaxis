use std::{ffi::OsString, path::Path};

use async_trait::async_trait;
use syntaxis_git::{
    parse_conflict_file, parse_diff_hunks, resolve_conflict_block, BranchComparison, BranchInfo,
    BranchRequest, ClonePhase, CloneProgress, CloneRequest, CloneResult, CommitDetail, CommitInfo,
    CommitOutcome, CommitRequest, CommitResult, ConflictFile, ConflictRequest, DiffKind, GitError,
    GitErrorCode, GitOperations, GitResult, HunkAction, HunkRequest, MergeOutcome, PushOutcome,
    RemoteInfo, RemoteRequest, RemoteResult, RepositoryStatus, TagInfo, TagRequest, UnifiedDiff,
};
use syntaxis_workspace::{
    ErrorCode as WorkspaceErrorCode, RelativePath, WorkspaceFiles, WorkspaceRecord,
};
use syntaxis_workspace_host::HostWorkspaceFiles;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::runner::{validated_root, HostGit};

const MAX_COMMIT_MESSAGE_BYTES: usize = 256 * 1024;
const MAX_TAG_MESSAGE_BYTES: usize = 256 * 1024;
const MAX_CONFLICT_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PASSPHRASE_BYTES: usize = 16 * 1024;
const MAX_REMOTE_URL_BYTES: usize = 64 * 1024;

#[async_trait(?Send)]
impl GitOperations for HostGit {
    async fn clone_repository(&self, request: CloneRequest) -> GitResult<CloneResult> {
        let (progress, _progress_receiver) = mpsc::channel(1);
        self.clone_repository_with_progress(request, CancellationToken::new(), progress)
            .await
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
        let root = validated_root(workspace)?;
        let arguments = [
            "for-each-ref".into(),
            "--sort=-creatordate".into(),
            "--format=%(refname:short)%00%(objecttype)%00%(objectname)%00%(*objectname)".into(),
            "refs/tags".into(),
        ];
        let output = self.run_default(&root, &arguments).await?;
        parse_tags(&output.stdout)
    }

    async fn create_tag(&self, workspace: &WorkspaceRecord, request: TagRequest) -> GitResult<()> {
        let root = validated_root(workspace)?;
        self.validate_tag_name(&root, &request.name).await?;
        if request
            .message
            .as_ref()
            .is_some_and(|message| message.len() > MAX_TAG_MESSAGE_BYTES)
        {
            return Err(GitError::new(
                GitErrorCode::OutputTooLarge,
                "The tag message is too large.",
            ));
        }
        if let Some(target) = request.target.as_deref() {
            validate_revision(target)?;
        }
        let mut arguments = vec!["tag".into()];
        if let Some(message) = request.message.filter(|message| !message.trim().is_empty()) {
            arguments.extend([
                "-a".into(),
                request.name.into(),
                "-m".into(),
                message.into(),
            ]);
        } else {
            arguments.push(request.name.into());
        }
        if let Some(target) = request.target {
            arguments.push(target.into());
        }
        self.run_default(&root, &arguments).await?;
        Ok(())
    }

    async fn delete_tag(&self, workspace: &WorkspaceRecord, name: &str) -> GitResult<()> {
        let root = validated_root(workspace)?;
        self.validate_tag_name(&root, name).await?;
        let arguments = ["tag".into(), "-d".into(), "--".into(), name.into()];
        self.run_default(&root, &arguments).await?;
        Ok(())
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
        let root = validated_root(workspace)?;
        let limit = limit.clamp(1, 200);
        let arguments = [
            "log".into(),
            "-z".into(),
            "--no-show-signature".into(),
            "--format=%H%x1f%h%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%s".into(),
            format!("-n{limit}").into(),
        ];
        let output = self.run_default(&root, &arguments).await?;
        parse_history(&output.stdout)
    }

    async fn commit_message(
        &self,
        workspace: &WorkspaceRecord,
        revision: &str,
    ) -> GitResult<String> {
        validate_revision(revision)?;
        let root = validated_root(workspace)?;
        self.require_commit(&root, revision).await?;
        let arguments = [
            "show".into(),
            "-s".into(),
            "--no-show-signature".into(),
            "--format=%B".into(),
            revision.into(),
        ];
        let output = self.run_default(&root, &arguments).await?;
        String::from_utf8(trim_ascii_end(&output.stdout).to_vec()).map_err(|_| parse_error())
    }

    async fn commit_detail(
        &self,
        workspace: &WorkspaceRecord,
        revision: &str,
    ) -> GitResult<CommitDetail> {
        validate_revision(revision)?;
        let root = validated_root(workspace)?;
        let commit = self.commit_info(&root, revision).await?;
        let patch_arguments = [
            "show".into(),
            "--format=".into(),
            "--no-ext-diff".into(),
            "--no-color".into(),
            "--binary".into(),
            "--unified=3".into(),
            revision.into(),
        ];
        let patch_output = self.run_default(&root, &patch_arguments).await?;
        let patch = String::from_utf8(patch_output.stdout).map_err(|_| parse_error())?;
        let stats_arguments = [
            "show".into(),
            "--format=".into(),
            "--numstat".into(),
            revision.into(),
        ];
        let stats_output = self.run_default(&root, &stats_arguments).await?;
        let (files_changed, additions, deletions) = parse_numstat(&stats_output.stdout)?;
        Ok(CommitDetail {
            commit,
            patch,
            files_changed,
            additions,
            deletions,
        })
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
        let root = validated_root(workspace)?;
        let arguments = ["fetch".into(), "--prune".into()];
        self.run(
            &root,
            &arguments,
            None,
            &[("GIT_TERMINAL_PROMPT", "0".into())],
            &[0],
            CancellationToken::new(),
        )
        .await?;
        Ok(RemoteResult {
            message: "Fetch completed.".into(),
        })
    }

    async fn push(
        &self,
        workspace: &WorkspaceRecord,
        force_with_lease: bool,
    ) -> GitResult<PushOutcome> {
        let root = validated_root(workspace)?;
        let mut arguments = vec!["push".into()];
        if force_with_lease {
            arguments.push("--force-with-lease".into());
        }
        let result = self
            .run(
                &root,
                &arguments,
                None,
                &[("GIT_TERMINAL_PROMPT", "0".into())],
                &[0],
                CancellationToken::new(),
            )
            .await;
        match result {
            Ok(_) => Ok(PushOutcome::Pushed {
                result: RemoteResult {
                    message: if force_with_lease {
                        "Force-with-lease push completed."
                    } else {
                        "Push completed."
                    }
                    .into(),
                },
            }),
            Err(error) if error.code == GitErrorCode::NonFastForward && !force_with_lease => {
                Ok(PushOutcome::ForceWithLeaseRequired {
                    message: error.message,
                })
            }
            Err(error) => Err(error),
        }
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
        let arguments = [
            "clone".into(),
            "--progress".into(),
            "--".into(),
            request.url.into(),
            directory_name.into(),
        ];
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

    async fn validate_tag_name(&self, root: &Path, name: &str) -> GitResult<()> {
        validate_revision(name)?;
        let arguments = [
            "check-ref-format".into(),
            format!("refs/tags/{name}").into(),
        ];
        self.run_default(root, &arguments).await.map_err(|error| {
            if error.code == GitErrorCode::CommandFailed {
                GitError::new(GitErrorCode::Conflict, "Enter a valid Git tag name.")
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

fn validate_remote_request(request: &RemoteRequest) -> GitResult<()> {
    if request.name.trim().is_empty() {
        return Err(GitError::new(
            GitErrorCode::Conflict,
            "Remote name cannot be empty.",
        ));
    }
    if request.fetch_url.trim().is_empty() {
        return Err(GitError::new(
            GitErrorCode::Conflict,
            "Remote fetch URL cannot be empty.",
        ));
    }
    if request.fetch_url.len() > MAX_REMOTE_URL_BYTES
        || request
            .push_url
            .as_ref()
            .is_some_and(|url| url.len() > MAX_REMOTE_URL_BYTES)
    {
        return Err(GitError::new(
            GitErrorCode::OutputTooLarge,
            "Remote URL is too large.",
        ));
    }
    Ok(())
}

fn validate_commit_request(request: &CommitRequest) -> GitResult<()> {
    if request.message.trim().is_empty() {
        return Err(GitError::new(
            GitErrorCode::Conflict,
            "Enter a commit message.",
        ));
    }
    if request.message.len() > MAX_COMMIT_MESSAGE_BYTES {
        return Err(GitError::new(
            GitErrorCode::OutputTooLarge,
            "The commit message is too large.",
        ));
    }
    if request
        .signing_passphrase
        .as_ref()
        .is_some_and(|passphrase| passphrase.len() > MAX_PASSPHRASE_BYTES)
    {
        return Err(GitError::new(
            GitErrorCode::OutputTooLarge,
            "The signing passphrase is too large.",
        ));
    }
    Ok(())
}

fn config_enabled(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "true" | "yes" | "on" | "1"
    )
}

fn parse_branches(output: &[u8]) -> GitResult<Vec<BranchInfo>> {
    let branches = output
        .split(|byte| *byte == b'\n')
        .filter(|record| !record.is_empty())
        .map(|record| {
            let fields = record.split(|byte| *byte == 0).collect::<Vec<_>>();
            if fields.len() != 4 {
                return Err(parse_error());
            }
            Ok(BranchInfo {
                name: parse_utf8(fields[0])?.to_owned(),
                current: fields[1] == b"*",
                upstream: match parse_utf8(fields[2])? {
                    "" => None,
                    upstream => Some(upstream.to_owned()),
                },
                remote: fields[3].starts_with(b"refs/remotes/"),
            })
        })
        .collect::<GitResult<Vec<_>>>()?;
    Ok(branches
        .into_iter()
        .filter(|branch| !branch.remote || branch.name.contains('/'))
        .collect())
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
    Ok(UnifiedDiff {
        path: path.clone(),
        kind,
        patch: diff_text,
        binary,
    })
}

fn parse_path_numstat(output: &[u8]) -> GitResult<Vec<(RelativePath, u64, u64)>> {
    let records = output.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut stats = Vec::new();
    let mut index = 0;
    while index < records.len() {
        let record = records[index];
        index += 1;
        if record.is_empty() {
            continue;
        }
        let mut fields = record.splitn(3, |byte| *byte == b'\t');
        let additions = fields.next().ok_or_else(parse_error)?;
        let deletions = fields.next().ok_or_else(parse_error)?;
        let inline_path = fields.next().ok_or_else(parse_error)?;
        let path = if inline_path.is_empty() {
            index += 1;
            let renamed_path = records.get(index).ok_or_else(parse_error)?;
            index += 1;
            *renamed_path
        } else {
            inline_path
        };
        let additions = if additions == b"-" {
            0
        } else {
            parse_utf8(additions)?.parse().map_err(|_| parse_error())?
        };
        let deletions = if deletions == b"-" {
            0
        } else {
            parse_utf8(deletions)?.parse().map_err(|_| parse_error())?
        };
        stats.push((
            RelativePath::try_from(parse_utf8(path)?).map_err(|_| parse_error())?,
            additions,
            deletions,
        ));
    }
    Ok(stats)
}

fn apply_path_stats(
    status: &mut RepositoryStatus,
    path_stats: &[(RelativePath, u64, u64)],
    staged: bool,
) {
    for (path, additions, deletions) in path_stats {
        let Some(change) = status
            .changes
            .iter_mut()
            .find(|change| change.path == *path || change.original_path.as_ref() == Some(path))
        else {
            continue;
        };
        if staged {
            change.staged_additions = change.staged_additions.saturating_add(*additions);
            change.staged_deletions = change.staged_deletions.saturating_add(*deletions);
        } else {
            change.unstaged_additions = change.unstaged_additions.saturating_add(*additions);
            change.unstaged_deletions = change.unstaged_deletions.saturating_add(*deletions);
        }
    }
}

fn apply_untracked_stats(root: &Path, status: &mut RepositoryStatus, max_bytes: usize) {
    for change in &mut status.changes {
        if change.worktree != Some(syntaxis_git::ChangeKind::Untracked) {
            continue;
        }
        let path = root.join(change.path.as_str());
        let Ok(metadata) = path.symlink_metadata() else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > max_bytes as u64 {
            continue;
        }
        let Ok(contents) = std::fs::read(path) else {
            continue;
        };
        if contents.contains(&0) {
            continue;
        }
        let lines = contents
            .split(|byte| *byte == b'\n')
            .count()
            .saturating_sub(usize::from(contents.last() == Some(&b'\n')));
        change.unstaged_additions = lines.try_into().unwrap_or(u64::MAX);
    }
}

fn parse_tags(output: &[u8]) -> GitResult<Vec<TagInfo>> {
    output
        .split(|byte| *byte == b'\n')
        .filter(|record| !record.is_empty())
        .map(|record| {
            let fields = record.split(|byte| *byte == 0).collect::<Vec<_>>();
            if fields.len() != 4 {
                return Err(parse_error());
            }
            let annotated = fields[1] == b"tag";
            let target = if annotated { fields[3] } else { fields[2] };
            if target.is_empty() {
                return Err(parse_error());
            }
            Ok(TagInfo {
                name: parse_utf8(fields[0])?.to_owned(),
                target_oid: parse_utf8(target)?.to_owned(),
                annotated,
            })
        })
        .collect()
}

fn parse_history(output: &[u8]) -> GitResult<Vec<CommitInfo>> {
    output
        .split(|byte| *byte == 0)
        .map(trim_ascii_end)
        .filter(|record| !record.is_empty())
        .map(parse_commit_record)
        .collect()
}

fn parse_commit_record(record: &[u8]) -> GitResult<CommitInfo> {
    let fields = record.split(|byte| *byte == 0x1f).collect::<Vec<_>>();
    if fields.len() != 7 {
        return Err(parse_error());
    }
    Ok(CommitInfo {
        oid: parse_utf8(fields[0])?.to_owned(),
        short_oid: parse_utf8(fields[1])?.to_owned(),
        parents: parse_utf8(fields[2])?
            .split_ascii_whitespace()
            .map(ToOwned::to_owned)
            .collect(),
        author_name: parse_utf8(fields[3])?.to_owned(),
        author_email: parse_utf8(fields[4])?.to_owned(),
        authored_unix_seconds: parse_utf8(fields[5])?.parse().map_err(|_| parse_error())?,
        subject: parse_utf8(fields[6])?.to_owned(),
    })
}

fn parse_numstat(output: &[u8]) -> GitResult<(u32, u64, u64)> {
    let text = parse_utf8(output)?;
    let mut files = 0_u32;
    let mut additions = 0_u64;
    let mut deletions = 0_u64;
    for line in text.lines().filter(|line| !line.is_empty()) {
        let mut fields = line.splitn(3, '\t');
        let added = fields.next().ok_or_else(parse_error)?;
        let deleted = fields.next().ok_or_else(parse_error)?;
        fields.next().ok_or_else(parse_error)?;
        files = files.saturating_add(1);
        if added != "-" {
            additions = additions.saturating_add(added.parse().map_err(|_| parse_error())?);
        }
        if deleted != "-" {
            deletions = deletions.saturating_add(deleted.parse().map_err(|_| parse_error())?);
        }
    }
    Ok((files, additions, deletions))
}

fn parse_comparison_counts(output: &[u8]) -> GitResult<(u32, u32)> {
    let mut fields = parse_utf8(output)?.split_ascii_whitespace();
    let base_only = fields
        .next()
        .ok_or_else(parse_error)?
        .parse()
        .map_err(|_| parse_error())?;
    let head_only = fields
        .next()
        .ok_or_else(parse_error)?
        .parse()
        .map_err(|_| parse_error())?;
    if fields.next().is_some() {
        return Err(parse_error());
    }
    Ok((base_only, head_only))
}

fn validate_revision(value: &str) -> GitResult<()> {
    if value.is_empty()
        || value.starts_with('-')
        || value.len() > 1024
        || value.chars().any(char::is_control)
    {
        Err(GitError::new(
            GitErrorCode::Conflict,
            "Enter a valid Git revision or branch name.",
        ))
    } else {
        Ok(())
    }
}

fn parse_utf8(value: &[u8]) -> GitResult<&str> {
    std::str::from_utf8(value).map_err(|_| parse_error())
}

fn trim_ascii_end(mut value: &[u8]) -> &[u8] {
    while value.last().is_some_and(u8::is_ascii_whitespace) {
        value = &value[..value.len() - 1];
    }
    value
}

fn parse_error() -> GitError {
    GitError::new(
        GitErrorCode::Parse,
        "Git returned data in an unexpected format.",
    )
}

fn validate_clone_url(url: &str) -> GitResult<()> {
    let url = url.trim();
    let supported = url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with("ssh://")
        || url.starts_with("git://")
        || (url.starts_with("git@") && url.contains(':'));
    if !supported || url.chars().any(char::is_control) || url.len() > 16 * 1024 {
        return Err(GitError::new(
            GitErrorCode::Conflict,
            "Enter a supported HTTPS, SSH, or Git repository URL.",
        ));
    }
    Ok(())
}

fn canonical_clone_parent(value: &str) -> GitResult<std::path::PathBuf> {
    let parent = std::path::PathBuf::from(value);
    if !parent.is_absolute() {
        return Err(GitError::new(
            GitErrorCode::InvalidWorkspace,
            "The clone destination must be an absolute runtime path.",
        ));
    }
    let canonical = parent.canonicalize().map_err(|_| {
        GitError::new(
            GitErrorCode::InvalidWorkspace,
            "The clone destination is unavailable.",
        )
    })?;
    if canonical != parent || !canonical.is_dir() {
        return Err(GitError::new(
            GitErrorCode::InvalidWorkspace,
            "The clone destination is unavailable or has changed.",
        ));
    }
    Ok(canonical)
}

fn clone_directory_name(url: &str) -> GitResult<String> {
    let name = url
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or_default()
        .strip_suffix(".git")
        .unwrap_or_else(|| {
            url.trim_end_matches('/')
                .rsplit(['/', ':'])
                .next()
                .unwrap_or_default()
        })
        .to_owned();
    validate_clone_directory_name(&name)?;
    Ok(name)
}

fn validate_clone_directory_name(name: &str) -> GitResult<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.len() > 255
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
    {
        return Err(GitError::new(
            GitErrorCode::Conflict,
            "The repository URL does not provide a safe destination name.",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn signing_wrapper(
    passphrase: &[u8],
) -> GitResult<(tempfile::TempDir, std::path::PathBuf, std::path::PathBuf)> {
    use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt};

    let directory = tempfile::Builder::new()
        .prefix("syntaxis-gpg-")
        .tempdir()
        .map_err(|_| internal_error())?;
    let path = directory.path().join("gpg-loopback");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .open(&path)
        .map_err(|_| internal_error())?;
    file.write_all(
        b"#!/bin/sh\nprogram=${SYNTAXIS_GPG_PROGRAM:-gpg}\nif [ \"$program\" = \"$0\" ]; then program=gpg; fi\nexec 3<\"$SYNTAXIS_GPG_PASSPHRASE_FILE\"\nexec \"$program\" --batch --pinentry-mode loopback --passphrase-fd 3 \"$@\"\n",
    )
    .map_err(|_| internal_error())?;
    drop(file);

    let passphrase_path = directory.path().join("passphrase");
    let mut passphrase_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&passphrase_path)
        .map_err(|_| internal_error())?;
    passphrase_file
        .write_all(passphrase)
        .and_then(|()| passphrase_file.write_all(b"\n"))
        .map_err(|_| internal_error())?;
    drop(passphrase_file);

    Ok((directory, path, passphrase_path))
}

#[cfg(not(unix))]
fn signing_wrapper(
    _passphrase: &[u8],
) -> GitResult<(tempfile::TempDir, std::path::PathBuf, std::path::PathBuf)> {
    Err(GitError::new(
        GitErrorCode::Unavailable,
        "In-app signing passphrase retry is not available on this server platform.",
    ))
}

fn internal_error() -> GitError {
    GitError::new(
        GitErrorCode::Internal,
        "The Git operation could not be completed.",
    )
}

#[cfg(test)]
mod tests;
