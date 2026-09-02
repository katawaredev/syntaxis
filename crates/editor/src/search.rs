use regex::RegexBuilder;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
}

/// Finds all matches of `query` in `source`.
///
/// # Errors
///
/// Returns an error when regex mode is enabled and `query` is not a valid regex.
pub fn find_matches(
    source: &str,
    query: &str,
    options: SearchOptions,
) -> Result<Vec<(usize, usize)>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let expression = build_search_regex(query, options)?;
    Ok(matching_ranges(&expression, source, options.whole_word))
}

/// Replaces one selected search match in `source`.
///
/// # Errors
///
/// Returns an error when the query is invalid or `range` is no longer a match.
pub fn replace_search_match(
    source: &str,
    query: &str,
    replacement: &str,
    options: SearchOptions,
    range: (usize, usize),
) -> Result<String, String> {
    let expression = build_search_regex(query, options)?;
    let ranges = matching_ranges(&expression, source, options.whole_word);
    if !ranges.contains(&range) {
        return Err("The selected match is no longer available.".into());
    }
    let expanded = expand_replacement(&expression, source, range, replacement, options.regex);
    let mut next = source.to_owned();
    next.replace_range(range.0..range.1, &expanded);
    Ok(next)
}

/// Replaces every search match in `source`.
///
/// # Errors
///
/// Returns an error when regex mode is enabled and `query` is not a valid regex.
pub fn replace_all_search_matches(
    source: &str,
    query: &str,
    replacement: &str,
    options: SearchOptions,
) -> Result<String, String> {
    if query.is_empty() {
        return Ok(source.to_owned());
    }
    let expression = build_search_regex(query, options)?;
    let replacements = matching_ranges(&expression, source, options.whole_word)
        .into_iter()
        .map(|range| {
            let expanded =
                expand_replacement(&expression, source, range, replacement, options.regex);
            (range, expanded)
        })
        .collect::<Vec<_>>();
    let mut next = source.to_owned();
    for ((start, end), expanded) in replacements.into_iter().rev() {
        next.replace_range(start..end, &expanded);
    }
    Ok(next)
}

fn build_search_regex(query: &str, options: SearchOptions) -> Result<regex::Regex, String> {
    let pattern = if options.regex {
        query.to_owned()
    } else {
        regex::escape(query)
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!options.case_sensitive)
        .multi_line(true)
        .build()
        .map_err(|error| error.to_string())
}

fn matching_ranges(
    expression: &regex::Regex,
    source: &str,
    whole_word: bool,
) -> Vec<(usize, usize)> {
    expression
        .find_iter(source)
        .filter(|found| !whole_word || is_whole_word(source, found.start(), found.end()))
        .map(|found| (found.start(), found.end()))
        .collect()
}

fn expand_replacement(
    expression: &regex::Regex,
    source: &str,
    (start, end): (usize, usize),
    replacement: &str,
    expand_captures: bool,
) -> String {
    if !expand_captures {
        return replacement.to_owned();
    }
    let Some(captures) = expression.captures_at(source, start) else {
        return replacement.to_owned();
    };
    if captures
        .get(0)
        .is_none_or(|found| found.start() != start || found.end() != end)
    {
        return replacement.to_owned();
    }
    let mut expanded = String::new();
    captures.expand(replacement, &mut expanded);
    expanded
}

fn is_whole_word(source: &str, start: usize, end: usize) -> bool {
    let begins_at_boundary = source[..start]
        .chars()
        .next_back()
        .is_none_or(|character| !is_word_character(character));
    let ends_at_boundary = source[end..]
        .chars()
        .next()
        .is_none_or(|character| !is_word_character(character));
    begins_at_boundary && ends_at_boundary
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_options_control_case_words_and_regex() {
        let source = "cat scatter CAT";
        assert_eq!(
            find_matches(
                source,
                "cat",
                SearchOptions {
                    whole_word: true,
                    ..SearchOptions::default()
                },
            )
            .unwrap(),
            vec![(0, 3), (12, 15)]
        );
        assert_eq!(
            find_matches(
                source,
                "C.T",
                SearchOptions {
                    case_sensitive: true,
                    regex: true,
                    ..SearchOptions::default()
                },
            )
            .unwrap(),
            vec![(12, 15)]
        );
    }

    #[test]
    fn replace_supports_literal_and_capture_expansion() {
        assert_eq!(
            replace_search_match("one two", "two", "three", SearchOptions::default(), (4, 7),)
                .unwrap(),
            "one three"
        );
        assert_eq!(
            replace_all_search_matches(
                "a1 a2",
                r"a(\d)",
                "b$1",
                SearchOptions {
                    regex: true,
                    ..SearchOptions::default()
                },
            )
            .unwrap(),
            "b1 b2"
        );
    }
}
