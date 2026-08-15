use dioxus::prelude::*;
use dioxus_code_editor::EditorSelection;
use syntaxis_editor::{BufferStatus, EditorBuffer, EditorConfig};
use syntaxis_git::{DiffKind, UnifiedDiff};
use syntaxis_ui::prelude::Tone;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum OpenDocument {
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
    pub(super) fn path(&self) -> &str {
        match self {
            Self::Text(buffer) => &buffer.path,
            Self::Image { path, .. }
            | Self::Large { path, .. }
            | Self::Unsupported { path, .. } => path,
        }
    }

    pub(super) fn label(&self) -> &str {
        self.path().rsplit('/').next().unwrap_or(self.path())
    }

    pub(super) fn is_dirty(&self) -> bool {
        matches!(self, Self::Text(buffer) if buffer.is_dirty())
    }
}

#[derive(Clone, PartialEq)]
pub(super) enum ActiveDocumentView {
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

#[derive(Clone, Debug, PartialEq)]
pub(super) struct OpenTab {
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

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ActiveBufferMeta {
    pub path: String,
    pub status: BufferStatus,
}

impl ActiveBufferMeta {
    pub(super) fn is_dirty(&self) -> bool {
        self.status != BufferStatus::Clean
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FileAction {
    CreateFile,
    CreateFolder,
    Move,
    Duplicate,
    Delete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RevertAction {
    Unsaved,
    Unstaged,
    Original,
}

impl RevertAction {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Unsaved => "Revert Unsaved Changes",
            Self::Unstaged => "Revert Unstaged Changes",
            Self::Original => "Revert to Original",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct GitRevertRequest {
    pub path: String,
    pub action: RevertAction,
}

#[derive(Clone)]
pub(super) struct OpenDiffRequest {
    pub slug: String,
    pub kind: DiffKind,
    pub diff: Signal<Option<UnifiedDiff>>,
    pub toast: Signal<Option<ToastState>>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct FileActionDialog {
    pub action: FileAction,
    pub source: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct CloseRequest {
    pub paths: Vec<String>,
}

#[derive(Clone, PartialEq)]
pub(super) struct ToastState {
    pub message: String,
    pub tone: Tone,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct FilesSessionState {
    pub(super) workspace_id: Signal<Option<String>>,
    pub(super) documents: Signal<Vec<OpenDocument>>,
    pub(super) active_path: Signal<Option<String>>,
    pub(super) editor_selection: Signal<EditorSelection>,
    pub(super) processed_event_revision: Signal<u64>,
}

pub(crate) fn use_files_session() -> FilesSessionState {
    FilesSessionState {
        workspace_id: use_signal(|| None),
        documents: use_signal(Vec::new),
        active_path: use_signal(|| None),
        editor_selection: use_signal(EditorSelection::default),
        processed_event_revision: use_signal(|| 0),
    }
}

impl FilesSessionState {
    pub(crate) fn active_path(self) -> Option<String> {
        (self.active_path)()
    }

    pub(crate) fn active_reference(self) -> Option<String> {
        let path = (self.active_path)()?;
        let documents = self.documents.read();
        let OpenDocument::Text(buffer) =
            documents.iter().find(|document| document.path() == path)?
        else {
            return None;
        };
        let selection = (self.editor_selection)();
        Some(format_reference(&path, &buffer.contents, &selection))
    }

    pub(crate) fn has_dirty(self) -> bool {
        self.documents.read().iter().any(OpenDocument::is_dirty)
    }

    pub(crate) fn reset(mut self) {
        self.workspace_id.set(None);
        self.documents.set(Vec::new());
        self.active_path.set(None);
        self.editor_selection.set(EditorSelection::default());
        self.processed_event_revision.set(0);
    }

    pub(crate) fn activate(mut self, workspace_id: String) {
        if self.workspace_id.peek().as_deref() == Some(&workspace_id) {
            return;
        }
        self.workspace_id.set(Some(workspace_id));
        self.documents.set(Vec::new());
        self.active_path.set(None);
        self.editor_selection.set(EditorSelection::default());
        self.processed_event_revision.set(0);
    }
}

fn format_reference(path: &str, source: &str, selection: &EditorSelection) -> String {
    let start = char_boundary_at_or_before(source, selection.start.min(source.len()));
    let end = char_boundary_at_or_before(source, selection.end.min(source.len()));
    let (start, end) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let (start_line, start_column) = line_column_at(source, start);
    if start == end {
        return format!("{path}:{start_line}:{start_column}");
    }
    let (end_line, end_column) = line_column_at(source, end);
    if start_line == end_line {
        format!("{path}:{start_line}:{start_column}-{end_column}")
    } else {
        format!("{path}:{start_line}:{start_column}-{end_line}:{end_column}")
    }
}

fn char_boundary_at_or_before(source: &str, mut offset: usize) -> usize {
    while offset > 0 && !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn line_column_at(source: &str, offset: usize) -> (usize, usize) {
    let prefix = &source[..offset.min(source.len())];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, tail)| tail)
        .chars()
        .count()
        + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::format_reference;
    use dioxus_code_editor::EditorSelection;

    #[test]
    fn active_reference_includes_multiline_selection() {
        assert_eq!(
            format_reference(
                "src/main.rs",
                "one\ntwø\nthree",
                &EditorSelection {
                    start: 4,
                    end: 9,
                    ..EditorSelection::default()
                },
            ),
            "src/main.rs:2:1-3:1"
        );
    }
}
