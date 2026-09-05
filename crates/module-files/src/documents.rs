use std::collections::{BTreeSet, HashMap};

use dioxus::prelude::{ReadableExt, Signal, WritableExt, use_signal};
use dioxus_code_editor::{EditorEdit, EditorSelection};
use syntaxis_editor::{BufferStatus, EditorBuffer, EditorConfig};
use syntaxis_workspace::{FileSession, WorkspaceId};

use crate::format_file_reference;

/// A document owned by the canonical Files controller.
#[derive(Clone, Debug, PartialEq)]
pub enum OpenDocument {
    Text(EditorBuffer),
    Image {
        path: String,
        data_url: String,
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

impl OpenDocument {
    pub fn path(&self) -> &str {
        match self {
            Self::Text(buffer) => &buffer.path,
            Self::Image { path, .. }
            | Self::Large { path, .. }
            | Self::Unsupported { path, .. } => path,
        }
    }

    pub fn label(&self) -> &str {
        self.path().rsplit('/').next().unwrap_or(self.path())
    }

    pub fn is_dirty(&self) -> bool {
        matches!(self, Self::Text(buffer) if buffer.is_dirty())
    }
}

/// Render-ready snapshot of the active document.
#[derive(Clone, PartialEq)]
pub enum ActiveDocumentView {
    Text {
        path: String,
        contents: String,
        status: BufferStatus,
        config: EditorConfig,
    },
    Image {
        path: String,
        data_url: String,
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

impl From<&OpenDocument> for ActiveDocumentView {
    fn from(document: &OpenDocument) -> Self {
        match document {
            OpenDocument::Text(buffer) => Self::Text {
                path: buffer.path.clone(),
                contents: buffer.contents.clone(),
                status: buffer.status,
                config: buffer.config.clone(),
            },
            OpenDocument::Image {
                path,
                data_url,
                size,
            } => Self::Image {
                path: path.clone(),
                data_url: data_url.clone(),
                size: *size,
            },
            OpenDocument::Large { path, size } => Self::Large {
                path: path.clone(),
                size: *size,
            },
            OpenDocument::Unsupported { path, size, reason } => Self::Unsupported {
                path: path.clone(),
                size: *size,
                reason: reason.clone(),
            },
        }
    }
}

/// Lightweight tab metadata derived from an open document.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenTab {
    pub path: String,
    pub label: String,
    pub dirty: bool,
}

impl From<&OpenDocument> for OpenTab {
    fn from(document: &OpenDocument) -> Self {
        Self {
            path: document.path().to_owned(),
            label: document.label().to_owned(),
            dirty: document.is_dirty(),
        }
    }
}

/// Canonically ordered documents reconstructed from a persisted Files session.
#[derive(Clone, Debug, PartialEq)]
pub struct RestoredDocuments {
    pub documents: Vec<OpenDocument>,
    pub active_path: Option<String>,
}

/// Merges restored tabs with documents opened while restoration was running.
///
/// Current documents win over their restored snapshots so in-flight edits cannot be replaced.
pub fn merge_restored_documents(
    session: FileSession,
    restored: Vec<OpenDocument>,
    current: Vec<OpenDocument>,
) -> RestoredDocuments {
    let active = session
        .active
        .clone()
        .filter(|active| session.tabs.contains(active));
    let mut loaded = restored
        .into_iter()
        .map(|document| (document.path().to_owned(), document))
        .collect::<HashMap<_, _>>();
    let current_paths = current
        .iter()
        .map(|document| document.path().to_owned())
        .collect::<Vec<_>>();
    for document in current {
        loaded.insert(document.path().to_owned(), document);
    }
    let mut documents = session
        .tabs
        .into_iter()
        .filter_map(|path| loaded.remove(&path))
        .collect::<Vec<_>>();
    documents.extend(
        current_paths
            .into_iter()
            .filter_map(|path| loaded.remove(&path)),
    );
    let active_path = active
        .filter(|active| documents.iter().any(|document| document.path() == active))
        .or_else(|| documents.first().map(|document| document.path().to_owned()));
    RestoredDocuments {
        documents,
        active_path,
    }
}

/// Metadata needed by preview and revert controls for the active text buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveBufferMeta {
    pub path: String,
    pub status: BufferStatus,
}

impl ActiveBufferMeta {
    pub fn is_dirty(&self) -> bool {
        self.status != BufferStatus::Clean
    }
}

/// Private Files signal graph owned by the canonical controller.
///
/// Hosts provide this state to the Files route. Peer modules receive only [`crate::FilesUiState`].
#[derive(Clone, Copy, PartialEq)]
pub struct FilesController {
    workspace_id: Signal<Option<WorkspaceId>>,
    pub documents: Signal<Vec<OpenDocument>>,
    pub active_path: Signal<Option<String>>,
    pub editor_selection: Signal<EditorSelection>,
}

/// Creates the private canonical Files controller state.
pub fn use_files_controller() -> FilesController {
    FilesController {
        workspace_id: use_signal(|| None),
        documents: use_signal(Vec::new),
        active_path: use_signal(|| None),
        editor_selection: use_signal(EditorSelection::default),
    }
}

impl FilesController {
    pub fn active_path(self) -> Option<String> {
        (self.active_path)()
    }

    pub fn active_reference(self) -> Option<String> {
        let path = (self.active_path)()?;
        let documents = self.documents.read();
        let OpenDocument::Text(buffer) =
            documents.iter().find(|document| document.path() == path)?
        else {
            return None;
        };
        let selection = (self.editor_selection)();
        Some(format_file_reference(
            &path,
            &buffer.contents,
            selection.start,
            selection.end,
        ))
    }

    pub fn reset(mut self) {
        self.workspace_id.set(None);
        self.documents.set(Vec::new());
        self.active_path.set(None);
        self.editor_selection.set(EditorSelection::default());
    }

    pub fn activate(mut self, workspace_id: WorkspaceId) {
        if self.workspace_id.peek().as_ref() == Some(&workspace_id) {
            return;
        }
        self.workspace_id.set(Some(workspace_id));
        self.documents.set(Vec::new());
        self.active_path.set(None);
        self.editor_selection.set(EditorSelection::default());
    }
}

/// Applies editor byte-range edits to the matching text document.
pub fn apply_document_edits(
    path: &str,
    edits: &[EditorEdit],
    mut documents: Signal<Vec<OpenDocument>>,
) {
    if let Some(OpenDocument::Text(buffer)) = documents
        .write()
        .iter_mut()
        .find(|document| document.path() == path)
    {
        let edits = edits
            .iter()
            .map(|edit| (edit.start, edit.end, edit.text.clone()))
            .collect::<Vec<_>>();
        buffer.apply_edits(&edits);
    }
}

/// Restores one text document to its last saved contents.
pub fn revert_text_document(path: Option<String>, mut documents: Signal<Vec<OpenDocument>>) {
    let Some(path) = path else {
        return;
    };
    if let Some(OpenDocument::Text(buffer)) = documents
        .write()
        .iter_mut()
        .find(|document| document.path() == path)
    {
        buffer.revert();
    }
}

/// Renames open documents and the active path after a workspace entry move.
pub fn rename_documents(
    source: &str,
    destination: &str,
    mut documents: Signal<Vec<OpenDocument>>,
    mut active_path: Signal<Option<String>>,
) {
    let source_prefix = format!("{source}/");
    for document in documents.write().iter_mut() {
        let current = document.path().to_owned();
        if current == source || current.starts_with(&source_prefix) {
            let next = format!("{destination}{}", &current[source.len()..]);
            match document {
                OpenDocument::Text(buffer) => buffer.rename(next),
                OpenDocument::Image { path, .. }
                | OpenDocument::Large { path, .. }
                | OpenDocument::Unsupported { path, .. } => *path = next,
            }
        }
    }
    if let Some(active) = active_path()
        && (active == source || active.starts_with(&source_prefix))
    {
        active_path.set(Some(format!("{destination}{}", &active[source.len()..])));
    }
}

/// A group of paths awaiting confirmation before dirty documents are closed.
#[derive(Clone, Debug, PartialEq)]
pub struct CloseRequest {
    pub paths: Vec<String>,
}

/// Requests one document close, deferring dirty documents for confirmation.
pub fn request_close(
    path: String,
    documents: Signal<Vec<OpenDocument>>,
    active_path: Signal<Option<String>>,
    close_request: Signal<Option<CloseRequest>>,
) {
    request_close_many(vec![path], documents, active_path, close_request);
}

/// Requests multiple document closes, deferring the group when any document is dirty.
pub fn request_close_many(
    paths: Vec<String>,
    documents: Signal<Vec<OpenDocument>>,
    active_path: Signal<Option<String>>,
    mut close_request: Signal<Option<CloseRequest>>,
) {
    if paths.is_empty() {
        return;
    }
    let paths_to_close = paths.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let has_dirty_document = documents
        .read()
        .iter()
        .any(|document| paths_to_close.contains(document.path()) && document.is_dirty());
    if has_dirty_document {
        close_request.set(Some(CloseRequest { paths }));
    } else {
        close_documents(&paths, documents, active_path);
    }
}

/// Closes documents and selects the last remaining tab when the active tab closes.
pub fn close_documents(
    paths: &[String],
    mut documents: Signal<Vec<OpenDocument>>,
    mut active_path: Signal<Option<String>>,
) {
    let paths_to_close = paths.iter().map(String::as_str).collect::<BTreeSet<_>>();
    documents
        .write()
        .retain(|document| !paths_to_close.contains(document.path()));
    if active_path()
        .as_ref()
        .is_some_and(|active| paths.contains(active))
    {
        active_path.set(
            documents
                .read()
                .last()
                .map(|document| document.path().to_owned()),
        );
    }
}

/// Required repair when the active path no longer names an open document.
#[derive(Debug, PartialEq)]
pub enum ActivePathRepair {
    Unchanged,
    Replace(Option<String>),
}

/// Determines how to repair an active path after document changes.
pub fn repaired_active_path(active: Option<&str>, documents: &[OpenDocument]) -> ActivePathRepair {
    let Some(active) = active else {
        return ActivePathRepair::Unchanged;
    };
    if documents.iter().any(|document| document.path() == active) {
        return ActivePathRepair::Unchanged;
    }
    ActivePathRepair::Replace(documents.last().map(|document| document.path().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_text_document_metadata_is_stable() {
        let document = OpenDocument::Large {
            path: "target/build.log".to_owned(),
            size: 42,
        };

        assert_eq!(document.path(), "target/build.log");
        assert_eq!(document.label(), "build.log");
        assert!(!document.is_dirty());
        assert_eq!(
            OpenTab::from(&document),
            OpenTab {
                path: "target/build.log".to_owned(),
                label: "build.log".to_owned(),
                dirty: false,
            }
        );
    }

    #[test]
    fn invalid_active_path_falls_back_to_the_last_open_document() {
        let documents = vec![
            OpenDocument::Large {
                path: "one.bin".to_owned(),
                size: 1,
            },
            OpenDocument::Large {
                path: "two.bin".to_owned(),
                size: 2,
            },
        ];

        assert_eq!(
            repaired_active_path(Some("missing.bin"), &documents),
            ActivePathRepair::Replace(Some("two.bin".to_owned()))
        );
        assert_eq!(
            repaired_active_path(Some("one.bin"), &documents),
            ActivePathRepair::Unchanged
        );
        assert_eq!(
            repaired_active_path(None, &documents),
            ActivePathRepair::Unchanged
        );
        assert_eq!(
            repaired_active_path(Some("missing.bin"), &[]),
            ActivePathRepair::Replace(None)
        );
    }

    #[test]
    fn restored_order_preserves_current_documents_and_repairs_the_active_tab() {
        let restored = vec![
            OpenDocument::Large {
                path: "one.bin".to_owned(),
                size: 1,
            },
            OpenDocument::Large {
                path: "two.bin".to_owned(),
                size: 2,
            },
        ];
        let current = vec![
            OpenDocument::Large {
                path: "two.bin".to_owned(),
                size: 20,
            },
            OpenDocument::Large {
                path: "three.bin".to_owned(),
                size: 3,
            },
        ];
        let merged = merge_restored_documents(
            FileSession {
                tabs: vec!["one.bin".into(), "missing.bin".into(), "two.bin".into()],
                active: Some("missing.bin".into()),
            },
            restored,
            current,
        );

        assert_eq!(
            merged
                .documents
                .iter()
                .map(OpenDocument::path)
                .collect::<Vec<_>>(),
            vec!["one.bin", "two.bin", "three.bin"]
        );
        assert!(matches!(
            &merged.documents[1],
            OpenDocument::Large { size: 20, .. }
        ));
        assert_eq!(merged.active_path.as_deref(), Some("one.bin"));
    }
}
