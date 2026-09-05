use std::{fmt, fmt::Write as _};

/// URL query model for Files deep links and source selections.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FilesQuery {
    pub path: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub end_line: Option<usize>,
    pub end_column: Option<usize>,
}

impl FilesQuery {
    pub fn path(path: String) -> Self {
        Self {
            path: Some(path),
            ..Self::default()
        }
    }

    pub fn location(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_locations_round_trip() {
        let query = FilesQuery::location(
            "src/folder with spaces/main.rs".into(),
            12,
            Some(4),
            Some(13),
            Some(2),
        );
        let encoded = query.to_string();
        assert_eq!(FilesQuery::from(encoded.as_str()), query);
    }
}
