use std::collections::{BTreeMap, BTreeSet};

use syntaxis_workspace::{EntryKind, FileEntry};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExplorerNode {
    pub entry: FileEntry,
    pub depth: usize,
    pub expanded: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExplorerTree {
    directories: BTreeMap<String, Vec<FileEntry>>,
    expanded: BTreeSet<String>,
}

impl ExplorerTree {
    pub fn replace_directory(&mut self, path: &str, entries: Vec<FileEntry>) {
        self.directories.insert(path.to_owned(), entries);
    }

    pub fn toggle(&mut self, path: &str) -> bool {
        if self.expanded.remove(path) {
            false
        } else {
            self.expanded.insert(path.to_owned());
            true
        }
    }

    pub fn expand(&mut self, path: &str) {
        self.expanded.insert(path.to_owned());
    }

    pub fn is_loaded(&self, path: &str) -> bool {
        self.directories.contains_key(path)
    }

    pub fn flattened(
        &self,
        search: &str,
        git_paths: Option<&BTreeSet<String>>,
    ) -> Vec<ExplorerNode> {
        let search = search.trim().to_ascii_lowercase();
        let mut result = Vec::new();
        self.push_directory("", 0, &search, git_paths, &mut result);
        result
    }

    fn push_directory(
        &self,
        directory: &str,
        depth: usize,
        search: &str,
        git_paths: Option<&BTreeSet<String>>,
        result: &mut Vec<ExplorerNode>,
    ) {
        let Some(entries) = self.directories.get(directory) else {
            return;
        };
        for entry in entries {
            let path = entry.path.as_str();
            let is_directory = entry.kind == EntryKind::Directory;
            let path_matches = search.is_empty() || path.to_ascii_lowercase().contains(search);
            let git_matches = git_paths.is_none_or(|paths| {
                paths.contains(path)
                    || (is_directory
                        && paths
                            .iter()
                            .any(|changed| changed.starts_with(&format!("{path}/"))))
            });
            if path_matches && git_matches {
                result.push(ExplorerNode {
                    entry: entry.clone(),
                    depth,
                    expanded: is_directory && self.expanded.contains(path),
                });
            }
            if is_directory && (self.expanded.contains(path) || !search.is_empty()) {
                self.push_directory(path, depth + 1, search, git_paths, result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syntaxis_workspace::RelativePath;

    fn entry(path: &str, kind: EntryKind) -> FileEntry {
        FileEntry {
            path: RelativePath::try_from(path).unwrap(),
            name: path.rsplit('/').next().unwrap().into(),
            kind,
            size: 0,
            version: None,
        }
    }

    #[test]
    fn flattening_only_descends_into_expanded_directories() {
        let mut tree = ExplorerTree::default();
        tree.replace_directory("", vec![entry("src", EntryKind::Directory)]);
        tree.replace_directory("src", vec![entry("src/main.rs", EntryKind::File)]);
        assert_eq!(tree.flattened("", None).len(), 1);
        tree.expand("src");
        assert_eq!(tree.flattened("", None).len(), 2);
    }
}
