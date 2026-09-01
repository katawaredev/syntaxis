use crate::storage::{load_directory, save_directory};
use async_trait::async_trait;
use js_sys::{Promise, Reflect, Uint8Array};
use std::cell::RefCell;
use syntaxis_workspace::{
    BinaryFile, EntryKind, ErrorCode, FileEntry, FileVersion, RelativePath, TextFile,
    WorkspaceError, WorkspaceFiles, WorkspaceRecord, WorkspaceResult,
};
use wasm_bindgen::{JsCast, JsValue, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, File, FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemGetDirectoryOptions,
    FileSystemGetFileOptions, FileSystemHandle, FileSystemHandleKind, FileSystemWritableFileStream,
    WritableStream,
};
thread_local! {
    static LOCAL_ROOT: RefCell<Option<FileSystemDirectoryHandle>> = const {
        RefCell::new(None)
    };
}
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = web_sys::Window, js_name = Window)]
    type DirectoryPickerWindow;
    #[wasm_bindgen(catch, method, js_name = showDirectoryPicker)]
    fn show_directory_picker(window: &DirectoryPickerWindow) -> Result<Promise, JsValue>;
    #[wasm_bindgen(extends = FileSystemHandle, js_name = FileSystemHandle)]
    type PermissionFileSystemHandle;
    #[wasm_bindgen(catch, method, js_name = queryPermission)]
    fn query_permission(
        handle: &PermissionFileSystemHandle,
        descriptor: &JsValue,
    ) -> Result<Promise, JsValue>;
    #[wasm_bindgen(catch, method, js_name = requestPermission)]
    fn request_permission(
        handle: &PermissionFileSystemHandle,
        descriptor: &JsValue,
    ) -> Result<Promise, JsValue>;
}
/// State of a local directory handle restored from browser storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SavedDirectory {
    /// No directory handle has been saved.
    Missing,
    /// A handle exists but the browser requires a user gesture to restore access.
    NeedsPermission(String),
    /// The saved directory is active.
    Active(String),
}
/// Result of selecting a local directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedDirectory {
    /// The display name of the selected directory.
    pub name: String,
    /// A warning when the directory could not be persisted for a later reload.
    pub persistence_warning: Option<String>,
}
/// Workspace file operations backed by the browser's Origin Private File System.
///
/// This is intentionally a zero-sized adapter. Browser handles never enter Rust
/// application state and are reacquired for each operation, keeping the shared
/// `WorkspaceFiles` boundary platform-neutral.
#[derive(Clone, Copy, Debug, Default)]
pub struct OpfsWorkspaceFiles;
/// Returns whether this browser exposes the user-visible directory picker.
pub fn local_directory_picker_supported() -> bool {
    web_sys::window().is_some_and(|window| {
        Reflect::get(window.as_ref(), &JsValue::from_str("showDirectoryPicker"))
            .is_ok_and(|value| value.is_function())
    })
}
/// Prompts for a local directory and makes it the active workspace root.
///
/// The browser call is made directly through a Rust `wasm-bindgen` declaration;
/// no JavaScript adapter is involved.
///
/// # Errors
///
/// Returns an unavailable error when the picker is unsupported, rejected, or
/// dismissed by the user.
pub async fn select_local_directory() -> WorkspaceResult<SelectedDirectory> {
    let window = web_sys::window().ok_or_else(|| not_available("A browser window is required."))?;
    let promise = window
        .unchecked_ref::<DirectoryPickerWindow>()
        .show_directory_picker()
        .map_err(|error| browser_error("Could not open the folder picker", error))?;
    let value = JsFuture::from(promise)
        .await
        .map_err(|error| browser_error("Folder selection was cancelled", error))?;
    let directory = value.unchecked_into::<FileSystemDirectoryHandle>();
    let name = directory.unchecked_ref::<FileSystemHandle>().name();
    let persistence_warning = save_directory(&directory).await.err().map(|error| {
        format!(
            "Could not remember this folder for the next visit: {}",
            error.message,
        )
    });
    LOCAL_ROOT.with(|root| root.replace(Some(directory)));
    Ok(SelectedDirectory {
        name,
        persistence_warning,
    })
}
/// Restores the last selected local directory from `IndexedDB`.
///
/// Set `request_access` only in response to a user gesture. Browsers generally
/// reject permission prompts initiated during application startup.
///
/// # Errors
///
/// Returns an unavailable error when browser storage or the permission API
/// cannot be accessed.
pub async fn restore_local_directory(request_access: bool) -> WorkspaceResult<SavedDirectory> {
    let Some(directory) = load_directory().await? else {
        return Ok(SavedDirectory::Missing);
    };
    let name = directory.unchecked_ref::<FileSystemHandle>().name();
    let descriptor = permission_descriptor()?;
    let handle = directory.unchecked_ref::<PermissionFileSystemHandle>();
    let promise = if request_access {
        handle
            .request_permission(&descriptor)
            .map_err(|error| browser_error("Could not request folder access", error))?
    } else {
        handle
            .query_permission(&descriptor)
            .map_err(|error| browser_error("Could not inspect folder access", error))?
    };
    let permission = JsFuture::from(promise)
        .await
        .map_err(|error| browser_error("Could not inspect folder access", error))?
        .as_string()
        .unwrap_or_default();
    if permission == "granted" {
        LOCAL_ROOT.with(|root| root.replace(Some(directory)));
        Ok(SavedDirectory::Active(name))
    } else {
        Ok(SavedDirectory::NeedsPermission(name))
    }
}
/// Switches the adapter back to its origin-private browser workspace.
pub fn set_private_workspace() {
    LOCAL_ROOT.with(|root| root.replace(None));
}
#[async_trait(?Send)]
impl WorkspaceFiles for OpfsWorkspaceFiles {
    async fn list(
        &self,
        _workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<Vec<FileEntry>> {
        let directory = resolve_directory(path).await?;
        let iterator = directory.values();
        let mut entries = Vec::new();
        loop {
            let next = iterator
                .next()
                .map_err(|error| browser_error("Could not enumerate the workspace", error))?;
            let result = JsFuture::from(next)
                .await
                .map_err(|error| browser_error("Could not enumerate the workspace", error))?;
            let done = Reflect::get(&result, &JsValue::from_str("done"))
                .ok()
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            if done {
                break;
            }
            let handle = Reflect::get(&result, &JsValue::from_str("value"))
                .map_err(|error| browser_error("Could not read a directory entry", error))?
                .unchecked_into::<FileSystemHandle>();
            entries.push(entry_from_handle(path, handle).await?);
        }
        entries.sort_by(|left, right| {
            entry_rank(left.kind)
                .cmp(&entry_rank(right.kind))
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        Ok(entries)
    }
    async fn stat(
        &self,
        _workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        if path.is_root() {
            return Ok(FileEntry {
                path: RelativePath::root(),
                name: "Syntaxis Guest".into(),
                kind: EntryKind::Directory,
                size: 0,
                version: None,
            });
        }
        let (parent, name) = resolve_parent(path).await?;
        if let Ok(handle) = get_file_handle(&parent, &name, false).await {
            return entry_from_file_handle(path.clone(), name, handle).await;
        }
        get_directory_handle(&parent, &name, false).await?;
        Ok(FileEntry {
            path: path.clone(),
            name,
            kind: EntryKind::Directory,
            size: 0,
            version: None,
        })
    }
    async fn read_text(
        &self,
        _workspace: &WorkspaceRecord,
        path: &RelativePath,
        max_bytes: u64,
    ) -> WorkspaceResult<TextFile> {
        let file = open_file(path).await?;
        let version = file_version(&file)?;
        ensure_size(version.length, max_bytes)?;
        let content = JsFuture::from(file.text())
            .await
            .map_err(|error| browser_error("Could not read the file", error))?
            .as_string()
            .ok_or_else(|| {
                WorkspaceError::new(
                    ErrorCode::UnsupportedEncoding,
                    "The selected file is not valid text.",
                )
            })?;
        Ok(TextFile { content, version })
    }
    async fn read_binary(
        &self,
        _workspace: &WorkspaceRecord,
        path: &RelativePath,
        max_bytes: u64,
    ) -> WorkspaceResult<BinaryFile> {
        let file = open_file(path).await?;
        let version = file_version(&file)?;
        ensure_size(version.length, max_bytes)?;
        let buffer = JsFuture::from(file.array_buffer())
            .await
            .map_err(|error| browser_error("Could not read the file", error))?;
        Ok(BinaryFile {
            content: Uint8Array::new(&buffer).to_vec(),
            version,
        })
    }
    async fn create_file(
        &self,
        _workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        let (parent, name) = resolve_parent(path).await?;
        let handle = get_file_handle(&parent, &name, true).await?;
        entry_from_file_handle(path.clone(), name, handle).await
    }
    async fn create_directory(
        &self,
        _workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        let (parent, name) = resolve_parent(path).await?;
        get_directory_handle(&parent, &name, true).await?;
        Ok(FileEntry {
            path: path.clone(),
            name,
            kind: EntryKind::Directory,
            size: 0,
            version: None,
        })
    }
    async fn copy(
        &self,
        _workspace: &WorkspaceRecord,
        source: &RelativePath,
        destination: &RelativePath,
    ) -> WorkspaceResult<()> {
        copy_entry(source, destination).await
    }
    async fn move_entry(
        &self,
        _workspace: &WorkspaceRecord,
        source: &RelativePath,
        destination: &RelativePath,
    ) -> WorkspaceResult<()> {
        copy_entry(source, destination).await?;
        delete_entry(source).await
    }
    async fn delete(
        &self,
        _workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<()> {
        delete_entry(path).await
    }
    async fn write_text(
        &self,
        _workspace: &WorkspaceRecord,
        path: &RelativePath,
        content: &str,
        expected: Option<&FileVersion>,
        max_bytes: u64,
    ) -> WorkspaceResult<FileVersion> {
        let length = u64::try_from(content.len()).unwrap_or(u64::MAX);
        ensure_size(length, max_bytes)?;
        write_file(path, content.as_bytes(), expected).await
    }
    async fn write_binary(
        &self,
        _workspace: &WorkspaceRecord,
        path: &RelativePath,
        content: &[u8],
        max_bytes: u64,
    ) -> WorkspaceResult<FileVersion> {
        let length = u64::try_from(content.len()).unwrap_or(u64::MAX);
        ensure_size(length, max_bytes)?;
        write_file(path, content, None).await
    }
}
async fn copy_entry(source: &RelativePath, destination: &RelativePath) -> WorkspaceResult<()> {
    reject_root_operation(source)?;
    reject_root_operation(destination)?;
    if source == destination {
        return Err(WorkspaceError::invalid_path(
            "Source and destination must be different.",
        ));
    }
    if entry_exists(destination).await {
        return Err(WorkspaceError::new(
            ErrorCode::AlreadyExists,
            "The destination already exists.",
        ));
    }
    let source_kind = entry_kind(source).await?;
    if source_kind == EntryKind::File {
        return copy_file(source, destination).await;
    }
    if is_descendant(destination, source) {
        return Err(WorkspaceError::invalid_path(
            "A directory cannot be copied inside itself.",
        ));
    }
    create_directory_path(destination).await?;
    let mut pending = vec![(source.clone(), destination.clone())];
    while let Some((source_directory, destination_directory)) = pending.pop() {
        for handle in directory_handles(&source_directory).await? {
            let name = handle.name();
            let child_source = child_path(&source_directory, &name)?;
            let child_destination = child_path(&destination_directory, &name)?;
            match handle.kind() {
                FileSystemHandleKind::File => {
                    copy_file(&child_source, &child_destination).await?;
                }
                FileSystemHandleKind::Directory => {
                    create_directory_path(&child_destination).await?;
                    pending.push((child_source, child_destination));
                }
                _ => {
                    return Err(not_available("This browser file type is not supported."));
                }
            }
        }
    }
    Ok(())
}
async fn copy_file(source: &RelativePath, destination: &RelativePath) -> WorkspaceResult<()> {
    let file = open_file(source).await?;
    let buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(|error| browser_error("Could not copy the file", error))?;
    write_file(destination, &Uint8Array::new(&buffer).to_vec(), None).await?;
    Ok(())
}
async fn create_directory_path(path: &RelativePath) -> WorkspaceResult<()> {
    let (parent, name) = resolve_parent(path).await?;
    get_directory_handle(&parent, &name, true).await?;
    Ok(())
}
async fn delete_entry(path: &RelativePath) -> WorkspaceResult<()> {
    reject_root_operation(path)?;
    if entry_kind(path).await? == EntryKind::Directory {
        let mut pending = vec![path.clone()];
        let mut directories = Vec::new();
        while let Some(directory) = pending.pop() {
            directories.push(directory.clone());
            for handle in directory_handles(&directory).await? {
                let child = child_path(&directory, &handle.name())?;
                if handle.kind() == FileSystemHandleKind::Directory {
                    pending.push(child);
                } else {
                    remove_entry(&child).await?;
                }
            }
        }
        for directory in directories.into_iter().rev() {
            remove_entry(&directory).await?;
        }
        return Ok(());
    }
    remove_entry(path).await
}
async fn remove_entry(path: &RelativePath) -> WorkspaceResult<()> {
    let (parent, name) = resolve_parent(path).await?;
    JsFuture::from(parent.remove_entry(&name))
        .await
        .map_err(|error| browser_error("Could not delete the entry", error))?;
    Ok(())
}
async fn entry_kind(path: &RelativePath) -> WorkspaceResult<EntryKind> {
    let (parent, name) = resolve_parent(path).await?;
    if get_file_handle(&parent, &name, false).await.is_ok() {
        Ok(EntryKind::File)
    } else {
        get_directory_handle(&parent, &name, false).await?;
        Ok(EntryKind::Directory)
    }
}
async fn entry_exists(path: &RelativePath) -> bool {
    let Ok((parent, name)) = resolve_parent(path).await else {
        return false;
    };
    get_file_handle(&parent, &name, false).await.is_ok()
        || get_directory_handle(&parent, &name, false).await.is_ok()
}
async fn directory_handles(path: &RelativePath) -> WorkspaceResult<Vec<FileSystemHandle>> {
    let directory = resolve_directory(path).await?;
    let iterator = directory.values();
    let mut handles = Vec::new();
    loop {
        let next = iterator
            .next()
            .map_err(|error| browser_error("Could not enumerate the directory", error))?;
        let result = JsFuture::from(next)
            .await
            .map_err(|error| browser_error("Could not enumerate the directory", error))?;
        if Reflect::get(&result, &JsValue::from_str("done"))
            .ok()
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            break;
        }
        handles.push(
            Reflect::get(&result, &JsValue::from_str("value"))
                .map_err(|error| browser_error("Could not read a directory entry", error))?
                .unchecked_into(),
        );
    }
    Ok(handles)
}
fn reject_root_operation(path: &RelativePath) -> WorkspaceResult<()> {
    if path.is_root() {
        Err(WorkspaceError::new(
            ErrorCode::RootOperationRejected,
            "The workspace root cannot be modified.",
        ))
    } else {
        Ok(())
    }
}
fn is_descendant(candidate: &RelativePath, parent: &RelativePath) -> bool {
    candidate
        .as_str()
        .strip_prefix(parent.as_str())
        .is_some_and(|suffix| suffix.starts_with('/'))
}
fn permission_descriptor() -> WorkspaceResult<JsValue> {
    let descriptor = js_sys::Object::new();
    Reflect::set(
        &descriptor,
        &JsValue::from_str("mode"),
        &JsValue::from_str("readwrite"),
    )
    .map_err(|_| WorkspaceError::internal())?;
    Ok(descriptor.into())
}
async fn opfs_root() -> WorkspaceResult<FileSystemDirectoryHandle> {
    if let Some(directory) = LOCAL_ROOT.with(|root| root.borrow().clone()) {
        return Ok(directory);
    }
    let window = web_sys::window().ok_or_else(|| not_available("A browser window is required."))?;
    let value = JsFuture::from(window.navigator().storage().get_directory())
        .await
        .map_err(|error| browser_error("Browser storage is unavailable", error))?;
    Ok(value.unchecked_into())
}
async fn resolve_directory(path: &RelativePath) -> WorkspaceResult<FileSystemDirectoryHandle> {
    let mut directory = opfs_root().await?;
    for segment in segments(path) {
        directory = get_directory_handle(&directory, segment, false).await?;
    }
    Ok(directory)
}
async fn resolve_parent(
    path: &RelativePath,
) -> WorkspaceResult<(FileSystemDirectoryHandle, String)> {
    if path.is_root() {
        return Err(WorkspaceError::new(
            ErrorCode::RootOperationRejected,
            "The workspace root cannot be modified.",
        ));
    }
    let mut parts = segments(path).collect::<Vec<_>>();
    let name = parts
        .pop()
        .ok_or_else(|| WorkspaceError::invalid_path("A file name is required."))?
        .to_owned();
    let mut directory = opfs_root().await?;
    for segment in parts {
        directory = get_directory_handle(&directory, segment, false).await?;
    }
    Ok((directory, name))
}
async fn open_file(path: &RelativePath) -> WorkspaceResult<File> {
    let (parent, name) = resolve_parent(path).await?;
    let handle = get_file_handle(&parent, &name, false).await?;
    let value = JsFuture::from(handle.get_file())
        .await
        .map_err(|error| browser_error("Could not open the file", error))?;
    Ok(value.unchecked_into())
}
async fn get_file_handle(
    directory: &FileSystemDirectoryHandle,
    name: &str,
    create: bool,
) -> WorkspaceResult<FileSystemFileHandle> {
    let options = FileSystemGetFileOptions::new();
    options.set_create(create);
    let value = JsFuture::from(directory.get_file_handle_with_options(name, &options))
        .await
        .map_err(|error| browser_error("Could not open the file entry", error))?;
    Ok(value.unchecked_into())
}
async fn get_directory_handle(
    directory: &FileSystemDirectoryHandle,
    name: &str,
    create: bool,
) -> WorkspaceResult<FileSystemDirectoryHandle> {
    let options = FileSystemGetDirectoryOptions::new();
    options.set_create(create);
    let value = JsFuture::from(directory.get_directory_handle_with_options(name, &options))
        .await
        .map_err(|error| browser_error("Could not open the directory entry", error))?;
    Ok(value.unchecked_into())
}
async fn entry_from_handle(
    parent: &RelativePath,
    handle: FileSystemHandle,
) -> WorkspaceResult<FileEntry> {
    let name = handle.name();
    let path = child_path(parent, &name)?;
    match handle.kind() {
        FileSystemHandleKind::File => {
            entry_from_file_handle(path, name, handle.unchecked_into()).await
        }
        FileSystemHandleKind::Directory => Ok(FileEntry {
            path,
            name,
            kind: EntryKind::Directory,
            size: 0,
            version: None,
        }),
        _ => Err(not_available("This browser file type is not supported.")),
    }
}
async fn entry_from_file_handle(
    path: RelativePath,
    name: String,
    handle: FileSystemFileHandle,
) -> WorkspaceResult<FileEntry> {
    let value = JsFuture::from(handle.get_file())
        .await
        .map_err(|error| browser_error("Could not inspect the file", error))?;
    let file = value.unchecked_into::<File>();
    let version = file_version(&file)?;
    Ok(FileEntry {
        path,
        name,
        kind: EntryKind::File,
        size: version.length,
        version: Some(version),
    })
}
async fn write_file(
    path: &RelativePath,
    content: &[u8],
    expected: Option<&FileVersion>,
) -> WorkspaceResult<FileVersion> {
    if let Some(expected) = expected {
        let current = file_version(&open_file(path).await?)?;
        if &current != expected {
            return Err(WorkspaceError::new(
                ErrorCode::Conflict,
                "The file changed since it was opened.",
            ));
        }
    }
    let (parent, name) = resolve_parent(path).await?;
    let handle = get_file_handle(&parent, &name, true).await?;
    let value = JsFuture::from(handle.create_writable())
        .await
        .map_err(|error| browser_error("Could not open the file for writing", error))?;
    let writable = value.unchecked_into::<FileSystemWritableFileStream>();
    let bytes = Uint8Array::from(content);
    let write = writable
        .write_with_js_u8_array(&bytes)
        .map_err(|error| browser_error("Could not write the file", error))?;
    JsFuture::from(write)
        .await
        .map_err(|error| browser_error("Could not write the file", error))?;
    JsFuture::from(writable.unchecked_ref::<WritableStream>().close())
        .await
        .map_err(|error| browser_error("Could not finish writing the file", error))?;
    file_version(&open_file(path).await?)
}
fn file_version(file: &File) -> WorkspaceResult<FileVersion> {
    let blob = file.unchecked_ref::<Blob>();
    let length = f64_to_u64(blob.size())?;
    let modified_millis = f64_to_u128(file.last_modified())?;
    Ok(FileVersion {
        length,
        modified_unix_nanos: modified_millis.saturating_mul(1_000_000),
    })
}
#[allow(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "browser file metadata is exposed as finite non-negative JavaScript numbers"
)]
fn f64_to_u64(value: f64) -> WorkspaceResult<u64> {
    if !value.is_finite() || value.is_sign_negative() || value > u64::MAX as f64 {
        return Err(WorkspaceError::internal());
    }
    Ok(value as u64)
}
#[allow(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "browser timestamps are exposed as finite non-negative JavaScript numbers"
)]
fn f64_to_u128(value: f64) -> WorkspaceResult<u128> {
    if !value.is_finite() || value.is_sign_negative() || value > u128::MAX as f64 {
        return Err(WorkspaceError::internal());
    }
    Ok(value as u128)
}
fn segments(path: &RelativePath) -> impl Iterator<Item = &str> {
    path.as_str()
        .split('/')
        .filter(|segment| !segment.is_empty())
}
fn child_path(parent: &RelativePath, name: &str) -> WorkspaceResult<RelativePath> {
    let value = if parent.is_root() {
        name.to_owned()
    } else {
        format!("{}/{}", parent.as_str(), name)
    };
    RelativePath::try_from(value)
}
fn entry_rank(kind: EntryKind) -> u8 {
    match kind {
        EntryKind::Directory => 0,
        EntryKind::File => 1,
        EntryKind::Symlink => 2,
    }
}
fn ensure_size(length: u64, maximum: u64) -> WorkspaceResult<()> {
    if length > maximum {
        return Err(WorkspaceError::new(
            ErrorCode::TooLarge,
            format!("The file is larger than the {maximum}-byte guest limit."),
        ));
    }
    Ok(())
}
#[allow(
    clippy::needless_pass_by_value,
    reason = "Promise rejection handlers receive an owned JavaScript handle"
)]
pub(crate) fn browser_error(context: &str, value: JsValue) -> WorkspaceError {
    let detail = value
        .as_string()
        .or_else(|| {
            let message = Reflect::get(&value, &JsValue::from_str("message")).ok()?;
            message.as_string()
        })
        .unwrap_or_else(|| "Unknown browser error".into());
    WorkspaceError::new(ErrorCode::Unavailable, format!("{context}: {detail}"))
}
fn not_available(message: &str) -> WorkspaceError {
    WorkspaceError::new(ErrorCode::Unavailable, message)
}
