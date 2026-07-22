use std::collections::{BTreeMap, BTreeSet};

use syntaxis_workspace::{EntryKind, FileEntry};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExplorerNode {
    pub entry: FileEntry,
    pub depth: usize,
    pub expanded: bool,
    pub ignored: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExplorerTree {
    directories: BTreeMap<String, Vec<FileEntry>>,
    expanded: BTreeSet<String>,
}

struct FlattenOptions<'a> {
    search: &'a str,
    git_paths: Option<&'a BTreeSet<String>>,
    ignored_paths: &'a BTreeSet<String>,
    show_ignored: bool,
    expand_all: bool,
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
        ignored_paths: &BTreeSet<String>,
        show_ignored: bool,
    ) -> Vec<ExplorerNode> {
        self.flattened_with_expansion(search, git_paths, ignored_paths, show_ignored, false)
    }

    pub fn flattened_with_expansion(
        &self,
        search: &str,
        git_paths: Option<&BTreeSet<String>>,
        ignored_paths: &BTreeSet<String>,
        show_ignored: bool,
        expand_all: bool,
    ) -> Vec<ExplorerNode> {
        let search = search.trim().to_ascii_lowercase();
        let mut result = Vec::new();
        let options = FlattenOptions {
            search: &search,
            git_paths,
            ignored_paths,
            show_ignored,
            expand_all,
        };
        self.push_directory("", 0, &options, &mut result);
        result
    }

    fn push_directory(
        &self,
        directory: &str,
        depth: usize,
        options: &FlattenOptions<'_>,
        result: &mut Vec<ExplorerNode>,
    ) {
        let Some(entries) = self.directories.get(directory) else {
            return;
        };
        for entry in entries {
            let path = entry.path.as_str();
            let is_directory = entry.kind == EntryKind::Directory;
            let ignored = is_ignored(path, options.ignored_paths);
            if path == ".git" || path.starts_with(".git/") || (ignored && !options.show_ignored) {
                continue;
            }
            let path_matches =
                options.search.is_empty() || path.to_ascii_lowercase().contains(options.search);
            let git_matches = options.git_paths.is_none_or(|paths| {
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
                    expanded: is_directory && (options.expand_all || self.expanded.contains(path)),
                    ignored,
                });
            }
            if is_directory
                && (options.expand_all
                    || self.expanded.contains(path)
                    || !options.search.is_empty())
            {
                self.push_directory(path, depth + 1, options, result);
            }
        }
    }
}

fn is_ignored(path: &str, ignored_paths: &BTreeSet<String>) -> bool {
    ignored_paths.iter().any(|ignored| {
        path == ignored
            || path
                .strip_prefix(ignored)
                .is_some_and(|rest| rest.starts_with('/'))
    })
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
        assert_eq!(tree.flattened("", None, &BTreeSet::new(), false).len(), 1);
        tree.expand("src");
        assert_eq!(tree.flattened("", None, &BTreeSet::new(), false).len(), 2);
    }

    #[test]
    fn view_only_expansion_does_not_change_tree_state() {
        let mut tree = ExplorerTree::default();
        tree.replace_directory("", vec![entry("src", EntryKind::Directory)]);
        tree.replace_directory("src", vec![entry("src/main.rs", EntryKind::File)]);

        assert_eq!(
            tree.flattened_with_expansion("", None, &BTreeSet::new(), false, true)
                .len(),
            2
        );
        assert_eq!(tree.flattened("", None, &BTreeSet::new(), false).len(), 1);
    }

    #[test]
    fn ignored_directories_are_hidden_or_marked_with_their_children() {
        let mut tree = ExplorerTree::default();
        tree.replace_directory("", vec![entry("target", EntryKind::Directory)]);
        tree.replace_directory("target", vec![entry("target/app", EntryKind::File)]);
        tree.expand("target");
        let ignored = BTreeSet::from(["target".to_owned()]);

        assert!(tree.flattened("", None, &ignored, false).is_empty());
        assert!(tree
            .flattened("", None, &ignored, true)
            .iter()
            .all(|node| node.ignored));
    }
}
