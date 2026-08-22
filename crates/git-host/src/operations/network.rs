use std::ffi::OsString;

use syntaxis_git::{GitError, GitErrorCode, GitOperations, GitResult, PushOutcome, RemoteResult};
use syntaxis_workspace::WorkspaceRecord;
use tokio_util::sync::CancellationToken;

use crate::runner::{HostGit, validated_root};

pub(super) const GITHUB_SSH_PUSH_REWRITE: &str =
    "url.ssh://git@github.com/.pushInsteadOf=https://github.com/";

impl HostGit {
    pub(super) async fn fetch_all(&self, workspace: &WorkspaceRecord) -> GitResult<RemoteResult> {
        let root = validated_root(workspace)?;
        self.run(
            &root,
            &["fetch".into(), "--prune".into()],
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

    pub(super) async fn pull_fast_forward(
        &self,
        workspace: &WorkspaceRecord,
    ) -> GitResult<RemoteResult> {
        let root = validated_root(workspace)?;
        let result = self
            .run(
                &root,
                &["pull".into(), "--ff-only".into(), "--prune".into()],
                None,
                &[("GIT_TERMINAL_PROMPT", "0".into())],
                &[0],
                CancellationToken::new(),
            )
            .await;
        match result {
            Ok(_) => Ok(RemoteResult {
                message: "Pull completed.".into(),
            }),
            Err(error)
                if error.code == GitErrorCode::CommandFailed
                    && self.status(workspace).await.is_ok_and(|status| {
                        status.branch.ahead > 0 && status.branch.behind > 0
                    }) =>
            {
                Err(GitError::new(
                    GitErrorCode::Conflict,
                    "The local and upstream branches have diverged. Rebase or merge them in the terminal before pulling.",
                ))
            }
            Err(error) => Err(error),
        }
    }

    pub(super) async fn push_upstream(
        &self,
        workspace: &WorkspaceRecord,
        force_with_lease: bool,
    ) -> GitResult<PushOutcome> {
        let root = validated_root(workspace)?;
        let environment = [("GIT_TERMINAL_PROMPT", "0".into())];
        let mut result = self
            .run(
                &root,
                &push_arguments(force_with_lease, false),
                None,
                &environment,
                &[0],
                CancellationToken::new(),
            )
            .await;
        if result
            .as_ref()
            .is_err_and(|error| error.code == GitErrorCode::Authentication)
        {
            result = self
                .run(
                    &root,
                    &push_arguments(force_with_lease, true),
                    None,
                    &environment,
                    &[0],
                    CancellationToken::new(),
                )
                .await;
        }
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

    pub(super) async fn publish_current_branch(
        &self,
        workspace: &WorkspaceRecord,
        remote: &str,
    ) -> GitResult<RemoteResult> {
        let root = validated_root(workspace)?;
        let remote = super::validation::validate_remote_name(remote)?;
        self.run_default(
            &root,
            &[
                "remote".into(),
                "get-url".into(),
                "--push".into(),
                "--".into(),
                remote.clone().into(),
            ],
        )
        .await
        .map_err(|error| {
            if error.code == GitErrorCode::CommandFailed {
                GitError::new(
                    GitErrorCode::Conflict,
                    "The selected Git remote does not exist.",
                )
            } else {
                error
            }
        })?;

        let environment = [("GIT_TERMINAL_PROMPT", "0".into())];
        let mut result = self
            .run(
                &root,
                &publish_arguments(&remote, false),
                None,
                &environment,
                &[0],
                CancellationToken::new(),
            )
            .await;
        if result
            .as_ref()
            .is_err_and(|error| error.code == GitErrorCode::Authentication)
        {
            result = self
                .run(
                    &root,
                    &publish_arguments(&remote, true),
                    None,
                    &environment,
                    &[0],
                    CancellationToken::new(),
                )
                .await;
        }
        result?;
        Ok(RemoteResult {
            message: format!("Published branch to {remote}."),
        })
    }
}

pub(super) fn push_arguments(force_with_lease: bool, github_ssh_fallback: bool) -> Vec<OsString> {
    let mut arguments = Vec::with_capacity(usize::from(github_ssh_fallback) * 2 + 2);
    if github_ssh_fallback {
        arguments.extend(["-c".into(), GITHUB_SSH_PUSH_REWRITE.into()]);
    }
    arguments.push("push".into());
    if force_with_lease {
        arguments.push("--force-with-lease".into());
    }
    arguments
}

pub(super) fn publish_arguments(remote: &str, github_ssh_fallback: bool) -> Vec<OsString> {
    let mut arguments = Vec::with_capacity(usize::from(github_ssh_fallback) * 2 + 5);
    if github_ssh_fallback {
        arguments.extend(["-c".into(), GITHUB_SSH_PUSH_REWRITE.into()]);
    }
    arguments.extend([
        "push".into(),
        "--set-upstream".into(),
        remote.into(),
        "HEAD".into(),
    ]);
    arguments
}
