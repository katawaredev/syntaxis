use std::{
    fs::{self, Metadata},
    path::Path,
    time::UNIX_EPOCH,
};

use syntaxis_workspace::{
    EntryKind, ErrorCode, FileEntry, FileVersion, RelativePath, WorkspaceError, WorkspaceResult,
};

use crate::error::map_io_error;

pub(crate) fn entry_from_path(path: &Path, relative: RelativePath) -> WorkspaceResult<FileEntry> {
    let metadata = fs::symlink_metadata(path).map_err(map_io_error)?;
    let kind = if metadata.file_type().is_symlink() {
        EntryKind::Symlink
    } else if metadata.is_dir() {
        EntryKind::Directory
    } else {
        EntryKind::File
    };
    let name = path.file_name().map_or_else(
        || "/".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let version = (kind == EntryKind::File)
        .then(|| version_from_metadata(&metadata))
        .transpose()?;
    Ok(FileEntry {
        path: relative,
        name,
        kind,
        size: metadata.len(),
        version,
    })
}

pub(crate) fn version_from_metadata(metadata: &Metadata) -> WorkspaceResult<FileVersion> {
    let modified_unix_nanos = metadata
        .modified()
        .map_err(map_io_error)?
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WorkspaceError::internal())?
        .as_nanos();
    Ok(FileVersion {
        length: metadata.len(),
        modified_unix_nanos,
    })
}

pub(crate) fn require_regular_file(metadata: &Metadata) -> WorkspaceResult<()> {
    if metadata.is_file() {
        Ok(())
    } else {
        Err(WorkspaceError::invalid_path(
            "The selected path is not a regular file.",
        ))
    }
}

pub(crate) fn require_size(actual: u64, maximum: u64) -> WorkspaceResult<()> {
    if actual <= maximum {
        Ok(())
    } else {
        Err(too_large(maximum))
    }
}

pub(crate) fn too_large(maximum: u64) -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::TooLarge,
        format!("The file exceeds the {maximum}-byte operation limit."),
    )
}

pub(crate) fn entry_order(kind: EntryKind) -> u8 {
    match kind {
        EntryKind::Directory => 0,
        EntryKind::File => 1,
        EntryKind::Symlink => 2,
    }
}

pub(crate) fn copy_recursively(source: &Path, destination: &Path) -> WorkspaceResult<()> {
    let metadata = fs::symlink_metadata(source).map_err(map_io_error)?;
    if metadata.file_type().is_symlink() {
        return Err(WorkspaceError::invalid_path(
            "Copying symbolic links is not supported.",
        ));
    }
    if metadata.is_file() {
        fs::copy(source, destination).map_err(map_io_error)?;
        return Ok(());
    }
    fs::create_dir(destination).map_err(map_io_error)?;
    for entry in fs::read_dir(source).map_err(map_io_error)? {
        let entry = entry.map_err(map_io_error)?;
        copy_recursively(&entry.path(), &destination.join(entry.file_name()))?;
    }
    Ok(())
}
