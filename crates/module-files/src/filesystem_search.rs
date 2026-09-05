use std::collections::VecDeque;

use async_trait::async_trait;
use syntaxis_app_contracts::AppError;
use syntaxis_workspace::{EntryKind, RelativePath, WorkspaceFiles, WorkspaceRecord};

use crate::{
    ContentMatch, SearchMatcher, SearchRequest, SearchResult, SearchResults, WorkspaceSearchPort,
    files_error,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchLimits {
    pub max_results: usize,
    pub max_file_content_bytes: u64,
    pub max_scanned_content_bytes: u64,
}

/// Normalized bounded search over any workspace filesystem implementation.
pub struct FilesystemWorkspaceSearch<F> {
    files: F,
    limits: SearchLimits,
}

impl<F> FilesystemWorkspaceSearch<F> {
    pub const fn new(files: F, limits: SearchLimits) -> Self {
        Self { files, limits }
    }
}

#[async_trait(?Send)]
impl<F> WorkspaceSearchPort for FilesystemWorkspaceSearch<F>
where
    F: WorkspaceFiles,
{
    async fn search(
        &self,
        workspace: &WorkspaceRecord,
        request: SearchRequest,
    ) -> Result<SearchResults, AppError> {
        let Some(matcher) = SearchMatcher::new(&request.query, request.options) else {
            return Ok(SearchResults::default());
        };
        let limit = request.max_results.min(self.limits.max_results);
        if limit == 0 {
            return Ok(SearchResults::default());
        }

        let mut pending = VecDeque::from([RelativePath::root()]);
        let mut results = Vec::new();
        let mut scanned_bytes = 0_u64;
        let mut truncated = false;
        'directories: while let Some(directory) = pending.pop_front() {
            for entry in self
                .files
                .list(workspace, &directory)
                .await
                .map_err(files_error)?
            {
                let path = entry.path.as_str();
                if is_reserved(path)
                    || (!request.show_ignored && is_ignored(path, &request.ignored_paths))
                {
                    continue;
                }
                match entry.kind {
                    EntryKind::Directory => pending.push_back(entry.path),
                    EntryKind::Symlink => {}
                    EntryKind::File => {
                        let path_match = matcher.path_match(path);
                        let content_match = if !matcher.searches_contents() {
                            ContentMatch::default()
                        } else if entry.size <= self.limits.max_file_content_bytes
                            && scanned_bytes.saturating_add(entry.size)
                                <= self.limits.max_scanned_content_bytes
                        {
                            scanned_bytes = scanned_bytes.saturating_add(entry.size);
                            self.files
                                .read_text(
                                    workspace,
                                    &entry.path,
                                    self.limits.max_file_content_bytes,
                                )
                                .await
                                .map_or_else(
                                    |_error| ContentMatch::default(),
                                    |file| matcher.content_match(&file.content),
                                )
                        } else {
                            truncated = true;
                            ContentMatch::default()
                        };
                        if path_match.is_none() && content_match.match_count == 0 {
                            continue;
                        }
                        let score = path_match.as_ref().map_or_else(
                            || 10_000 + content_match.ranges.first().map_or(0, |range| range.start),
                            |matched| matched.score,
                        );
                        let target = content_match
                            .occurrences
                            .first()
                            .map(|occurrence| occurrence.target);
                        results.push((
                            score,
                            SearchResult {
                                entry,
                                matches: content_match.ranges,
                                target,
                                occurrences: content_match.occurrences,
                                match_count: content_match.match_count,
                            },
                        ));
                        if results.len() > limit {
                            truncated = true;
                            break 'directories;
                        }
                    }
                }
            }
        }

        results.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.entry.path.as_str().cmp(right.1.entry.path.as_str()))
        });
        results.truncate(limit);
        Ok(SearchResults {
            items: results.into_iter().map(|(_, result)| result).collect(),
            truncated,
        })
    }
}

fn is_reserved(path: &str) -> bool {
    path == ".git" || path.starts_with(".git/")
}

fn is_ignored(path: &str, ignored_paths: &[RelativePath]) -> bool {
    ignored_paths.iter().any(|ignored| {
        path == ignored.as_str()
            || path
                .strip_prefix(ignored.as_str())
                .is_some_and(|suffix| suffix.starts_with('/'))
    })
}

#[cfg(test)]
mod tests {
    use futures_lite::future::block_on;
    use syntaxis_workspace::{
        MockWorkspaceFiles, WorkspaceAvailability, WorkspaceIcon, WorkspaceIconSymbol, WorkspaceId,
        WorkspaceProfile, WorkspaceSection,
    };

    use crate::{SearchOptions, SearchScope};

    use super::*;

    fn workspace() -> WorkspaceRecord {
        WorkspaceRecord {
            id: WorkspaceId::new("filesystem-search"),
            slug: "filesystem-search".into(),
            name: "Filesystem search".into(),
            root: "/filesystem-search".into(),
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
    fn traversal_honors_scope_ignored_paths_and_limits() {
        let workspace = workspace();
        let files = MockWorkspaceFiles::default();
        for (path, content) in [
            ("src/alpha.rs", "needle"),
            ("src/beta.rs", "needle"),
            ("target/hidden.rs", "needle"),
        ] {
            files
                .insert_text(&workspace, &RelativePath::try_from(path).unwrap(), content)
                .unwrap();
        }
        let search = FilesystemWorkspaceSearch::new(
            files,
            SearchLimits {
                max_results: 100,
                max_file_content_bytes: 1024,
                max_scanned_content_bytes: 4096,
            },
        );
        let results = block_on(search.search(
            &workspace,
            SearchRequest {
                query: "needle".into(),
                options: SearchOptions {
                    fuzzy: false,
                    case_sensitive: true,
                    scope: SearchScope::Contents,
                },
                ignored_paths: vec![RelativePath::try_from("target").unwrap()],
                show_ignored: false,
                max_results: 1,
            },
        ))
        .unwrap();
        assert_eq!(results.items.len(), 1);
        assert_eq!(results.items[0].match_count, 1);
        assert!(results.truncated);
    }
}
