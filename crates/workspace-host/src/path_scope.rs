use std::path::PathBuf;

use syntaxis_workspace::{
    ErrorCode, RelativePath, WorkspaceError, WorkspaceRecord, WorkspaceResult,
};

use crate::error::map_io_error;

pub(crate) struct PathScope {
    root: PathBuf,
}

impl PathScope {
    pub(crate) fn for_workspace(workspace: &WorkspaceRecord) -> WorkspaceResult<Self> {
        let registered = PathBuf::from(&workspace.root);
        if !registered.is_absolute() {
            return Err(WorkspaceError::invalid_path(
                "The registered workspace root is invalid.",
            ));
        }
        let canonical = registered.canonicalize().map_err(map_io_error)?;
        if canonical != registered || !canonical.is_dir() {
            return Err(WorkspaceError::new(
                ErrorCode::Unavailable,
                "The workspace root is unavailable or has changed.",
            ));
        }
        Ok(Self { root: canonical })
    }

    pub(crate) fn existing(&self, relative: &RelativePath) -> WorkspaceResult<PathBuf> {
        let candidate = self.root.join(relative.as_str());
        let canonical = candidate.canonicalize().map_err(map_io_error)?;
        self.require_inside(canonical)
    }

    pub(crate) fn destination(&self, relative: &RelativePath) -> WorkspaceResult<PathBuf> {
        if relative.is_root() {
            return Err(WorkspaceError::new(
                ErrorCode::RootOperationRejected,
                "This operation cannot target the workspace root.",
            ));
        }

        let candidate = self.root.join(relative.as_str());
        if candidate.exists() {
            return self.existing(relative);
        }
        let parent = candidate
            .parent()
            .ok_or_else(|| WorkspaceError::invalid_path("The destination has no parent."))?
            .canonicalize()
            .map_err(map_io_error)?;
        let parent = self.require_inside(parent)?;
        if !parent.is_dir() {
            return Err(WorkspaceError::invalid_path(
                "The destination parent must be a directory.",
            ));
        }
        let name = candidate
            .file_name()
            .ok_or_else(|| WorkspaceError::invalid_path("The destination needs a file name."))?;
        Ok(parent.join(name))
    }

    pub(crate) fn destructive(&self, relative: &RelativePath) -> WorkspaceResult<PathBuf> {
        if relative.is_root() {
            return Err(WorkspaceError::new(
                ErrorCode::RootOperationRejected,
                "Destructive operations cannot target the workspace root.",
            ));
        }
        let candidate = self.root.join(relative.as_str());
        let parent = candidate
            .parent()
            .ok_or_else(|| WorkspaceError::invalid_path("The entry has no parent."))?
            .canonicalize()
            .map_err(map_io_error)?;
        self.require_inside(parent)?;
        let metadata = std::fs::symlink_metadata(&candidate).map_err(map_io_error)?;
        if metadata.file_type().is_symlink() {
            Ok(candidate)
        } else {
            self.existing(relative)
        }
    }

    fn require_inside(&self, canonical: PathBuf) -> WorkspaceResult<PathBuf> {
        if canonical.starts_with(&self.root) {
            Ok(canonical)
        } else {
            Err(WorkspaceError::new(
                ErrorCode::OutsideAllowedRoot,
                "The path resolves outside the active workspace.",
            ))
        }
    }
}
