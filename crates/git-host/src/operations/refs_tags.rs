use std::path::Path;

use syntaxis_git::{GitError, GitErrorCode, GitResult, TagInfo, TagRequest};
use syntaxis_workspace::WorkspaceRecord;

use crate::runner::{HostGit, validated_root};

use super::parsing::parse_tags;
use super::validation::validate_revision;

const MAX_TAG_MESSAGE_BYTES: usize = 256 * 1024;

impl HostGit {
    pub(super) async fn list_tags(&self, workspace: &WorkspaceRecord) -> GitResult<Vec<TagInfo>> {
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

    pub(super) async fn create_repository_tag(
        &self,
        workspace: &WorkspaceRecord,
        request: TagRequest,
    ) -> GitResult<()> {
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

    pub(super) async fn delete_repository_tag(
        &self,
        workspace: &WorkspaceRecord,
        name: &str,
    ) -> GitResult<()> {
        let root = validated_root(workspace)?;
        self.validate_tag_name(&root, name).await?;
        let arguments = ["tag".into(), "-d".into(), "--".into(), name.into()];
        self.run_default(&root, &arguments).await?;
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
}
