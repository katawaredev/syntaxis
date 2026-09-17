/// Formats a file and byte-offset selection as a one-based source reference.
///
/// Offsets inside a UTF-8 code point are clamped to the preceding character boundary. Reversed
/// selections are normalized before line and column positions are calculated.
pub fn format_file_reference(path: &str, source: &str, start: usize, end: usize) -> String {
    let start = char_boundary_at_or_before(source, start.min(source.len()));
    let end = char_boundary_at_or_before(source, end.min(source.len()));
    let (start, end) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let (start_line, start_column) = line_column_at(source, start);
    if start == end {
        return format!("{path}:{start_line}:{start_column}");
    }
    let (end_line, end_column) = line_column_at(source, end);
    if start_line == end_line {
        format!("{path}:{start_line}:{start_column}-{end_column}")
    } else {
        format!("{path}:{start_line}:{start_column}-{end_line}:{end_column}")
    }
}

fn char_boundary_at_or_before(source: &str, mut offset: usize) -> usize {
    while offset > 0 && !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn line_column_at(source: &str, offset: usize) -> (usize, usize) {
    let prefix = &source[..offset.min(source.len())];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, tail)| tail)
        .chars()
        .count()
        + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::format_file_reference;

    #[test]
    fn references_include_multiline_utf8_selections() {
        assert_eq!(
            format_file_reference("src/main.rs", "one\ntwø\nthree", 4, 9),
            "src/main.rs:2:1-3:1"
        );
    }

    #[test]
    fn references_normalize_reversed_and_split_codepoint_offsets() {
        assert_eq!(
            format_file_reference("utf8.txt", "aøz", 3, 2),
            "utf8.txt:1:2-3"
        );
    }
}
