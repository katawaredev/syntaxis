use std::collections::BTreeSet;
#[cfg(any(feature = "desktop", feature = "server"))]
use std::sync::Arc;

use dioxus::prelude::*;
use dioxus_code_editor::EditorRange;
#[cfg(any(feature = "desktop", feature = "server"))]
use futures_util::{StreamExt, stream};
use serde::{Deserialize, Serialize};

use crate::client_error::server_error_message;

#[cfg(any(feature = "desktop", feature = "server"))]
use super::{EntryKind, MAX_TEXT_BYTES, RelativePath, workspace_client};
use super::{FileEntry, WorkspaceRecord};

#[cfg(any(feature = "desktop", feature = "server"))]
const MAX_RESULT_FILES: usize = 500;
#[cfg(any(test, feature = "desktop", feature = "server"))]
const MAX_OCCURRENCES_PER_FILE: usize = 5;
#[cfg(any(test, feature = "desktop", feature = "server"))]
const MAX_HIGHLIGHT_RANGES_PER_FILE: usize = 100;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum SearchScope {
    FileNames,
    Contents,
    #[default]
    FileNamesAndContents,
}

impl SearchScope {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::FileNames => "File names",
            Self::Contents => "File contents",
            Self::FileNamesAndContents => "Names and contents",
        }
    }

    #[cfg(any(feature = "desktop", feature = "server"))]
    fn searches_names(self) -> bool {
        self != Self::Contents
    }

    #[cfg(any(feature = "desktop", feature = "server"))]
    fn searches_contents(self) -> bool {
        self != Self::FileNames
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct WorkspaceSearchOptions {
    pub(crate) fuzzy: bool,
    pub(crate) case_sensitive: bool,
    pub(crate) scope: SearchScope,
}

impl Default for WorkspaceSearchOptions {
    fn default() -> Self {
        Self {
            fuzzy: true,
            case_sensitive: false,
            scope: SearchScope::FileNamesAndContents,
        }
    }
}

#[cfg(any(test, feature = "desktop", feature = "server"))]
enum PreparedQuery {
    Fuzzy {
        characters: Vec<char>,
        case_sensitive: bool,
    },
    Literal(regex::Regex),
}

#[cfg(any(test, feature = "desktop", feature = "server"))]
impl PreparedQuery {
    fn new(query: &str, options: WorkspaceSearchOptions) -> Option<Self> {
        if query.is_empty() {
            return None;
        }
        if options.fuzzy {
            Some(Self::Fuzzy {
                characters: query.chars().collect(),
                case_sensitive: options.case_sensitive,
            })
        } else {
            regex::RegexBuilder::new(&regex::escape(query))
                .case_insensitive(!options.case_sensitive)
                .build()
                .ok()
                .map(Self::Literal)
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct WorkspaceSearchResult {
    pub(crate) entry: FileEntry,
    pub(super) matches: Vec<EditorRange>,
    pub(super) target: Option<EditorRange>,
    pub(super) occurrences: Vec<SearchOccurrence>,
    pub(super) match_count: usize,
    score: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(super) struct SearchOccurrence {
    pub(super) line: usize,
    pub(super) preview: String,
    pub(super) target: EditorRange,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub(crate) struct WorkspaceSearchResults {
    pub(crate) items: Vec<WorkspaceSearchResult>,
    pub(super) truncated: bool,
}

pub(crate) async fn search_workspace_files(
    workspace: WorkspaceRecord,
    query: String,
    options: WorkspaceSearchOptions,
    ignored_paths: BTreeSet<String>,
    show_ignored: bool,
) -> Result<WorkspaceSearchResults, String> {
    #[cfg(not(feature = "desktop"))]
    {
        search_workspace_remote(workspace.id.0, query, options, ignored_paths, show_ignored)
            .await
            .map_err(server_error_message)
    }
    #[cfg(feature = "desktop")]
    search_workspace_files_local(workspace, query, options, ignored_paths, show_ignored).await
}

#[post("/api/workspace-files/search")]
async fn search_workspace_remote(
    workspace_id: String,
    query: String,
    options: WorkspaceSearchOptions,
    ignored_paths: BTreeSet<String>,
    show_ignored: bool,
) -> Result<WorkspaceSearchResults, ServerFnError> {
    let workspace = crate::workspace::api::get_workspace(workspace_id).await?;
    search_workspace_files_local(workspace, query, options, ignored_paths, show_ignored)
        .await
        .map_err(|message| ServerFnError::ServerError {
            message,
            code: 500,
            details: None,
        })
}

#[cfg(any(feature = "desktop", feature = "server"))]
async fn search_workspace_files_local(
    workspace: WorkspaceRecord,
    query: String,
    options: WorkspaceSearchOptions,
    ignored_paths: BTreeSet<String>,
    show_ignored: bool,
) -> Result<WorkspaceSearchResults, String> {
    let mut directories = vec![RelativePath::root()];
    let mut files = Vec::new();
    while let Some(directory) = directories.pop() {
        for entry in workspace_client::list_files(workspace.clone(), directory).await? {
            let path = entry.path.as_str();
            let ignored = is_ignored(path, &ignored_paths);
            if path == ".git" || path.starts_with(".git/") || (ignored && !show_ignored) {
                continue;
            }
            match entry.kind {
                EntryKind::Directory => directories.push(entry.path.clone()),
                EntryKind::File => files.push(entry),
                EntryKind::Symlink => {}
            }
        }
    }

    let Some(query) = PreparedQuery::new(&query, options).map(Arc::new) else {
        return Ok(WorkspaceSearchResults::default());
    };
    let mut results = stream::iter(files)
        .map(|entry| {
            let workspace = workspace.clone();
            let query = Arc::clone(&query);
            async move {
                let name_score = options
                    .scope
                    .searches_names()
                    .then(|| match_score(entry.path.as_str(), &query))
                    .flatten();
                let mut content_result = ContentSearchResult::default();
                if options.scope.searches_contents()
                    && entry.size <= MAX_TEXT_BYTES
                    && let Ok(file) =
                        workspace_client::read_text(workspace, entry.path.clone(), MAX_TEXT_BYTES)
                            .await
                {
                    content_result = content_matches_async(file.content, query).await;
                }
                if name_score.is_none() && content_result.ranges.is_empty() {
                    return None;
                }
                let target = content_result
                    .occurrences
                    .first()
                    .map(|occurrence| occurrence.target);
                Some(WorkspaceSearchResult {
                    entry,
                    score: name_score.unwrap_or_else(|| {
                        10_000 + content_result.ranges.first().map_or(0, |range| range.start)
                    }),
                    match_count: content_result.match_count,
                    matches: content_result.ranges,
                    target,
                    occurrences: content_result.occurrences,
                })
            }
        })
        .buffer_unordered(8)
        .filter_map(|result| async move { result })
        .collect::<Vec<_>>()
        .await;
    results.sort_by(|left, right| {
        left.score
            .cmp(&right.score)
            .then_with(|| left.entry.path.as_str().cmp(right.entry.path.as_str()))
    });
    let truncated = results.len() > MAX_RESULT_FILES;
    results.truncate(MAX_RESULT_FILES);
    Ok(WorkspaceSearchResults {
        items: results,
        truncated,
    })
}

#[cfg(any(feature = "desktop", feature = "server"))]
async fn content_matches_async(source: String, query: Arc<PreparedQuery>) -> ContentSearchResult {
    #[cfg(any(feature = "desktop", feature = "server"))]
    {
        tokio::task::spawn_blocking(move || content_matches_prepared(&source, &query))
            .await
            .unwrap_or_default()
    }
    #[cfg(not(any(feature = "desktop", feature = "server")))]
    {
        content_matches_prepared(&source, &query)
    }
}

#[cfg(any(test, feature = "desktop", feature = "server"))]
fn is_ignored(path: &str, ignored_paths: &BTreeSet<String>) -> bool {
    let mut candidate = path;
    loop {
        if ignored_paths.contains(candidate) {
            return true;
        }
        let Some((parent, _)) = candidate.rsplit_once('/') else {
            return false;
        };
        candidate = parent;
    }
}

#[cfg(any(feature = "desktop", feature = "server"))]
fn match_score(candidate: &str, query: &PreparedQuery) -> Option<usize> {
    match query {
        PreparedQuery::Fuzzy {
            characters,
            case_sensitive,
        } => {
            let ranges = fuzzy_ranges_prepared(candidate, characters, *case_sensitive)?;
            Some(ranges.last()?.end.saturating_sub(ranges.first()?.start) - ranges.len())
        }
        PreparedQuery::Literal(expression) => {
            expression.find(candidate).map(|matched| matched.start())
        }
    }
}

#[cfg(any(test, feature = "desktop", feature = "server"))]
#[derive(Default)]
struct ContentSearchResult {
    ranges: Vec<EditorRange>,
    occurrences: Vec<SearchOccurrence>,
    match_count: usize,
}

#[cfg(test)]
fn content_matches(
    source: &str,
    query: &str,
    options: WorkspaceSearchOptions,
) -> ContentSearchResult {
    PreparedQuery::new(query, options).map_or_else(ContentSearchResult::default, |query| {
        content_matches_prepared(source, &query)
    })
}

#[cfg(any(test, feature = "desktop", feature = "server"))]
fn content_matches_prepared(source: &str, query: &PreparedQuery) -> ContentSearchResult {
    match query {
        PreparedQuery::Fuzzy {
            characters,
            case_sensitive,
        } => fuzzy_content_matches(source, characters, *case_sensitive),
        PreparedQuery::Literal(expression) => literal_content_matches(source, expression),
    }
}

#[cfg(any(test, feature = "desktop", feature = "server"))]
fn literal_content_matches(source: &str, expression: &regex::Regex) -> ContentSearchResult {
    let mut result = ContentSearchResult::default();
    let mut offset = 0;
    for (line_index, raw_line) in source.split_inclusive('\n').enumerate() {
        let line = raw_line.trim_end_matches(['\r', '\n']);
        for matched in expression.find_iter(line) {
            result.match_count += 1;
            let target = EditorRange {
                start: offset + matched.start(),
                end: offset + matched.end(),
            };
            if result.ranges.len() < MAX_HIGHLIGHT_RANGES_PER_FILE {
                result.ranges.push(target);
            }
            if result.occurrences.len() < MAX_OCCURRENCES_PER_FILE {
                result.occurrences.push(SearchOccurrence {
                    line: line_index + 1,
                    preview: line.trim().to_owned(),
                    target,
                });
            }
        }
        offset += raw_line.len();
    }
    result
}

#[cfg(any(test, feature = "desktop", feature = "server"))]
fn fuzzy_content_matches(
    source: &str,
    query: &[char],
    case_sensitive: bool,
) -> ContentSearchResult {
    let mut result = ContentSearchResult::default();
    let mut offset = 0;
    for (line_index, raw_line) in source.split_inclusive('\n').enumerate() {
        let line = raw_line.trim_end_matches(['\r', '\n']);
        if let Some(mut line_ranges) = fuzzy_ranges_prepared(line, query, case_sensitive)
            .filter(|ranges| is_compact_content_match(line, ranges))
        {
            result.match_count += 1;
            for range in &mut line_ranges {
                range.start += offset;
                range.end += offset;
            }
            if result.occurrences.len() < MAX_OCCURRENCES_PER_FILE
                && let Some((first, last)) = line_ranges.first().zip(line_ranges.last())
            {
                result.occurrences.push(SearchOccurrence {
                    line: line_index + 1,
                    preview: line.trim().to_owned(),
                    target: EditorRange {
                        start: first.start,
                        end: last.end,
                    },
                });
            }
            let remaining = MAX_HIGHLIGHT_RANGES_PER_FILE.saturating_sub(result.ranges.len());
            result
                .ranges
                .extend(line_ranges.into_iter().take(remaining));
        }
        offset += raw_line.len();
    }
    result
}

#[cfg(test)]
fn literal_ranges(source: &str, query: &str, case_sensitive: bool) -> Vec<EditorRange> {
    let options = WorkspaceSearchOptions {
        fuzzy: false,
        case_sensitive,
        ..WorkspaceSearchOptions::default()
    };
    let Some(PreparedQuery::Literal(expression)) = PreparedQuery::new(query, options) else {
        return Vec::new();
    };
    expression
        .find_iter(source)
        .map(|matched| EditorRange {
            start: matched.start(),
            end: matched.end(),
        })
        .collect()
}

#[cfg(test)]
fn fuzzy_ranges(source: &str, query: &str, case_sensitive: bool) -> Option<Vec<EditorRange>> {
    let characters = query.chars().collect::<Vec<_>>();
    fuzzy_ranges_prepared(source, &characters, case_sensitive)
}

#[cfg(any(test, feature = "desktop", feature = "server"))]
fn fuzzy_ranges_prepared(
    source: &str,
    query: &[char],
    case_sensitive: bool,
) -> Option<Vec<EditorRange>> {
    let last_query_index = query.len().checked_sub(1)?;
    let mut prefix_starts = vec![None; query.len()];
    let mut best = None::<EditorRange>;

    for (offset, candidate) in source.char_indices() {
        for query_index in (0..query.len()).rev() {
            if !chars_match(candidate, query[query_index], case_sensitive) {
                continue;
            }
            if query_index == 0 {
                prefix_starts[0] = Some(offset);
            } else if let Some(start) = prefix_starts[query_index - 1] {
                prefix_starts[query_index] = Some(start);
            }
            if query_index == last_query_index {
                let Some(start) = prefix_starts[query_index] else {
                    continue;
                };
                let candidate_range = EditorRange {
                    start,
                    end: offset + candidate.len_utf8(),
                };
                if best.as_ref().is_none_or(|current| {
                    candidate_range.end - candidate_range.start < current.end - current.start
                }) {
                    best = Some(candidate_range);
                }
            }
        }
    }

    let best = best?;
    let mut wanted = query.iter();
    let mut current = wanted.next()?;
    let mut ranges = Vec::with_capacity(query.len());
    for (relative_offset, candidate) in source[best.start..best.end].char_indices() {
        if !chars_match(candidate, *current, case_sensitive) {
            continue;
        }
        let start = best.start + relative_offset;
        ranges.push(EditorRange {
            start,
            end: start + candidate.len_utf8(),
        });
        let Some(next) = wanted.next() else {
            break;
        };
        current = next;
    }
    (ranges.len() == query.len()).then_some(ranges)
}

#[cfg(any(test, feature = "desktop", feature = "server"))]
fn chars_match(candidate: char, wanted: char, case_sensitive: bool) -> bool {
    if case_sensitive {
        candidate == wanted
    } else {
        candidate.eq_ignore_ascii_case(&wanted)
    }
}

#[cfg(any(test, feature = "desktop", feature = "server"))]
fn is_compact_content_match(source: &str, ranges: &[EditorRange]) -> bool {
    let Some((first, last)) = ranges.first().zip(ranges.last()) else {
        return false;
    };
    let span = source
        .get(first.start..last.end)
        .unwrap_or_default()
        .chars()
        .count();
    let largest_gap = ranges
        .windows(2)
        .map(|pair| {
            source
                .get(pair[0].end..pair[1].start)
                .unwrap_or_default()
                .chars()
                .count()
        })
        .max()
        .unwrap_or(0);
    largest_gap <= 2 && span <= ranges.len() + ranges.len().div_ceil(2).max(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignored_paths_match_entries_and_descendants_only() {
        let ignored = BTreeSet::from(["target".to_owned(), "nested/cache".to_owned()]);
        assert!(is_ignored("target/debug/app", &ignored));
        assert!(is_ignored("nested/cache", &ignored));
        assert!(!is_ignored("targeted/file", &ignored));
        assert!(!is_ignored("nested/cached/file", &ignored));
    }

    #[test]
    fn fuzzy_matching_preserves_source_byte_ranges() {
        let ranges = fuzzy_ranges("src/FileSearch.rs", "fsr", false).unwrap();
        let matched = ranges
            .iter()
            .map(|range| {
                "src/FileSearch.rs"
                    .get(range.start..range.end)
                    .unwrap_or_default()
            })
            .collect::<String>();
        assert_eq!(matched.to_ascii_lowercase(), "fsr");
    }

    #[test]
    fn literal_matching_honors_case_sensitivity() {
        assert_eq!(literal_ranges("Search search", "search", false).len(), 2);
        assert_eq!(literal_ranges("Search search", "search", true).len(), 1);
    }

    #[test]
    fn fuzzy_content_searches_each_line_independently() {
        let result = content_matches(
            "alpha beta\na blue table\n",
            "abl",
            WorkspaceSearchOptions::default(),
        );
        assert_eq!(result.match_count, 1);
        assert_eq!(result.ranges.len(), 3);
        assert!(result.ranges.iter().all(|range| range.start >= 11));
    }

    #[test]
    fn fuzzy_content_rejects_letters_scattered_across_a_sentence() {
        let result = content_matches(
            "Increments when a protocol change is not backward compatible.",
            "welcome",
            WorkspaceSearchOptions::default(),
        );
        assert_eq!(result.match_count, 0);
        assert!(result.ranges.is_empty());
    }

    #[test]
    fn content_results_keep_totals_while_bounding_render_payloads() {
        let source = "match\n".repeat(1_200);
        let result = content_matches(
            &source,
            "match",
            WorkspaceSearchOptions {
                fuzzy: false,
                ..WorkspaceSearchOptions::default()
            },
        );
        assert_eq!(result.match_count, 1_200);
        assert_eq!(result.occurrences.len(), MAX_OCCURRENCES_PER_FILE);
        assert_eq!(result.ranges.len(), MAX_HIGHLIGHT_RANGES_PER_FILE);
    }

    #[test]
    fn fuzzy_matching_prefers_the_most_compact_candidate() {
        let ranges = fuzzy_ranges("w----e----l welcome", "welcome", false).unwrap();
        assert_eq!(ranges.first().unwrap().start, 12);
        assert_eq!(ranges.last().unwrap().end, 19);
    }
}
