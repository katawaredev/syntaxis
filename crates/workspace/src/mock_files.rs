use std::{collections::HashMap, sync::Mutex};

use async_trait::async_trait;

use crate::{
    BinaryFile, EntryKind, ErrorCode, FileEntry, FileVersion, RelativePath, TextFile,
    WorkspaceError, WorkspaceFiles, WorkspaceRecord, WorkspaceResult,
};

#[derive(Clone, Debug)]
enum MockNode {
    Directory,
    Text { content: String, revision: u128 },
}

#[derive(Default)]
pub struct MockWorkspaceFiles {
    nodes: Mutex<HashMap<(String, String), MockNode>>,
}

impl MockWorkspaceFiles {
    /// Adds a text file to the mock workspace, creating missing parent directories.
    ///
    /// # Errors
    ///
    /// Returns an internal error if the mock store lock is poisoned.
    pub fn insert_text(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        content: impl Into<String>,
    ) -> WorkspaceResult<()> {
        self.ensure_parents(workspace, path)?;
        self.lock()?.insert(
            key(workspace, path),
            MockNode::Text {
                content: content.into(),
                revision: 1,
            },
        );
        Ok(())
    }

    fn ensure_parents(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<()> {
        let mut nodes = self.lock()?;
        let mut parent = String::new();
        let parts = path.as_str().split('/').collect::<Vec<_>>();
        for part in parts.iter().take(parts.len().saturating_sub(1)) {
            parent = join(&parent, part);
            nodes
                .entry((workspace.id.0.clone(), parent.clone()))
                .or_insert(MockNode::Directory);
        }
        Ok(())
    }

    fn lock(
        &self,
    ) -> WorkspaceResult<std::sync::MutexGuard<'_, HashMap<(String, String), MockNode>>> {
        self.nodes
            .lock()
            .map_err(|_poison_error| WorkspaceError::internal())
    }
}

#[async_trait(?Send)]
impl WorkspaceFiles for MockWorkspaceFiles {
    async fn list(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<Vec<FileEntry>> {
        let nodes = self.lock()?;
        require_directory(&nodes, workspace, path)?;
        let mut entries = nodes
            .iter()
            .filter(|((id, candidate), _)| {
                id == &workspace.id.0 && parent(candidate) == path.as_str()
            })
            .filter_map(|((_, candidate), node)| {
                let relative = RelativePath::try_from(candidate.clone()).ok()?;
                Some(entry(relative, node))
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| (entry.kind != EntryKind::Directory, entry.name.clone()));
        Ok(entries)
    }

    async fn stat(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        if path.is_root() {
            return Ok(entry(path.clone(), &MockNode::Directory));
        }
        self.lock()?
            .get(&key(workspace, path))
            .map(|node| entry(path.clone(), node))
            .ok_or_else(not_found)
    }

    async fn read_text(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        max_bytes: u64,
    ) -> WorkspaceResult<TextFile> {
        match self.lock()?.get(&key(workspace, path)) {
            Some(MockNode::Text { content, revision }) => {
                require_size(content.len(), max_bytes)?;
                Ok(TextFile {
                    content: content.clone(),
                    version: version(content, *revision),
                })
            }
            Some(MockNode::Directory) => Err(WorkspaceError::invalid_path(
                "The mock path is a directory.",
            )),
            None => Err(not_found()),
        }
    }

    async fn read_binary(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        max_bytes: u64,
    ) -> WorkspaceResult<BinaryFile> {
        let text = self.read_text(workspace, path, max_bytes).await?;
        Ok(BinaryFile {
            content: text.content.into_bytes(),
            version: text.version,
        })
    }

    async fn create_file(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        create_node(
            self,
            workspace,
            path,
            MockNode::Text {
                content: String::new(),
                revision: 1,
            },
        )
    }

    async fn create_directory(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        create_node(self, workspace, path, MockNode::Directory)
    }

    async fn copy(
        &self,
        workspace: &WorkspaceRecord,
        source: &RelativePath,
        destination: &RelativePath,
    ) -> WorkspaceResult<()> {
        mutate_tree(self, workspace, source, destination, false)
    }

    async fn move_entry(
        &self,
        workspace: &WorkspaceRecord,
        source: &RelativePath,
        destination: &RelativePath,
    ) -> WorkspaceResult<()> {
        mutate_tree(self, workspace, source, destination, true)
    }

    async fn delete(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<()> {
        reject_root(path)?;
        let mut nodes = self.lock()?;
        let before = nodes.len();
        let prefix = format!("{}/", path.as_str());
        nodes.retain(|(id, candidate), _| {
            id != &workspace.id.0 || (candidate != path.as_str() && !candidate.starts_with(&prefix))
        });
        if nodes.len() == before {
            Err(not_found())
        } else {
            Ok(())
        }
    }

    async fn write_text(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        content: &str,
        expected: Option<&FileVersion>,
        max_bytes: u64,
    ) -> WorkspaceResult<FileVersion> {
        require_size(content.len(), max_bytes)?;
        let mut nodes = self.lock()?;
        let node = nodes.get_mut(&key(workspace, path)).ok_or_else(not_found)?;
        let MockNode::Text {
            content: current,
            revision,
        } = node
        else {
            return Err(WorkspaceError::invalid_path(
                "The mock path is a directory.",
            ));
        };
        if expected.is_some_and(|expected| &version(current, *revision) != expected) {
            return Err(WorkspaceError::new(
                ErrorCode::Conflict,
                "The mock file changed.",
            ));
        }
        *revision = revision.saturating_add(1);
        content.clone_into(current);
        Ok(version(current, *revision))
    }
}

fn create_node(
    files: &MockWorkspaceFiles,
    workspace: &WorkspaceRecord,
    path: &RelativePath,
    node: MockNode,
) -> WorkspaceResult<FileEntry> {
    reject_root(path)?;
    let mut nodes = files.lock()?;
    require_parent(&nodes, workspace, path)?;
    if nodes.contains_key(&key(workspace, path)) {
        return Err(WorkspaceError::new(
            ErrorCode::AlreadyExists,
            "The mock path exists.",
        ));
    }
    let result = entry(path.clone(), &node);
    nodes.insert(key(workspace, path), node);
    Ok(result)
}

fn mutate_tree(
    files: &MockWorkspaceFiles,
    workspace: &WorkspaceRecord,
    source: &RelativePath,
    destination: &RelativePath,
    remove_source: bool,
) -> WorkspaceResult<()> {
    reject_root(source)?;
    reject_root(destination)?;
    let mut nodes = files.lock()?;
    require_parent(&nodes, workspace, destination)?;
    if nodes.contains_key(&key(workspace, destination)) {
        return Err(WorkspaceError::new(
            ErrorCode::AlreadyExists,
            "The mock destination exists.",
        ));
    }
    let source_prefix = format!("{}/", source.as_str());
    let copied = nodes
        .iter()
        .filter(|((id, path), _)| {
            id == &workspace.id.0 && (path == source.as_str() || path.starts_with(&source_prefix))
        })
        .map(|((_, path), node)| {
            let suffix = path
                .strip_prefix(source.as_str())
                .unwrap_or_default()
                .trim_start_matches('/');
            (
                (workspace.id.0.clone(), join(destination.as_str(), suffix)),
                node.clone(),
            )
        })
        .collect::<Vec<_>>();
    if copied.is_empty() {
        return Err(not_found());
    }
    if remove_source {
        nodes.retain(|(id, path), _| {
            id != &workspace.id.0 || (path != source.as_str() && !path.starts_with(&source_prefix))
        });
    }
    nodes.extend(copied);
    Ok(())
}

fn require_parent(
    nodes: &HashMap<(String, String), MockNode>,
    workspace: &WorkspaceRecord,
    path: &RelativePath,
) -> WorkspaceResult<()> {
    let parent_path = parent(path.as_str());
    if parent_path.is_empty() {
        return Ok(());
    }
    match nodes.get(&(workspace.id.0.clone(), parent_path.to_owned())) {
        Some(MockNode::Directory) => Ok(()),
        _ => Err(WorkspaceError::invalid_path(
            "The mock parent directory does not exist.",
        )),
    }
}

fn require_directory(
    nodes: &HashMap<(String, String), MockNode>,
    workspace: &WorkspaceRecord,
    path: &RelativePath,
) -> WorkspaceResult<()> {
    if path.is_root() {
        return Ok(());
    }
    match nodes.get(&key(workspace, path)) {
        Some(MockNode::Directory) => Ok(()),
        Some(MockNode::Text { .. }) => Err(WorkspaceError::invalid_path(
            "The mock path is not a directory.",
        )),
        None => Err(not_found()),
    }
}

fn entry(path: RelativePath, node: &MockNode) -> FileEntry {
    let name = path.as_str().rsplit('/').next().unwrap_or("/").to_owned();
    match node {
        MockNode::Directory => FileEntry {
            path,
            name,
            kind: EntryKind::Directory,
            size: 0,
            version: None,
        },
        MockNode::Text { content, revision } => FileEntry {
            path,
            name,
            kind: EntryKind::File,
            size: u64::try_from(content.len()).unwrap_or(u64::MAX),
            version: Some(version(content, *revision)),
        },
    }
}

fn version(content: &str, revision: u128) -> FileVersion {
    FileVersion {
        length: u64::try_from(content.len()).unwrap_or(u64::MAX),
        modified_unix_nanos: revision,
    }
}

fn key(workspace: &WorkspaceRecord, path: &RelativePath) -> (String, String) {
    (workspace.id.0.clone(), path.as_str().to_owned())
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}
fn join(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_owned()
    } else if child.is_empty() {
        parent.to_owned()
    } else {
        format!("{parent}/{child}")
    }
}
fn reject_root(path: &RelativePath) -> WorkspaceResult<()> {
    if path.is_root() {
        Err(WorkspaceError::new(
            ErrorCode::RootOperationRejected,
            "The mock root cannot be changed.",
        ))
    } else {
        Ok(())
    }
}
fn require_size(size: usize, maximum: u64) -> WorkspaceResult<()> {
    if u64::try_from(size).unwrap_or(u64::MAX) <= maximum {
        Ok(())
    } else {
        Err(WorkspaceError::new(
            ErrorCode::TooLarge,
            "The mock file is too large.",
        ))
    }
}
fn not_found() -> WorkspaceError {
    WorkspaceError::new(ErrorCode::NotFound, "The mock path was not found.")
}

#[cfg(test)]
mod tests;
