use std::path::PathBuf;

use syntaxis_git::{CommitRequest, GitError, GitErrorCode, GitResult, RemoteRequest};

const MAX_COMMIT_MESSAGE_BYTES: usize = 256 * 1024;
const MAX_PASSPHRASE_BYTES: usize = 16 * 1024;
const MAX_REMOTE_URL_BYTES: usize = 64 * 1024;

pub(super) fn validate_remote_request(request: &RemoteRequest) -> GitResult<()> {
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

pub(super) fn validate_remote_name(name: &str) -> GitResult<String> {
    let name = name.trim();
    if name.is_empty()
        || name.starts_with('-')
        || name.len() > 1024
        || name.chars().any(char::is_control)
    {
        return Err(GitError::new(
            GitErrorCode::Conflict,
            "Enter a valid Git remote name.",
        ));
    }
    Ok(name.to_owned())
}

pub(super) fn validate_commit_request(request: &CommitRequest) -> GitResult<()> {
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

pub(super) fn validate_revision(value: &str) -> GitResult<()> {
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

pub(super) fn validate_clone_url(url: &str) -> GitResult<()> {
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

pub(super) fn canonical_clone_parent(value: &str) -> GitResult<PathBuf> {
    let parent = PathBuf::from(value);
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

pub(super) fn clone_directory_name(url: &str) -> GitResult<String> {
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

pub(super) fn validate_clone_directory_name(name: &str) -> GitResult<()> {
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
