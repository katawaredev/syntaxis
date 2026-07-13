use std::path::Path;

use async_trait::async_trait;
use syntaxis_workspace::{
    BrowseDirectory, BrowseRoot, ErrorCode, WorkspaceBrowser, WorkspaceError, WorkspaceResult,
};

use crate::{error::map_io_error, RegistrationPolicy};

#[derive(Clone, Debug)]
pub struct HostWorkspaceBrowser {
    policy: RegistrationPolicy,
}

impl HostWorkspaceBrowser {
    /// Creates a browser constrained by the supplied registration policy.
    ///
    /// # Errors
    ///
    /// Returns an error if an allowlisted root cannot be canonicalized.
    pub fn new(policy: RegistrationPolicy) -> WorkspaceResult<Self> {
        Ok(Self {
            policy: policy.canonicalize()?,
        })
    }

    fn require_permitted(&self, path: &Path) -> WorkspaceResult<std::path::PathBuf> {
        let canonical = path.canonicalize().map_err(map_io_error)?;
        if !canonical.is_dir() {
            return Err(WorkspaceError::invalid_path(
                "Only directories can be browsed.",
            ));
        }
        if self.policy.permits(&canonical) {
            Ok(canonical)
        } else {
            Err(WorkspaceError::new(
                ErrorCode::OutsideAllowedRoot,
                "That directory is outside the roots exposed by this runtime.",
            ))
        }
    }
}

#[async_trait(?Send)]
impl WorkspaceBrowser for HostWorkspaceBrowser {
    async fn roots(&self) -> WorkspaceResult<Vec<BrowseRoot>> {
        Ok(self
            .policy
            .roots()
            .iter()
            .map(|root| BrowseRoot {
                name: root
                    .file_name()
                    .unwrap_or(root.as_os_str())
                    .to_string_lossy()
                    .into_owned(),
                path: root.to_string_lossy().into_owned(),
            })
            .collect())
    }

    async fn directories(&self, absolute_path: &str) -> WorkspaceResult<Vec<BrowseDirectory>> {
        let directory = self.require_permitted(Path::new(absolute_path))?;
        let mut directories = std::fs::read_dir(directory)
            .map_err(map_io_error)?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                let file_type = entry.file_type().ok()?;
                if !file_type.is_dir() || file_type.is_symlink() {
                    return None;
                }
                Some(BrowseDirectory {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path: path.to_string_lossy().into_owned(),
                })
            })
            .collect::<Vec<_>>();
        directories.sort_by_key(|entry| entry.name.to_lowercase());
        Ok(directories)
    }
}

#[cfg(test)]
mod tests {
    use futures_lite::future::block_on;
    use syntaxis_workspace::{ErrorCode, WorkspaceBrowser};
    use tempfile::tempdir;

    use crate::{HostWorkspaceBrowser, RegistrationPolicy};

    #[test]
    fn browser_cannot_leave_allowlisted_roots() {
        let allowed = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let browser = HostWorkspaceBrowser::new(RegistrationPolicy::Allowlisted {
            roots: vec![allowed.path().to_owned()],
        })
        .unwrap();
        let error = block_on(browser.directories(outside.path().to_str().unwrap())).unwrap_err();
        assert_eq!(error.code, ErrorCode::OutsideAllowedRoot);
    }
}
