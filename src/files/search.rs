use std::collections::BTreeSet;

use dioxus_code_editor::EditorRange;
use futures_util::{stream, StreamExt};

use super::{
    workspace_client, EntryKind, FileEntry, RelativePath, WorkspaceRecord, MAX_TEXT_BYTES,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum SearchScope {
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

    fn searches_names(self) -> bool {
        self != Self::Contents
    }

    fn searches_contents(self) -> bool {
        self != Self::FileNames
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct WorkspaceSearchOptions {
    pub(super) fuzzy: bool,
    pub(super) case_sensitive: bool,
    pub(super) scope: SearchScope,
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

#[derive(Clone, Debug, PartialEq)]
pub(super) struct WorkspaceSearchResult {
    pub(super) entry: FileEntry,
    pub(super) matches: Vec<EditorRange>,
    pub(super) target: Option<EditorRange>,
    pub(super) occurrences: Vec<SearchOccurrence>,
    pub(super) match_count: usize,
    score: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct SearchOccurrence {
    pub(super) line: usize,
    pub(super) preview: String,
    pub(super) target: EditorRange,
}

pub(super) async fn search_workspace_files(
    workspace: WorkspaceRecord,
    query: String,
    options: WorkspaceSearchOptions,
    ignored_paths: BTreeSet<String>,
    show_ignored: bool,
) -> Result<Vec<WorkspaceSearchResult>, String> {
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

    let mut results = stream::iter(files)
        .map(|entry| {
            let workspace = workspace.clone();
            let query = query.clone();
            async move {
                let name_score = options
                    .scope
                    .searches_names()
                    .then(|| match_score(entry.path.as_str(), &query, options))
                    .flatten();
                let mut content_result = ContentSearchResult::default();
                if options.scope.searches_contents() && entry.size <= MAX_TEXT_BYTES {
                    if let Ok(file) =
                        workspace_client::read_text(workspace, entry.path.clone(), MAX_TEXT_BYTES)
                            .await
                    {
                        content_result = content_matches(&file.content, &query, options);
                    }
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
    Ok(results)
}

fn is_ignored(path: &str, ignored_paths: &BTreeSet<String>) -> bool {
    ignored_paths.iter().any(|ignored| {
        path == ignored
            || path
                .strip_prefix(ignored)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

fn match_score(candidate: &str, query: &str, options: WorkspaceSearchOptions) -> Option<usize> {
    if options.fuzzy {
        let ranges = fuzzy_ranges(candidate, query, options.case_sensitive)?;
        Some(ranges.last()?.end.saturating_sub(ranges.first()?.start) - ranges.len())
    } else {
        literal_ranges(candidate, query, options.case_sensitive)
            .first()
            .map(|range| range.start)
    }
}

#[derive(Default)]
struct ContentSearchResult {
    ranges: Vec<EditorRange>,
    occurrences: Vec<SearchOccurrence>,
    match_count: usize,
}

fn content_matches(
    source: &str,
    query: &str,
    options: WorkspaceSearchOptions,
) -> ContentSearchResult {
    if options.fuzzy {
        fuzzy_content_matches(source, query, options.case_sensitive)
    } else {
        let ranges = literal_ranges(source, query, options.case_sensitive);
        let occurrences = ranges
            .iter()
            .map(|range| search_occurrence(source, *range))
            .collect::<Vec<_>>();
        ContentSearchResult {
            match_count: ranges.len(),
            ranges,
            occurrences,
        }
    }
}

fn fuzzy_content_matches(source: &str, query: &str, case_sensitive: bool) -> ContentSearchResult {
    let mut result = ContentSearchResult::default();
    let mut offset = 0;
    for (line_index, raw_line) in source.split_inclusive('\n').enumerate() {
        let line = raw_line.trim_end_matches(['\r', '\n']);
        if let Some(line_ranges) = fuzzy_ranges(line, query, case_sensitive)
            .filter(|ranges| is_compact_content_match(line, ranges))
        {
            result.match_count += 1;
            let absolute_ranges = line_ranges
                .into_iter()
                .map(|range| EditorRange {
                    start: offset + range.start,
                    end: offset + range.end,
                })
                .collect::<Vec<_>>();
            result.occurrences.push(SearchOccurrence {
                line: line_index + 1,
                preview: line.trim().to_owned(),
                target: EditorRange {
                    start: absolute_ranges.first().unwrap().start,
                    end: absolute_ranges.last().unwrap().end,
                },
            });
            result.ranges.extend(absolute_ranges);
        }
        offset += raw_line.len();
    }
    result
}

fn search_occurrence(source: &str, target: EditorRange) -> SearchOccurrence {
    let line_start = source[..target.start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let line_end = source[target.end..]
        .find('\n')
        .map_or(source.len(), |index| target.end + index);
    SearchOccurrence {
        line: source[..line_start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1,
        preview: source[line_start..line_end].trim().to_owned(),
        target,
    }
}

fn literal_ranges(source: &str, query: &str, case_sensitive: bool) -> Vec<EditorRange> {
    if query.is_empty() {
        return Vec::new();
    }
    regex::RegexBuilder::new(&regex::escape(query))
        .case_insensitive(!case_sensitive)
        .build()
        .map(|expression| {
            expression
                .find_iter(source)
                .map(|matched| EditorRange {
                    start: matched.start(),
                    end: matched.end(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn fuzzy_ranges(source: &str, query: &str, case_sensitive: bool) -> Option<Vec<EditorRange>> {
    let source_chars = source.char_indices().collect::<Vec<_>>();
    let query_chars = query.chars().collect::<Vec<_>>();
    let first = *query_chars.first()?;
    let mut best = None::<Vec<EditorRange>>;

    for (start_index, &(start, candidate)) in source_chars.iter().enumerate() {
        if !chars_match(candidate, first, case_sensitive) {
            continue;
        }
        let mut ranges = vec![EditorRange {
            start,
            end: start + candidate.len_utf8(),
        }];
        let mut source_index = start_index + 1;
        for &wanted in &query_chars[1..] {
            let Some((matched_index, &(start, candidate))) = source_chars
                .iter()
                .enumerate()
                .skip(source_index)
                .find(|(_, (_, candidate))| chars_match(*candidate, wanted, case_sensitive))
            else {
                ranges.clear();
                break;
            };
            ranges.push(EditorRange {
                start,
                end: start + candidate.len_utf8(),
            });
            source_index = matched_index + 1;
        }
        if ranges.len() == query_chars.len()
            && best.as_ref().is_none_or(|current| {
                ranges.last().unwrap().end - ranges.first().unwrap().start
                    < current.last().unwrap().end - current.first().unwrap().start
            })
        {
            best = Some(ranges);
        }
    }
    best
}

fn chars_match(candidate: char, wanted: char, case_sensitive: bool) -> bool {
    if case_sensitive {
        candidate == wanted
    } else {
        candidate.eq_ignore_ascii_case(&wanted)
    }
}

fn is_compact_content_match(source: &str, ranges: &[EditorRange]) -> bool {
    let Some((first, last)) = ranges.first().zip(ranges.last()) else {
        return false;
    };
    let span = source[first.start..last.end].chars().count();
    let largest_gap = ranges
        .windows(2)
        .map(|pair| source[pair[0].end..pair[1].start].chars().count())
        .max()
        .unwrap_or(0);
    largest_gap <= 2 && span <= ranges.len() + ranges.len().div_ceil(2).max(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matching_preserves_source_byte_ranges() {
        let ranges = fuzzy_ranges("src/FileSearch.rs", "fsr", false).unwrap();
        let matched = ranges
            .iter()
            .map(|range| &"src/FileSearch.rs"[range.start..range.end])
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
    fn fuzzy_matching_prefers_the_most_compact_candidate() {
        let ranges = fuzzy_ranges("w----e----l welcome", "welcome", false).unwrap();
        assert_eq!(ranges.first().unwrap().start, 12);
        assert_eq!(ranges.last().unwrap().end, 19);
    }
}
