//! Files deep-link projection.

use dioxus_code_editor::EditorCommandKind;
pub use crate::FilesQuery;

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
        assert_eq!(multiline.to_string(), "path=src%2Fmain.rs&at=42:17-44:3");
    }
}
