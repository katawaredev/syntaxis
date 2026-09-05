use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, RetryAdvice};
use syntaxis_workspace::{ErrorCode, FileVersion, RelativePath, WorkspaceRecord};

use crate::{FilesPorts, files_error};

/// Controls whether an upload may replace an existing workspace entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UploadCollisionPolicy {
    Overwrite,
    RejectExisting,
}

/// Runtime-specific limits for the shared upload use case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadPolicy {
    pub max_bytes: u64,
    pub collision: UploadCollisionPolicy,
}

impl UploadPolicy {
    pub const fn overwrite(max_bytes: u64) -> Self {
        Self {
            max_bytes,
            collision: UploadCollisionPolicy::Overwrite,
        }
    }

    pub const fn reject_existing(max_bytes: u64) -> Self {
        Self {
            max_bytes,
            collision: UploadCollisionPolicy::RejectExisting,
        }
    }
}

/// A validated upload target. Browser-native file reading can happen after this inexpensive step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedUpload {
    name: String,
    path: RelativePath,
    policy: UploadPolicy,
}

impl PreparedUpload {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> &RelativePath {
        &self.path
    }
}

fn upload_error(
    code: AppErrorCode,
    message: impl Into<String>,
    retry: RetryAdvice,
) -> AppError {
    AppError::new(code, message, retry, ErrorSource::Files)
}

fn readable_limit(max_bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    const KIB: u64 = 1024;
    if max_bytes.is_multiple_of(MIB) {
        format!("{} MiB", max_bytes / MIB)
    } else if max_bytes.is_multiple_of(KIB) {
        format!("{} KiB", max_bytes / KIB)
    } else {
        format!("{max_bytes} bytes")
    }
}

/// Validates browser-provided upload metadata and resolves a workspace-relative destination.
///
/// This deliberately accepts picker paths but retains only their final component, matching the
/// browser file-input behavior used by both application runtimes.
pub fn prepare_upload(
    directory: &RelativePath,
    picker_name: &str,
    declared_size: u64,
    policy: UploadPolicy,
) -> Result<PreparedUpload, AppError> {
    let name = picker_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default();
    if name.is_empty() || name == "." || name == ".." {
        return Err(upload_error(
            AppErrorCode::InvalidInput,
            "A selected file has an invalid name.",
            RetryAdvice::Never,
        ));
    }
    if declared_size > policy.max_bytes {
        return Err(upload_error(
            AppErrorCode::TooLarge,
            format!(
                "{name} exceeds the {} upload limit.",
                readable_limit(policy.max_bytes)
            ),
            RetryAdvice::Never,
        ));
    }

    let path = if directory.is_root() {
        name.to_owned()
    } else {
        format!("{}/{name}", directory.as_str())
    };
    let path = RelativePath::try_from(path).map_err(files_error)?;
    Ok(PreparedUpload {
        name: name.to_owned(),
        path,
        policy,
    })
}

/// Writes a prepared upload through the injected workspace Files port.
///
/// The actual byte length is checked again because picker metadata is not an authority boundary.
pub async fn execute_upload(
    files: &FilesPorts,
    workspace: &WorkspaceRecord,
    upload: &PreparedUpload,
    content: &[u8],
) -> Result<FileVersion, AppError> {
    let content_size = u64::try_from(content.len()).unwrap_or(u64::MAX);
    if content_size > upload.policy.max_bytes {
        return Err(upload_error(
            AppErrorCode::TooLarge,
            format!(
                "{} exceeds the {} upload limit.",
                upload.name,
                readable_limit(upload.policy.max_bytes)
            ),
            RetryAdvice::Never,
        ));
    }

    if upload.policy.collision == UploadCollisionPolicy::RejectExisting {
        match files.files().stat(workspace, &upload.path).await {
            Ok(_) => {
                return Err(upload_error(
                    AppErrorCode::Conflict,
                    format!("{} already exists.", upload.name),
                    RetryAdvice::AfterUserAction,
                ));
            }
            Err(error) if error.code == ErrorCode::NotFound => {}
            Err(error) => return Err(files_error(error)),
        }
    }

    files
        .files()
        .write_binary(workspace, &upload.path, content, upload.policy.max_bytes)
        .await
        .map_err(files_error)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures_lite::future::block_on;
    use syntaxis_workspace::{
        MockWorkspaceFiles, WorkspaceAvailability, WorkspaceFiles, WorkspaceIcon,
        WorkspaceIconSymbol, WorkspaceId, WorkspaceProfile, WorkspaceSection,
    };

    use super::*;

    fn workspace() -> WorkspaceRecord {
        WorkspaceRecord {
            id: WorkspaceId::new("uploads"),
            slug: "uploads".into(),
            name: "Uploads".into(),
            root: "/uploads".into(),
            icon: WorkspaceIcon::Symbol {
                name: WorkspaceIconSymbol::Folder,
            },
            profile: WorkspaceProfile::default(),
            registered_at_unix_ms: 0,
            last_opened_unix_ms: 0,
            last_section: WorkspaceSection::Files,
            availability: WorkspaceAvailability::Available,
        }
    }

    fn ports(files: Arc<MockWorkspaceFiles>) -> FilesPorts {
        FilesPorts::new(
            files,
            Arc::new(crate::FixedWorkspaceSearch::default()),
            Arc::new(crate::MemoryFilesSession::default()),
        )
    }

    #[test]
    fn upload_metadata_is_normalized_and_bounded() {
        let policy = UploadPolicy::overwrite(4 * 1024 * 1024);
        let upload = prepare_upload(
            &RelativePath::try_from("assets").unwrap(),
            r"picker\photo.png",
            12,
            policy,
        )
        .unwrap();
        assert_eq!(upload.name(), "photo.png");
        assert_eq!(upload.path().as_str(), "assets/photo.png");

        let too_large = prepare_upload(
            &RelativePath::root(),
            "archive.zip",
            policy.max_bytes + 1,
            policy,
        )
        .expect_err("oversized picker metadata must be rejected before reading");
        assert_eq!(too_large.code, AppErrorCode::TooLarge);
        assert_eq!(too_large.source, ErrorSource::Files);

        prepare_upload(&RelativePath::root(), "..", 0, policy)
            .expect_err("reserved path components must be rejected");
    }

    #[test]
    fn upload_execution_honors_collision_policy_and_actual_size() {
        let workspace = workspace();
        let adapter = Arc::new(MockWorkspaceFiles::default());
        let ports = ports(Arc::clone(&adapter));
        let path = RelativePath::try_from("notes.txt").unwrap();
        block_on(adapter.create_file(&workspace, &path)).unwrap();

        let reject = prepare_upload(
            &RelativePath::root(),
            "notes.txt",
            3,
            UploadPolicy::reject_existing(8),
        )
        .unwrap();
        let collision = block_on(execute_upload(&ports, &workspace, &reject, b"new"))
            .expect_err("browser uploads must not silently replace files");
        assert_eq!(collision.code, AppErrorCode::Conflict);

        let overwrite = prepare_upload(
            &RelativePath::root(),
            "notes.txt",
            3,
            UploadPolicy::overwrite(8),
        )
        .unwrap();
        block_on(execute_upload(&ports, &workspace, &overwrite, b"new")).unwrap();
        assert_eq!(
            block_on(adapter.read_binary(&workspace, &path, 8))
                .unwrap()
                .content,
            b"new"
        );

        let actual_too_large = block_on(execute_upload(
            &ports,
            &workspace,
            &overwrite,
            b"too large",
        ))
        .expect_err("actual bytes must be bounded independently of picker metadata");
        assert_eq!(actual_too_large.code, AppErrorCode::TooLarge);
    }
}
