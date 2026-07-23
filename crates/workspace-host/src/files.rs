use std::{
    fs::{self, File},
    io::{Read, Write},
};

use async_trait::async_trait;
use syntaxis_workspace::{
    BinaryFile, ErrorCode, FileEntry, FileVersion, RelativePath, TextFile, WorkspaceError,
    WorkspaceFiles, WorkspaceRecord, WorkspaceResult,
};
use tempfile::NamedTempFile;

use crate::{
    entry::{
        copy_recursively, entry_from_path, entry_order, require_regular_file, require_size,
        too_large, version_from_metadata,
    },
    error::map_io_error,
    path_scope::PathScope,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct HostWorkspaceFiles;

impl HostWorkspaceFiles {
    fn list_entries(
        workspace: &WorkspaceRecord,
        relative: &RelativePath,
    ) -> WorkspaceResult<Vec<FileEntry>> {
        let scope = PathScope::for_workspace(workspace)?;
        let directory = scope.existing(relative)?;
        if !directory.is_dir() {
            return Err(WorkspaceError::invalid_path(
                "Only directories can be listed.",
            ));
        }
        let mut entries = fs::read_dir(directory)
            .map_err(map_io_error)?
            .filter(|entry| {
                entry.as_ref().map_or(true, |entry| {
                    entry.file_name() != std::ffi::OsStr::new(".syntaxis-worktrees")
                })
            })
            .map(|entry| {
                let entry = entry.map_err(map_io_error)?;
                let child = if relative.is_root() {
                    RelativePath::try_from(entry.file_name().to_string_lossy().as_ref())?
                } else {
                    RelativePath::try_from(format!(
                        "{}/{}",
                        relative.as_str(),
                        entry.file_name().to_string_lossy()
                    ))?
                };
                entry_from_path(&entry.path(), child)
            })
            .collect::<WorkspaceResult<Vec<_>>>()?;
        entries.sort_by(|left, right| {
            entry_order(left.kind)
                .cmp(&entry_order(right.kind))
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        Ok(entries)
    }

    fn stat_entry(
        workspace: &WorkspaceRecord,
        relative: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        let scope = PathScope::for_workspace(workspace)?;
        let path = scope.existing(relative)?;
        entry_from_path(&path, relative.clone())
    }

    fn read_text_file(
        workspace: &WorkspaceRecord,
        relative: &RelativePath,
        max_bytes: u64,
    ) -> WorkspaceResult<TextFile> {
        let scope = PathScope::for_workspace(workspace)?;
        let path = scope.existing(relative)?;
        let metadata = path.metadata().map_err(map_io_error)?;
        require_regular_file(&metadata)?;
        require_size(metadata.len(), max_bytes)?;
        let capacity = usize::try_from(metadata.len()).map_err(|_| too_large(max_bytes))?;
        let mut bytes = Vec::with_capacity(capacity);
        File::open(path)
            .map_err(map_io_error)?
            .take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(map_io_error)?;
        let size = u64::try_from(bytes.len()).map_err(|_| too_large(max_bytes))?;
        require_size(size, max_bytes)?;
        let content = String::from_utf8(bytes).map_err(|_| {
            WorkspaceError::new(
                ErrorCode::UnsupportedEncoding,
                "The file is not valid UTF-8 text.",
            )
        })?;
        Ok(TextFile {
            content,
            version: version_from_metadata(&metadata)?,
        })
    }

    fn read_binary_file(
        workspace: &WorkspaceRecord,
        relative: &RelativePath,
        max_bytes: u64,
    ) -> WorkspaceResult<BinaryFile> {
        let scope = PathScope::for_workspace(workspace)?;
        let path = scope.existing(relative)?;
        let metadata = path.metadata().map_err(map_io_error)?;
        require_regular_file(&metadata)?;
        require_size(metadata.len(), max_bytes)?;
        let capacity = usize::try_from(metadata.len()).map_err(|_| too_large(max_bytes))?;
        let mut content = Vec::with_capacity(capacity);
        File::open(path)
            .map_err(map_io_error)?
            .take(max_bytes.saturating_add(1))
            .read_to_end(&mut content)
            .map_err(map_io_error)?;
        let size = u64::try_from(content.len()).map_err(|_| too_large(max_bytes))?;
        require_size(size, max_bytes)?;
        Ok(BinaryFile {
            content,
            version: version_from_metadata(&metadata)?,
        })
    }

    fn create_empty_file(
        workspace: &WorkspaceRecord,
        relative: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        let scope = PathScope::for_workspace(workspace)?;
        let destination = scope.destination(relative)?;
        File::options()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(map_io_error)?;
        entry_from_path(&destination, relative.clone())
    }

    fn create_directory_entry(
        workspace: &WorkspaceRecord,
        relative: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        let scope = PathScope::for_workspace(workspace)?;
        let destination = scope.destination(relative)?;
        fs::create_dir(&destination).map_err(map_io_error)?;
        entry_from_path(&destination, relative.clone())
    }

    fn copy_entry(
        workspace: &WorkspaceRecord,
        source: &RelativePath,
        destination: &RelativePath,
    ) -> WorkspaceResult<()> {
        let scope = PathScope::for_workspace(workspace)?;
        let source = scope.destructive(source)?;
        let destination = scope.destination(destination)?;
        if destination.exists() {
            return Err(WorkspaceError::new(
                ErrorCode::AlreadyExists,
                "The copy destination already exists.",
            ));
        }
        copy_recursively(&source, &destination)
    }

    fn move_path(
        workspace: &WorkspaceRecord,
        source: &RelativePath,
        destination: &RelativePath,
    ) -> WorkspaceResult<()> {
        let scope = PathScope::for_workspace(workspace)?;
        let source = scope.destructive(source)?;
        let destination = scope.destination(destination)?;
        if destination.exists() {
            return Err(WorkspaceError::new(
                ErrorCode::AlreadyExists,
                "The move destination already exists.",
            ));
        }
        fs::rename(source, destination).map_err(map_io_error)
    }

    fn delete_entry(workspace: &WorkspaceRecord, relative: &RelativePath) -> WorkspaceResult<()> {
        let scope = PathScope::for_workspace(workspace)?;
        let path = scope.destructive(relative)?;
        let metadata = fs::symlink_metadata(&path).map_err(map_io_error)?;
        if metadata.file_type().is_symlink() || metadata.is_file() {
            fs::remove_file(path).map_err(map_io_error)
        } else if metadata.is_dir() {
            fs::remove_dir_all(path).map_err(map_io_error)
        } else {
            Err(WorkspaceError::invalid_path(
                "The selected entry type cannot be deleted.",
            ))
        }
    }

    fn write_text_file(
        workspace: &WorkspaceRecord,
        relative: &RelativePath,
        content: &str,
        expected: Option<&FileVersion>,
        max_bytes: u64,
    ) -> WorkspaceResult<FileVersion> {
        let size = u64::try_from(content.len()).map_err(|_| too_large(max_bytes))?;
        require_size(size, max_bytes)?;
        let scope = PathScope::for_workspace(workspace)?;
        let path = scope.existing(relative)?;
        let metadata = path.metadata().map_err(map_io_error)?;
        require_regular_file(&metadata)?;
        if expected
            .is_some_and(|expected| version_from_metadata(&metadata).as_ref() != Ok(expected))
        {
            return Err(WorkspaceError::new(
                ErrorCode::Conflict,
                "The file changed outside Syntaxis. Reload it before saving.",
            ));
        }

        let parent = path.parent().ok_or_else(WorkspaceError::internal)?;
        let mut temporary = NamedTempFile::new_in(parent).map_err(map_io_error)?;
        temporary
            .write_all(content.as_bytes())
            .map_err(map_io_error)?;
        temporary.as_file().sync_all().map_err(map_io_error)?;
        temporary
            .as_file()
            .set_permissions(metadata.permissions())
            .map_err(map_io_error)?;
        temporary
            .persist(&path)
            .map_err(|error| map_io_error(error.error))?;
        let version = version_from_metadata(&path.metadata().map_err(map_io_error)?)?;
        Ok(version)
    }
}

#[async_trait(?Send)]
impl WorkspaceFiles for HostWorkspaceFiles {
    async fn list(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<Vec<FileEntry>> {
        Self::list_entries(workspace, path)
    }

    async fn stat(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        Self::stat_entry(workspace, path)
    }

    async fn read_text(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        max_bytes: u64,
    ) -> WorkspaceResult<TextFile> {
        Self::read_text_file(workspace, path, max_bytes)
    }

    async fn read_binary(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        max_bytes: u64,
    ) -> WorkspaceResult<BinaryFile> {
        Self::read_binary_file(workspace, path, max_bytes)
    }

    async fn create_file(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        Self::create_empty_file(workspace, path)
    }

    async fn create_directory(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        Self::create_directory_entry(workspace, path)
    }

    async fn copy(
        &self,
        workspace: &WorkspaceRecord,
        source: &RelativePath,
        destination: &RelativePath,
    ) -> WorkspaceResult<()> {
        Self::copy_entry(workspace, source, destination)
    }

    async fn move_entry(
        &self,
        workspace: &WorkspaceRecord,
        source: &RelativePath,
        destination: &RelativePath,
    ) -> WorkspaceResult<()> {
        Self::move_path(workspace, source, destination)
    }

    async fn delete(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<()> {
        Self::delete_entry(workspace, path)
    }

    async fn write_text(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        content: &str,
        expected: Option<&FileVersion>,
        max_bytes: u64,
    ) -> WorkspaceResult<FileVersion> {
        Self::write_text_file(workspace, path, content, expected, max_bytes)
    }
}

#[cfg(test)]
mod tests;
