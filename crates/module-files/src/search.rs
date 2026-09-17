use serde::{Deserialize, Serialize};
use syntaxis_workspace::{FileEntry, RelativePath};

const MAX_OCCURRENCES_PER_FILE: usize = 5;
const MAX_HIGHLIGHT_RANGES_PER_FILE: usize = 100;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum SearchScope {
    FileNames,
    Contents,
    #[default]
    FileNamesAndContents,
}

impl SearchScope {
    pub const fn label(self) -> &'static str {
        match self {
            Self::FileNames => "File names",
            Self::Contents => "File contents",
            Self::FileNamesAndContents => "File names and contents",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SearchOptions {
    pub fuzzy: bool,
    pub case_sensitive: bool,
    pub scope: SearchScope,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            fuzzy: true,
            case_sensitive: false,
            scope: SearchScope::FileNamesAndContents,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SearchRequest {
    pub query: String,
    pub options: SearchOptions,
    pub ignored_paths: Vec<RelativePath>,
    pub show_ignored: bool,
    pub max_results: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TextRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SearchOccurrence {
    pub line: usize,
    pub preview: String,
    pub target: TextRange,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SearchResult {
    pub entry: FileEntry,
    pub matches: Vec<TextRange>,
    pub target: Option<TextRange>,
    pub occurrences: Vec<SearchOccurrence>,
    pub match_count: usize,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SearchResults {
    pub items: Vec<SearchResult>,
    pub truncated: bool,
}

/// Prepared, runtime-neutral matching semantics used by search adapters.
///
/// Runtime adapters remain responsible for traversal, I/O limits, cancellation,
/// and scheduling. Keeping matching here prevents host and browser search from
/// disagreeing about fuzzy matching, case sensitivity, and result ranges.
pub struct SearchMatcher {
    query: PreparedQuery,
    scope: SearchScope,
}

enum PreparedQuery {
    Fuzzy {
        characters: Vec<char>,
        case_sensitive: bool,
    },
    Literal(regex::Regex),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathMatch {
    pub ranges: Vec<TextRange>,
    pub score: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContentMatch {
    pub ranges: Vec<TextRange>,
    pub occurrences: Vec<SearchOccurrence>,
    pub match_count: usize,
}

impl SearchMatcher {
    pub fn new(query: &str, options: SearchOptions) -> Option<Self> {
        let query = query.trim();
        if query.is_empty() {
            return None;
        }
        let query = if options.fuzzy {
            PreparedQuery::Fuzzy {
                characters: query.chars().collect(),
                case_sensitive: options.case_sensitive,
            }
        } else {
            regex::RegexBuilder::new(&regex::escape(query))
                .case_insensitive(!options.case_sensitive)
                .build()
                .ok()
                .map(PreparedQuery::Literal)?
        };
        Some(Self {
            query,
            scope: options.scope,
        })
    }

    pub fn path_match(&self, candidate: &str) -> Option<PathMatch> {
        if self.scope == SearchScope::Contents {
            return None;
        }
        match &self.query {
            PreparedQuery::Fuzzy {
                characters,
                case_sensitive,
            } => {
                let ranges = fuzzy_ranges(candidate, characters, *case_sensitive)?;
                let score = ranges.last()?.end.saturating_sub(ranges.first()?.start) - ranges.len();
                Some(PathMatch { ranges, score })
            }
            PreparedQuery::Literal(expression) => {
                let matched = expression.find(candidate)?;
                Some(PathMatch {
                    ranges: vec![TextRange {
                        start: matched.start(),
                        end: matched.end(),
                    }],
                    score: matched.start(),
                })
            }
        }
    }

    pub const fn searches_contents(&self) -> bool {
        !matches!(self.scope, SearchScope::FileNames)
    }

    pub fn content_match(&self, source: &str) -> ContentMatch {
        if self.scope == SearchScope::FileNames {
            return ContentMatch::default();
        }
        match &self.query {
            PreparedQuery::Fuzzy {
                characters,
                case_sensitive,
            } => fuzzy_content_matches(source, characters, *case_sensitive),
            PreparedQuery::Literal(expression) => literal_content_matches(source, expression),
        }
    }
}

fn literal_content_matches(source: &str, expression: &regex::Regex) -> ContentMatch {
    let mut result = ContentMatch::default();
    let mut offset = 0;
    for (line_index, raw_line) in source.split_inclusive('\n').enumerate() {
        let line = raw_line.trim_end_matches(['\r', '\n']);
        for matched in expression.find_iter(line) {
            result.match_count += 1;
            let target = TextRange {
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

fn fuzzy_content_matches(source: &str, query: &[char], case_sensitive: bool) -> ContentMatch {
    let mut result = ContentMatch::default();
    let mut offset = 0;
    for (line_index, raw_line) in source.split_inclusive('\n').enumerate() {
        let line = raw_line.trim_end_matches(['\r', '\n']);
        if let Some(mut line_ranges) = fuzzy_ranges(line, query, case_sensitive)
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
                    target: TextRange {
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

fn fuzzy_ranges(source: &str, query: &[char], case_sensitive: bool) -> Option<Vec<TextRange>> {
    let last_query_index = query.len().checked_sub(1)?;
    let mut prefix_starts = vec![None; query.len()];
    let mut best = None::<TextRange>;
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
            if query_index == last_query_index
                && let Some(start) = prefix_starts[query_index]
            {
                let candidate_range = TextRange {
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
    for (relative_offset, candidate) in source.get(best.start..best.end)?.char_indices() {
        if !chars_match(candidate, *current, case_sensitive) {
            continue;
        }
        let start = best.start + relative_offset;
        ranges.push(TextRange {
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

fn chars_match(candidate: char, wanted: char, case_sensitive: bool) -> bool {
    if case_sensitive {
        candidate == wanted
    } else {
        candidate.eq_ignore_ascii_case(&wanted)
    }
}

fn is_compact_content_match(source: &str, ranges: &[TextRange]) -> bool {
    let Some((first, last)) = ranges.first().zip(ranges.last()) else {
        return false;
    };
    let Some(span) = source
        .get(first.start..last.end)
        .map(|matched| matched.chars().count())
    else {
        return false;
    };
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
        .unwrap_or_default();
    largest_gap <= 2 && span <= ranges.len() + ranges.len().div_ceil(2).max(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_path_matching_preserves_utf8_byte_ranges() {
        let matcher = SearchMatcher::new("fsr", SearchOptions::default()).unwrap();
        let path_match = matcher.path_match("src/FileSearch.rs").unwrap();
        let text = path_match
            .ranges
            .iter()
            .map(|range| &"src/FileSearch.rs"[range.start..range.end])
            .collect::<String>();
        assert_eq!(text.to_ascii_lowercase(), "fsr");
    }

    #[test]
    fn literal_matching_honors_case_sensitivity() {
        let insensitive = SearchMatcher::new(
            "search",
            SearchOptions {
                fuzzy: false,
                ..SearchOptions::default()
            },
        )
        .unwrap();
        let sensitive = SearchMatcher::new(
            "search",
            SearchOptions {
                fuzzy: false,
                case_sensitive: true,
                ..SearchOptions::default()
            },
        )
        .unwrap();
        assert_eq!(insensitive.content_match("Search search").match_count, 2);
        assert_eq!(sensitive.content_match("Search search").match_count, 1);
    }

    #[test]
    fn content_payloads_are_bounded_without_losing_match_totals() {
        let matcher = SearchMatcher::new(
            "match",
            SearchOptions {
                fuzzy: false,
                ..SearchOptions::default()
            },
        )
        .unwrap();
        let result = matcher.content_match(&"match\n".repeat(1_200));
        assert_eq!(result.match_count, 1_200);
        assert_eq!(result.occurrences.len(), MAX_OCCURRENCES_PER_FILE);
        assert_eq!(result.ranges.len(), MAX_HIGHLIGHT_RANGES_PER_FILE);
    }

    #[test]
    fn fuzzy_content_searches_each_line_independently() {
        let matcher = SearchMatcher::new("abl", SearchOptions::default()).unwrap();
        let result = matcher.content_match("alpha beta\na blue table\n");
        assert_eq!(result.match_count, 1);
        assert_eq!(result.ranges.len(), 3);
        assert!(result.ranges.iter().all(|range| range.start >= 11));
    }

    #[test]
    fn fuzzy_content_rejects_letters_scattered_across_a_sentence() {
        let matcher = SearchMatcher::new("welcome", SearchOptions::default()).unwrap();
        let result =
            matcher.content_match("Increments when a protocol change is not backward compatible.");
        assert_eq!(result.match_count, 0);
        assert!(result.ranges.is_empty());
    }

    #[test]
    fn fuzzy_matching_prefers_the_most_compact_candidate() {
        let matcher = SearchMatcher::new("welcome", SearchOptions::default()).unwrap();
        let path_match = matcher.path_match("w----e----l welcome").unwrap();
        assert_eq!(path_match.ranges.first().unwrap().start, 12);
        assert_eq!(path_match.ranges.last().unwrap().end, 19);
    }
}
