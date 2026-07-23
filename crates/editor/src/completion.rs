use std::collections::HashSet;

const DOCUMENT_COMPLETION_RADIUS: usize = 64 * 1024;

/// Document-derived completion candidates for the word at `cursor`.
///
/// This follows the same language-agnostic model as `CodeMirror`'s
/// `completeAnyWord`: words already present in the document become suggestions.
/// Candidates retain document order, are de-duplicated, and are filtered by the
/// word prefix immediately before the cursor.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WordCompletions {
    /// Byte offset where accepting a completion should start replacing text.
    pub from: usize,
    /// Unique candidates matching the current prefix.
    pub options: Vec<String>,
}

/// Collect up to `limit` completion candidates from `source`.
pub fn complete_any_word(source: &str, cursor: usize, limit: usize) -> WordCompletions {
    complete_with_words(source, cursor, &[], limit)
}

/// Complete from a maintained dictionary and words near the cursor.
///
/// Dictionary candidates are considered first. Document scanning is bounded to
/// keep automatic completion responsive for large buffers.
pub fn complete_with_words(
    source: &str,
    cursor: usize,
    dictionary: &[String],
    limit: usize,
) -> WordCompletions {
    let cursor = char_boundary_at_or_before(source, cursor.min(source.len()));
    let from = word_start(source, cursor);
    let prefix = source.get(from..cursor).unwrap_or_default();
    let mut seen = HashSet::new();
    let mut options = Vec::new();

    if limit == 0 {
        return WordCompletions { from, options };
    }

    for word in dictionary {
        if word == prefix || !word.starts_with(prefix) || !seen.insert(word.as_str()) {
            continue;
        }
        options.push(word.clone());
        if options.len() == limit {
            return WordCompletions { from, options };
        }
    }

    let (window_start, window) = completion_window(source, cursor);
    for (start, word) in words(window) {
        let start = window_start + start;
        if start == from || word == prefix || !word.starts_with(prefix) || !seen.insert(word) {
            continue;
        }
        options.push(word.to_owned());
        if options.len() == limit {
            break;
        }
    }

    WordCompletions { from, options }
}

fn completion_window(source: &str, cursor: usize) -> (usize, &str) {
    let mut start =
        char_boundary_at_or_before(source, cursor.saturating_sub(DOCUMENT_COMPLETION_RADIUS));
    if start > 0
        && source
            .get(..start)
            .unwrap_or_default()
            .chars()
            .next_back()
            .is_some_and(is_word_character)
    {
        while start < cursor {
            let Some(character) = source.get(start..).unwrap_or_default().chars().next() else {
                break;
            };
            if !is_word_character(character) {
                break;
            }
            start += character.len_utf8();
        }
    }

    let mut end = char_boundary_at_or_before(
        source,
        cursor
            .saturating_add(DOCUMENT_COMPLETION_RADIUS)
            .min(source.len()),
    );
    if end < source.len()
        && source
            .get(..end)
            .unwrap_or_default()
            .chars()
            .next_back()
            .is_some_and(is_word_character)
    {
        while end > cursor {
            let Some(character) = source.get(..end).unwrap_or_default().chars().next_back() else {
                break;
            };
            if !is_word_character(character) {
                break;
            }
            end -= character.len_utf8();
        }
    }

    (start, source.get(start..end).unwrap_or_default())
}

fn word_start(source: &str, cursor: usize) -> usize {
    source
        .get(..cursor)
        .unwrap_or_default()
        .char_indices()
        .rev()
        .find_map(|(index, character)| {
            (!is_word_character(character)).then_some(index + character.len_utf8())
        })
        .unwrap_or(0)
}

fn words(source: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut word_start = None;
    source
        .char_indices()
        .chain(std::iter::once((source.len(), '\0')))
        .filter_map(move |(index, character)| {
            if is_word_character(character) {
                word_start.get_or_insert(index);
                None
            } else {
                word_start
                    .take()
                    .map(|start| (start, source.get(start..index).unwrap_or_default()))
            }
        })
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '$')
}

fn char_boundary_at_or_before(source: &str, mut offset: usize) -> usize {
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::{complete_any_word, complete_with_words, WordCompletions};

    #[test]
    fn completes_from_unique_document_words_in_document_order() {
        let source = "render_page renderer render_page ren";

        assert_eq!(
            complete_any_word(source, source.len(), 8),
            WordCompletions {
                from: source.len() - 3,
                options: vec!["render_page".into(), "renderer".into()],
            }
        );
    }

    #[test]
    fn supports_unicode_and_common_identifier_characters() {
        let source = "данни_ред $document дан";

        assert_eq!(
            complete_any_word(source, source.len(), 8).options,
            vec!["данни_ред"]
        );
        assert_eq!(
            complete_any_word("$document $doc", "$document $doc".len(), 8).options,
            vec!["$document"]
        );
    }

    #[test]
    fn explicit_completion_without_a_prefix_lists_document_words() {
        assert_eq!(
            complete_any_word("alpha beta ", "alpha beta ".len(), 8).options,
            vec!["alpha", "beta"]
        );
    }

    #[test]
    fn maintained_dictionary_precedes_document_words_and_deduplicates_them() {
        let dictionary = vec!["return".into(), "ref".into()];

        assert_eq!(
            complete_with_words("renderer return re", 18, &dictionary, 8).options,
            vec!["return", "ref", "renderer"]
        );
    }

    #[test]
    fn respects_the_limit_and_clamps_non_boundary_cursors() {
        assert_eq!(
            complete_any_word("alpha atom axe a", 16, 2).options.len(),
            2
        );
        assert_eq!(complete_any_word("éclair é", 1, 8).from, 0);
    }
}
