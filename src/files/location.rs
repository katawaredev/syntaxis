use std::fmt::{self, Write as _};

use dioxus_code_editor::EditorCommandKind;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FilesQuery {
    pub(super) path: Option<String>,
    pub(super) line: Option<usize>,
    pub(super) column: Option<usize>,
    pub(super) end_line: Option<usize>,
    pub(super) end_column: Option<usize>,
}

impl FilesQuery {
    pub(super) fn path(path: String) -> Self {
        Self {
            path: Some(path),
            ..Self::default()
        }
    }

    pub(crate) fn location(
        path: String,
        line: usize,
        column: Option<usize>,
        end_line: Option<usize>,
        end_column: Option<usize>,
    ) -> Self {
        Self {
            path: Some(path),
            line: Some(line.max(1)),
            column: column.map(|value| value.max(1)),
            end_line: end_line.map(|value| value.max(1)),
            end_column: end_column.map(|value| value.max(1)),
        }
    }
}

impl From<&str> for FilesQuery {
    fn from(query: &str) -> Self {
        let mut result = Self::default();
        for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
            match key.as_ref() {
                "path" if result.path.is_none() => {
                    result.path = (!value.trim().is_empty()).then(|| value.into_owned());
                }
                "at" => {
                    if let Some(location) = parse_compact_location(&value) {
                        result.line.get_or_insert(location.line);
                        result.column = result.column.or(location.column);
                        result.end_line = result.end_line.or(location.end_line);
                        result.end_column = result.end_column.or(location.end_column);
                    }
                }
                _ => {}
            }
        }
        result
    }
}

impl fmt::Display for FilesQuery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        if let Some(path) = self.path.as_deref() {
            serializer.append_pair("path", path);
        }
        let mut query = serializer.finish();
        if let Some(location) = compact_location(self) {
            if !query.is_empty() {
                query.push('&');
            }
            query.push_str("at=");
            query.push_str(&location);
        }
        formatter.write_str(&query)
    }
}

struct ParsedLocation {
    line: usize,
    column: Option<usize>,
    end_line: Option<usize>,
    end_column: Option<usize>,
}

fn parse_compact_location(value: &str) -> Option<ParsedLocation> {
    let (start, end) = value
        .split_once('-')
        .map_or((value, None), |(start, end)| (start, Some(end)));
    let (line, column) = parse_line_column(start)?;
    let (end_line, end_column) = match end {
        Some(end) if end.contains(':') => {
            let (line, column) = parse_line_column(end)?;
            (Some(line), column)
        }
        Some(end) => (Some(line), Some(positive_usize(end)?)),
        None => (None, None),
    };
    Some(ParsedLocation {
        line,
        column,
        end_line,
        end_column,
    })
}

fn parse_line_column(value: &str) -> Option<(usize, Option<usize>)> {
    let (line, column) = value
        .split_once(':')
        .map_or((value, None), |(line, column)| (line, Some(column)));
    let column = match column {
        Some(column) => Some(positive_usize(column)?),
        None => None,
    };
    Some((positive_usize(line)?, column))
}

fn positive_usize(value: &str) -> Option<usize> {
    value.parse::<usize>().ok().filter(|value| *value > 0)
}

fn compact_location(query: &FilesQuery) -> Option<String> {
    let line = query.line?;
    let Some(column) = query.column else {
        return Some(line.to_string());
    };
    let mut location = format!("{line}:{column}");
    if let Some(end_column) = query.end_column {
        if query.end_line.is_none() || query.end_line == Some(line) {
            let _ = write!(location, "-{end_column}");
        } else if let Some(end_line) = query.end_line {
            let _ = write!(location, "-{end_line}:{end_column}");
        }
    }
    Some(location)
}

pub(super) fn location_command(source: &str, location: &FilesQuery) -> EditorCommandKind {
    let Some(line) = location.line else {
        return EditorCommandKind::Focus;
    };
    let Some(column) = location.column else {
        return EditorCommandKind::GoToLine { line };
    };
    let start = line_column_offset(source, line, column);
    let end = location
        .end_line
        .or(location.end_column)
        .map_or(start, |_| {
            line_column_offset(
                source,
                location.end_line.unwrap_or(line),
                location.end_column.unwrap_or(column),
            )
        });
    EditorCommandKind::Select {
        start: start.min(end),
        end: start.max(end),
    }
}

fn line_column_offset(source: &str, line: usize, column: usize) -> usize {
    let start = if line.max(1) == 1 {
        0
    } else {
        source
            .match_indices('\n')
            .nth(line.max(1) - 2)
            .map_or(source.len(), |(offset, _)| offset + 1)
    };
    let end = source[start..]
        .find('\n')
        .map_or(source.len(), |offset| start + offset);
    source[start..end]
        .char_indices()
        .nth(column.max(1) - 1)
        .map_or(end, |(offset, _)| start + offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_location_links_round_trip_through_the_router() {
        let route = crate::app::Route::Files {
            slug: "syntaxis-demo".into(),
            query: FilesQuery::location(
                "src/folder with spaces/main.rs".into(),
                42,
                Some(17),
                Some(42),
                Some(25),
            ),
        };
        let link = route.to_string();
        assert_eq!(
            link,
            "/workspaces/syntaxis-demo/files?path=src%2Ffolder+with+spaces%2Fmain.rs&at=42:17-25"
        );
        assert_eq!(link.parse::<crate::app::Route>().unwrap(), route);
    }

    #[test]
    fn file_location_commands_use_one_based_unicode_columns_and_clamp() {
        let source = "first\nαβγ\nlast";
        let location = FilesQuery::location("src/main.rs".into(), 2, Some(2), Some(2), Some(4));
        assert_eq!(
            location_command(source, &location),
            EditorCommandKind::Select { start: 8, end: 12 }
        );
        assert_eq!(line_column_offset(source, 99, 99), source.len());
    }

    #[test]
    fn compact_file_locations_support_lines_columns_and_ranges() {
        let line = FilesQuery::from("path=src%2Fmain.rs&at=42");
        assert_eq!((line.line, line.column), (Some(42), None));

        let same_line = FilesQuery::from("path=src%2Fmain.rs&at=42%3A17-25");
        assert_eq!(same_line.line, Some(42));
        assert_eq!(same_line.column, Some(17));
        assert_eq!(same_line.end_line, Some(42));
        assert_eq!(same_line.end_column, Some(25));

        let multiline = FilesQuery::from("path=src%2Fmain.rs&at=42%3A17-44%3A3");
        assert_eq!(multiline.end_line, Some(44));
        assert_eq!(multiline.end_column, Some(3));
        assert_eq!(compact_location(&multiline).as_deref(), Some("42:17-44:3"));
    }
}
