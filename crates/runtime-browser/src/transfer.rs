use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use async_trait::async_trait;
use dioxus::prelude::document;
use serde::{Deserialize, Serialize};
use syntaxis_app_contracts::{
    AppError, AppErrorCode, ChangeOrigin, ErrorSource, RetryAdvice, WorkspaceEventBus,
};
use syntaxis_module_files::{
    LocalFolderAccess, LocalFolderPermissionPort, TransferSummary, WorkspaceArchive,
    WorkspaceTransferPort,
};
use syntaxis_workspace::{
    EntryKind, ErrorCode, RelativePath, WorkspaceAvailability, WorkspaceFiles, WorkspaceIcon,
    WorkspaceIconSymbol, WorkspaceId, WorkspaceProfile, WorkspaceRecord, WorkspaceSection,
};
use syntaxis_workspace_browser::{
    OpfsWorkspaceFiles, SavedDirectory, local_directory_picker_supported, restore_local_directory,
    select_local_directory, set_private_workspace,
};

use crate::bridge::{BrowserBridge, ensure_bridge};

const MAX_ARCHIVE_FILES: usize = 10_000;
const MAX_ARCHIVE_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ARCHIVE_WORKSPACE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone)]
pub struct BrowserWorkspaceTransfer {
    events: WorkspaceEventBus,
    generation: Arc<AtomicU64>,
}

impl BrowserWorkspaceTransfer {
    pub fn new(events: WorkspaceEventBus) -> Self {
        Self {
            events,
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    fn begin(&self) -> u64 {
        self.generation
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1)
    }

    fn check(&self, generation: u64) -> Result<(), AppError> {
        if self.generation.load(Ordering::Relaxed) == generation {
            Ok(())
        } else {
            Err(transfer_error(
                AppErrorCode::Cancelled,
                "The archive operation was cancelled.",
            ))
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveEntry {
    path: String,
    #[serde(default)]
    directory: bool,
    content: Vec<u8>,
}

#[derive(Deserialize)]
struct ArchiveBridgeResponse<T> {
    ok: bool,
    value: Option<T>,
    error: Option<String>,
    #[serde(default)]
    unavailable: bool,
}

#[async_trait(?Send)]
impl WorkspaceTransferPort for BrowserWorkspaceTransfer {
    fn cancel(&self) -> Result<(), AppError> {
        self.generation.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn import_archive(
        &self,
        workspace: &WorkspaceRecord,
        bytes: Vec<u8>,
    ) -> Result<TransferSummary, AppError> {
        let generation = self.begin();
        if bytes.len() as u64 > MAX_ARCHIVE_WORKSPACE_BYTES {
            return Err(transfer_error(
                AppErrorCode::TooLarge,
                "The ZIP file exceeds the 32 MiB limit.",
            ));
        }
        ensure_bridge(BrowserBridge::Archive)
            .await
            .map_err(|message| transfer_error(AppErrorCode::Offline, message))?;
        let mut eval = document::eval(
            r#"
            const bytes = await dioxus.recv();
            const bridge = globalThis.SyntaxisGuestArchive;
            if (!bridge || bridge.version !== 1) {
              await dioxus.send({
                ok: false,
                unavailable: true,
                error: "The ZIP archive bridge is unavailable or incompatible.",
              });
            } else {
              try {
                await dioxus.send({ ok: true, value: bridge.importZip(bytes) });
              } catch (error) {
                await dioxus.send({ ok: false, error: error?.message ?? String(error) });
              }
            }
        "#,
        );
        eval.send(bytes)
            .map_err(|error| transfer_error(AppErrorCode::Internal, error.to_string()))?;
        let response = eval
            .recv::<ArchiveBridgeResponse<Vec<ArchiveEntry>>>()
            .await
            .map_err(|error| {
                transfer_error(
                    AppErrorCode::Internal,
                    format!("The ZIP archive bridge returned invalid data: {error}"),
                )
            })?;
        let entries = archive_bridge_result(response, AppErrorCode::InvalidInput)?;
        self.check(generation)?;
        let applied = apply_archive_entries(
            &OpfsWorkspaceFiles,
            workspace,
            entries,
            &self.generation,
            generation,
        )
        .await;
        if applied.is_err() {
            // Validation failures do not mutate the workspace, but an I/O failure or
            // cancellation can happen after some entries were written. A resync is
            // intentionally cheap here and keeps open buffers/explorer state honest.
            self.events
                .publish_resync(workspace.id.clone(), None, ChangeOrigin::Files);
        }
        let summary = applied?;
        self.check(generation)?;
        self.events
            .publish_resync(workspace.id.clone(), None, ChangeOrigin::Files);
        Ok(summary)
    }

    async fn export_archive(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<WorkspaceArchive, AppError> {
        let generation = self.begin();
        let entries =
            collect_archive_entries(&OpfsWorkspaceFiles, workspace, &self.generation, generation)
                .await?;
        self.check(generation)?;
        ensure_bridge(BrowserBridge::Archive)
            .await
            .map_err(|message| transfer_error(AppErrorCode::Offline, message))?;
        let mut eval = document::eval(
            r#"
            const entries = await dioxus.recv();
            const bridge = globalThis.SyntaxisGuestArchive;
            if (!bridge || bridge.version !== 1) {
              await dioxus.send({
                ok: false,
                unavailable: true,
                error: "The ZIP archive bridge is unavailable or incompatible.",
              });
            } else {
              try {
                await dioxus.send({ ok: true, value: Array.from(bridge.exportZip(entries)) });
              } catch (error) {
                await dioxus.send({ ok: false, error: error?.message ?? String(error) });
              }
            }
        "#,
        );
        eval.send(entries)
            .map_err(|error| transfer_error(AppErrorCode::Internal, error.to_string()))?;
        let response = eval
            .recv::<ArchiveBridgeResponse<Vec<u8>>>()
            .await
            .map_err(|error| {
                transfer_error(
                    AppErrorCode::Internal,
                    format!("The ZIP archive bridge returned invalid data: {error}"),
                )
            })?;
        let bytes = archive_bridge_result(response, AppErrorCode::Internal)?;
        self.check(generation)?;
        Ok(WorkspaceArchive {
            filename: format!("{}.zip", workspace.slug),
            bytes,
        })
    }
}

async fn collect_archive_entries(
    files: &OpfsWorkspaceFiles,
    workspace: &WorkspaceRecord,
    cancellation: &AtomicU64,
    generation: u64,
) -> Result<Vec<ArchiveEntry>, AppError> {
    let mut pending = vec![RelativePath::root()];
    let mut entries = Vec::new();
    let mut total_bytes = 0_u64;
    while let Some(directory) = pending.pop() {
        check_generation(cancellation, generation)?;
        let listed = files
            .list(workspace, &directory)
            .await
            .map_err(AppError::from)?;
        for entry in listed {
            check_generation(cancellation, generation)?;
            if reserved_path(entry.path.as_str()) {
                continue;
            }
            if entries.len() >= MAX_ARCHIVE_FILES {
                return Err(transfer_error(
                    AppErrorCode::TooLarge,
                    "The workspace contains too many entries for a ZIP export.",
                ));
            }
            match entry.kind {
                EntryKind::Directory => {
                    entries.push(ArchiveEntry {
                        path: format!("{}/", entry.path.as_str()),
                        directory: true,
                        content: Vec::new(),
                    });
                    pending.push(entry.path);
                }
                EntryKind::File => {
                    if entry.size > MAX_ARCHIVE_FILE_BYTES {
                        return Err(transfer_error(
                            AppErrorCode::TooLarge,
                            format!(
                                "{} exceeds the 8 MiB ZIP export file limit.",
                                entry.path.as_str()
                            ),
                        ));
                    }
                    let file = files
                        .read_binary(workspace, &entry.path, MAX_ARCHIVE_FILE_BYTES)
                        .await
                        .map_err(AppError::from)?;
                    total_bytes = total_bytes
                        .saturating_add(u64::try_from(file.content.len()).unwrap_or(u64::MAX));
                    if total_bytes > MAX_ARCHIVE_WORKSPACE_BYTES {
                        return Err(transfer_error(
                            AppErrorCode::TooLarge,
                            "The workspace exceeds the 32 MiB ZIP export limit.",
                        ));
                    }
                    entries.push(ArchiveEntry {
                        path: entry.path.as_str().to_owned(),
                        directory: false,
                        content: file.content,
                    });
                }
                EntryKind::Symlink => {}
            }
        }
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

#[async_trait(?Send)]
impl LocalFolderPermissionPort for BrowserWorkspaceTransfer {
    fn picker_supported(&self) -> bool {
        local_directory_picker_supported()
    }

    async fn select(&self) -> Result<WorkspaceRecord, AppError> {
        let selected = select_local_directory().await.map_err(AppError::from)?;
        Ok(browser_workspace(
            slug_for_project(&selected.name),
            selected.name,
        ))
    }

    async fn restore(&self, request_access: bool) -> Result<LocalFolderAccess, AppError> {
        restore_local_directory(request_access)
            .await
            .map(|saved| match saved {
                SavedDirectory::Active(name) => LocalFolderAccess::Active { name },
                SavedDirectory::NeedsPermission(name) => {
                    LocalFolderAccess::PermissionRequired { name }
                }
                SavedDirectory::Missing => LocalFolderAccess::Missing,
            })
            .map_err(AppError::from)
    }

    fn use_private_storage(&self) -> WorkspaceRecord {
        set_private_workspace();
        browser_workspace("browser".into(), "Browser workspace".into())
    }
}

async fn apply_archive_entries(
    files: &OpfsWorkspaceFiles,
    workspace: &WorkspaceRecord,
    entries: Vec<ArchiveEntry>,
    cancellation: &AtomicU64,
    generation: u64,
) -> Result<TransferSummary, AppError> {
    if entries.len() > MAX_ARCHIVE_FILES {
        return Err(transfer_error(
            AppErrorCode::TooLarge,
            "The ZIP contains too many entries.",
        ));
    }
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(entries.len());
    let mut total_bytes = 0_u64;
    for entry in entries {
        check_generation(cancellation, generation)?;
        let path =
            RelativePath::try_from(entry.path.trim_end_matches('/').to_owned()).map_err(|_| {
                transfer_error(
                    AppErrorCode::InvalidInput,
                    format!("The ZIP contains an invalid path: {}", entry.path),
                )
            })?;
        if path.is_root() || reserved_path(path.as_str()) || !seen.insert(path.as_str().to_owned())
        {
            return Err(transfer_error(
                AppErrorCode::InvalidInput,
                format!(
                    "The ZIP contains a reserved, duplicate, or root path: {}",
                    entry.path
                ),
            ));
        }
        let size = u64::try_from(entry.content.len()).unwrap_or(u64::MAX);
        if size > MAX_ARCHIVE_FILE_BYTES
            || total_bytes.saturating_add(size) > MAX_ARCHIVE_WORKSPACE_BYTES
        {
            return Err(transfer_error(
                AppErrorCode::TooLarge,
                "The ZIP exceeds the 8 MiB file or 32 MiB workspace limit.",
            ));
        }
        total_bytes = total_bytes.saturating_add(size);
        normalized.push((path, entry.directory, entry.content));
    }
    normalized.sort_by_key(|(path, directory, _)| {
        (
            if *directory { 0 } else { 1 },
            path.as_str().matches('/').count(),
        )
    });
    for (path, _, _) in &normalized {
        check_generation(cancellation, generation)?;
        if files.stat(workspace, path).await.is_ok() {
            return Err(transfer_error(
                AppErrorCode::Conflict,
                format!("The destination already exists: {}", path.as_str()),
            ));
        }
    }
    for (path, directory, content) in &normalized {
        check_generation(cancellation, generation)?;
        ensure_parent(files, workspace, path).await?;
        if *directory {
            files
                .create_directory(workspace, path)
                .await
                .map_err(AppError::from)?;
        } else {
            files
                .write_binary(workspace, path, content, MAX_ARCHIVE_FILE_BYTES)
                .await
                .map_err(AppError::from)?;
        }
    }
    Ok(TransferSummary {
        entries: normalized.len(),
        bytes: total_bytes,
    })
}

fn check_generation(cancellation: &AtomicU64, generation: u64) -> Result<(), AppError> {
    if cancellation.load(Ordering::Relaxed) == generation {
        Ok(())
    } else {
        Err(transfer_error(
            AppErrorCode::Cancelled,
            "The archive operation was cancelled.",
        ))
    }
}

async fn ensure_parent(
    files: &OpfsWorkspaceFiles,
    workspace: &WorkspaceRecord,
    path: &RelativePath,
) -> Result<(), AppError> {
    let mut segments = path.as_str().split('/').collect::<Vec<_>>();
    segments.pop();
    let mut current = String::new();
    for segment in segments {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(segment);
        let parent = RelativePath::try_from(current.clone()).map_err(AppError::from)?;
        match files.stat(workspace, &parent).await {
            Ok(entry) if entry.kind == EntryKind::Directory => {}
            Ok(_) => {
                return Err(transfer_error(
                    AppErrorCode::Conflict,
                    format!("A ZIP parent is not a directory: {current}"),
                ));
            }
            Err(error) if error.code == ErrorCode::NotFound => {
                files
                    .create_directory(workspace, &parent)
                    .await
                    .map_err(AppError::from)?;
            }
            Err(error) => return Err(AppError::from(error)),
        }
    }
    Ok(())
}

fn reserved_path(path: &str) -> bool {
    path == ".syntaxis-guest-history.json" || path == ".git" || path.starts_with(".git/")
}

fn browser_workspace(slug: String, name: String) -> WorkspaceRecord {
    WorkspaceRecord {
        id: WorkspaceId::new("browser-opfs"),
        slug,
        name,
        root: "opfs://syntaxis-guest".into(),
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

fn slug_for_project(name: &str) -> String {
    let slug = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "browser".into()
    } else {
        slug.into()
    }
}

fn transfer_error(code: AppErrorCode, message: impl Into<String>) -> AppError {
    let retry = match code {
        AppErrorCode::Internal | AppErrorCode::Offline => RetryAdvice::Backoff,
        AppErrorCode::Conflict | AppErrorCode::PermissionDenied => RetryAdvice::AfterUserAction,
        _ => RetryAdvice::Never,
    };
    AppError::new(code, message, retry, ErrorSource::Files)
}

fn archive_bridge_result<T>(
    response: ArchiveBridgeResponse<T>,
    operation_error: AppErrorCode,
) -> Result<T, AppError> {
    if response.ok {
        return response.value.ok_or_else(|| {
            transfer_error(
                AppErrorCode::Internal,
                "The ZIP archive bridge returned no result.",
            )
        });
    }
    let code = if response.unavailable {
        AppErrorCode::Offline
    } else {
        operation_error
    };
    Err(transfer_error(
        code,
        response
            .error
            .unwrap_or_else(|| "The ZIP archive operation failed.".to_owned()),
    ))
}
