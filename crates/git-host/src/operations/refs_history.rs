use syntaxis_git::{CommitDetail, CommitInfo, GitResult};
use syntaxis_workspace::WorkspaceRecord;

use crate::runner::{HostGit, validated_root};

use super::parsing::{parse_error, parse_history, parse_numstat, trim_ascii_end};
use super::validation::validate_revision;

impl HostGit {
    pub(super) async fn load_history(
        &self,
        workspace: &WorkspaceRecord,
        limit: u32,
    ) -> GitResult<Vec<CommitInfo>> {
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

    pub(super) async fn load_commit_message(
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

    pub(super) async fn load_commit_detail(
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
}
