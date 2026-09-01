use std::collections::VecDeque;
use syntaxis_workspace::{EntryKind, FileEntry, WorkspaceFiles, WorkspaceRecord, WorkspaceResult};
const MAX_RESULTS: usize = 100;
const MAX_SCANNED_CONTENT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_FILE_CONTENT_BYTES: u64 = 1024 * 1024;
/// A workspace entry matching a browser-local search query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserSearchHit {
    /// The matching workspace entry.
    pub entry: FileEntry,
    /// Whether the query was found in the file contents as well as its path.
    pub content_match: bool,
}
/// Searches names and small text files in the browser workspace.
///
/// Traversal and matching stay in Rust. The bounded scan prevents a broad
/// query from reading an unbounded amount of browser storage into WASM.
pub async fn search<F>(
    files: &F,
    workspace: &WorkspaceRecord,
    query: &str,
) -> WorkspaceResult<Vec<BrowserSearchHit>>
where
    F: WorkspaceFiles,
{
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let mut pending = VecDeque::from([syntaxis_workspace::RelativePath::root()]);
    let mut hits = Vec::new();
    let mut scanned_bytes = 0_u64;
    while let Some(directory) = pending.pop_front() {
        for entry in files.list(workspace, &directory).await? {
            match entry.kind {
                EntryKind::Directory => pending.push_back(entry.path.clone()),
                EntryKind::File => {
                    let name_match = entry.path.as_str().to_lowercase().contains(&query);
                    let content_match = if !name_match
                        && entry.size <= MAX_FILE_CONTENT_BYTES
                        && scanned_bytes.saturating_add(entry.size) <= MAX_SCANNED_CONTENT_BYTES
                    {
                        scanned_bytes = scanned_bytes.saturating_add(entry.size);
                        files
                            .read_text(workspace, &entry.path, MAX_FILE_CONTENT_BYTES)
                            .await
                            .is_ok_and(|file| file.content.to_lowercase().contains(&query))
                    } else {
                        false
                    };
                    if name_match || content_match {
                        hits.push(BrowserSearchHit {
                            entry,
                            content_match,
                        });
                        if hits.len() >= MAX_RESULTS {
                            return Ok(hits);
                        }
                    }
                }
                EntryKind::Symlink => {}
            }
        }
    }
    hits.sort_by(|left, right| left.entry.path.as_str().cmp(right.entry.path.as_str()));
    Ok(hits)
}
