use syntaxis_editor::{EditorBuffer, EditorConfigSource, resolve_editor_config};
use syntaxis_workspace::{FileEntry, RelativePath, WorkspaceRecord};

use crate::{FilesPorts, MAX_TEXT_BYTES};

/// Maximum binary payload loaded for the canonical in-editor image preview.
pub const MAX_BINARY_PREVIEW_BYTES: u64 = 4 * 1024 * 1024;

/// Runtime-neutral result of classifying and loading a workspace document.
#[derive(Clone, Debug, PartialEq)]
pub enum DocumentLoad {
    Text(EditorBuffer),
    Image {
        path: String,
        mime: &'static str,
        content: Vec<u8>,
        size: u64,
    },
    Large {
        path: String,
        size: u64,
    },
    Unsupported {
        path: String,
        size: u64,
        reason: String,
    },
}

impl DocumentLoad {
    pub fn path(&self) -> &str {
        match self {
            Self::Text(buffer) => &buffer.path,
            Self::Image { path, .. }
            | Self::Large { path, .. }
            | Self::Unsupported { path, .. } => path,
        }
    }
}

/// Returns the canonical safe raster-image MIME type for a workspace path.
pub fn image_mime(path: &str) -> Option<&'static str> {
    match path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("webp") => Some("image/webp"),
        Some("bmp") => Some("image/bmp"),
        Some("ico") => Some("image/x-icon"),
        _ => None,
    }
}

/// Classifies and loads one document through the injected Files port.
pub async fn load_document_content(
    files: &FilesPorts,
    workspace: &WorkspaceRecord,
    entry: FileEntry,
    configs: &[EditorConfigSource],
) -> DocumentLoad {
    let path = entry.path.as_str().to_owned();
    if entry.size > MAX_TEXT_BYTES {
        return DocumentLoad::Large {
            path,
            size: entry.size,
        };
    }
    let loaded = if let Some(mime) = image_mime(&path) {
        files
            .files()
            .read_binary(workspace, &entry.path, MAX_BINARY_PREVIEW_BYTES)
            .await
            .map(|file| DocumentLoad::Image {
                path: path.clone(),
                mime,
                content: file.content,
                size: entry.size,
            })
    } else {
        files
            .files()
            .read_text(workspace, &entry.path, MAX_TEXT_BYTES)
            .await
            .map(|file| {
                DocumentLoad::Text(EditorBuffer::open(
                    path.clone(),
                    file.content,
                    file.version,
                    resolve_editor_config(configs, &path),
                ))
            })
    };
    loaded.unwrap_or_else(|error| DocumentLoad::Unsupported {
        path,
        size: entry.size,
        reason: error.message,
    })
}

/// Loads a persisted tab when its path still exists in the workspace.
pub async fn load_restored_document_content(
    files: &FilesPorts,
    workspace: &WorkspaceRecord,
    configs: &[EditorConfigSource],
    path: &str,
) -> Option<DocumentLoad> {
    let relative = RelativePath::try_from(path.to_owned()).ok()?;
    let entry = files.files().stat(workspace, &relative).await.ok()?;
    Some(load_document_content(files, workspace, entry, configs).await)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures_lite::future::block_on;
    use syntaxis_workspace::{
        MockWorkspaceFiles, WorkspaceAvailability, WorkspaceFiles, WorkspaceIcon,
        WorkspaceIconSymbol, WorkspaceId, WorkspaceProfile, WorkspaceSection,
    };

    use crate::{FixedWorkspaceSearch, MemoryFilesSession};

    use super::*;

    fn workspace() -> WorkspaceRecord {
        WorkspaceRecord {
            id: WorkspaceId::new("document-loading"),
            slug: "document-loading".into(),
            name: "Document loading".into(),
            root: "/document-loading".into(),
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

    #[test]
    fn text_documents_load_and_missing_tabs_are_skipped() {
        let workspace = workspace();
        let adapter = Arc::new(MockWorkspaceFiles::default());
        let path = RelativePath::try_from("src/main.rs").unwrap();
        adapter
            .insert_text(&workspace, &path, "fn main() {}")
            .unwrap();
        let entry = block_on(adapter.stat(&workspace, &path)).unwrap();
        let files = FilesPorts::new(
            adapter,
            Arc::new(FixedWorkspaceSearch::default()),
            Arc::new(MemoryFilesSession::default()),
        );

        let loaded = block_on(load_document_content(&files, &workspace, entry, &[]));
        assert!(matches!(
            loaded,
            DocumentLoad::Text(buffer)
                if buffer.path == "src/main.rs" && buffer.contents == "fn main() {}"
        ));
        assert!(
            block_on(load_restored_document_content(
                &files,
                &workspace,
                &[],
                "missing.rs",
            ))
            .is_none()
        );
    }

    #[test]
    fn image_detection_is_explicit_and_case_insensitive() {
        assert_eq!(image_mime("assets/photo.PNG"), Some("image/png"));
        assert_eq!(image_mime("assets/vector.svg"), None);
        assert_eq!(image_mime("archive.bin"), None);
    }
}
