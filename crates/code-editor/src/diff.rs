use std::collections::BTreeSet;

use dioxus::prelude::*;
use dioxus_code::advanced::{Buffer, CodeThemeStyles, TokenSpan};
use dioxus_code::{CodeTheme, Language};
use similar::{ChangeTag, TextDiff};

use super::{editor_class, shared_code_theme, CODE_EDITOR_CSS};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DiffLayout {
    #[default]
    Editor,
    Embedded,
    FullFile,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DiffLineKind {
    Equal,
    Delete,
    Insert,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiffSegment {
    text: String,
    emphasized: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiffToken {
    text: String,
    tag: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiffLine {
    kind: DiffLineKind,
    old_line: Option<usize>,
    new_line: Option<usize>,
    segments: Vec<DiffSegment>,
    tokens: Vec<DiffToken>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DiffRow {
    Line(DiffLine),
    Fold { id: usize, lines: Vec<DiffLine> },
}

/// Syntax-aware, read-only unified diff used by both the editor and Git views.
#[component]
pub fn UnifiedDiffView(
    original: String,
    current: String,
    #[props(default = Language::Rust)] language: Language,
    #[props(default = shared_code_theme(), into)] theme: CodeTheme,
    #[props(default = true)] line_numbers: bool,
    #[props(default = false)] word_wrap: bool,
    #[props(default = 4)] tab_width: usize,
    #[props(default = true)] collapse_unchanged: bool,
    #[props(default)] layout: DiffLayout,
    #[props(default)] old_line_offset: usize,
    #[props(default)] new_line_offset: usize,
    #[props(into, default)] class: String,
) -> Element {
    let mut class = editor_class(theme, line_numbers, word_wrap, &class);
    class.push_str(match layout {
        DiffLayout::Editor => " dxc-diff-layout-editor",
        DiffLayout::Embedded => " dxc-diff-layout-embedded",
        DiffLayout::FullFile => " dxc-diff-layout-full-file",
    });
    let rows = unified_diff_rows(
        &original,
        &current,
        language,
        old_line_offset,
        new_line_offset,
        collapse_unchanged,
    );
    let mut expanded = use_signal(BTreeSet::<usize>::new);
    rsx! {
        CodeThemeStyles { theme }
        document::Stylesheet { href: CODE_EDITOR_CSS }
        div {
            class: "{class} dxc-diff-editor",
            style: "--dxc-editor-tab-width: {tab_width.max(1)}",
            role: "region",
            "aria-label": "Inline file changes",
            div { class: "dxc-diff-canvas",
                if rows.is_empty() {
                    div { class: "dxc-diff-empty", "No changes" }
                }
                for row in rows {
                    match row {
                        DiffRow::Line(line) => rsx! {
                            DiffLineView { line }
                        },
                        DiffRow::Fold { id, lines } => {
                            if expanded.read().contains(&id) {
                                rsx! {
                                    for line in lines {
                                        DiffLineView { line }
                                    }
                                }
                            } else {
                                let hidden_count = lines.len();
                                rsx! {
                                    button {
                                        class: "dxc-diff-fold",
                                        "aria-label": "Expand {hidden_count} unchanged lines",
                                        onclick: move |_| {
                                            expanded.write().insert(id);
                                        },
                                        span { class: "dxc-diff-fold-meta", "⋯" }
                                        span { class: "dxc-diff-fold-label", "Expand {hidden_count} unchanged lines" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DiffLineView(line: DiffLine) -> Element {
    let (kind_class, marker) = match line.kind {
        DiffLineKind::Equal => ("dxc-diff-equal", ""),
        DiffLineKind::Delete => ("dxc-diff-delete", "−"),
        DiffLineKind::Insert => ("dxc-diff-insert", "+"),
    };
    rsx! {
        div { class: "dxc-diff-row {kind_class}",
            div { class: "dxc-diff-meta", aria_hidden: "true",
                span { class: "dxc-diff-marker", "{marker}" }
                span { class: "dxc-diff-old-line",
                    {line.old_line.map(|line| line.to_string()).unwrap_or_default()}
                }
                span { class: "dxc-diff-new-line",
                    {line.new_line.map(|line| line.to_string()).unwrap_or_default()}
                }
            }
            code { class: "dxc-diff-code",
                span { class: "dxc-diff-syntax",
                    for token in line.tokens {
                        if let Some(tag) = token.tag {
                            TokenSpan { text: token.text, tag }
                        } else {
                            span { "{token.text}" }
                        }
                    }
                }
                if line.segments.iter().any(|segment| segment.emphasized) {
                    span { class: "dxc-diff-inline-overlay", aria_hidden: "true",
                        for segment in line.segments {
                            if segment.emphasized {
                                mark { class: "dxc-diff-inline", "{segment.text}" }
                            } else {
                                span { "{segment.text}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn unified_diff_rows(
    original: &str,
    current: &str,
    language: Language,
    old_line_offset: usize,
    new_line_offset: usize,
    collapse_unchanged: bool,
) -> Vec<DiffRow> {
    if original == current {
        return Vec::new();
    }
    let diff = TextDiff::from_lines(original, current);
    let old_tokens = highlighted_diff_lines(original, language);
    let new_tokens = highlighted_diff_lines(current, language);
    let lines = diff
        .iter_all_inline_changes()
        .map(|change| {
            let kind = match change.tag() {
                ChangeTag::Equal => DiffLineKind::Equal,
                ChangeTag::Delete => DiffLineKind::Delete,
                ChangeTag::Insert => DiffLineKind::Insert,
            };
            let mut segments = change
                .iter_strings_lossy()
                .map(|(emphasized, value)| DiffSegment {
                    text: value.into_owned(),
                    emphasized,
                })
                .collect::<Vec<_>>();
            trim_diff_line_ending(&mut segments);
            DiffLine {
                kind,
                old_line: change.old_index().map(|index| index + old_line_offset + 1),
                new_line: change.new_index().map(|index| index + new_line_offset + 1),
                segments,
                tokens: match kind {
                    DiffLineKind::Delete => change
                        .old_index()
                        .and_then(|index| old_tokens.get(index))
                        .cloned()
                        .unwrap_or_default(),
                    DiffLineKind::Equal | DiffLineKind::Insert => change
                        .new_index()
                        .and_then(|index| new_tokens.get(index))
                        .cloned()
                        .unwrap_or_default(),
                },
            }
        })
        .collect::<Vec<_>>();
    if collapse_unchanged {
        collapse_unchanged_lines(&lines)
    } else {
        lines.into_iter().map(DiffRow::Line).collect()
    }
}

fn highlighted_diff_lines(source: &str, language: Language) -> Vec<Vec<DiffToken>> {
    Buffer::new(language, source.to_owned()).map_or_else(
        |_| {
            source
                .split('\n')
                .map(|line| {
                    vec![DiffToken {
                        text: line.to_owned(),
                        tag: None,
                    }]
                })
                .collect()
        },
        |buffer| {
            buffer
                .lines()
                .into_iter()
                .map(|line| {
                    line.into_iter()
                        .map(|segment| DiffToken {
                            text: segment.text().to_owned(),
                            tag: segment.tag(),
                        })
                        .collect()
                })
                .collect()
        },
    )
}

fn trim_diff_line_ending(segments: &mut Vec<DiffSegment>) {
    let Some(last) = segments.last_mut() else {
        return;
    };
    if last.text.ends_with('\n') {
        last.text.pop();
        if last.text.ends_with('\r') {
            last.text.pop();
        }
    }
    if last.text.is_empty() && segments.len() > 1 {
        segments.pop();
    }
}

fn collapse_unchanged_lines(lines: &[DiffLine]) -> Vec<DiffRow> {
    const MARGIN: usize = 4;
    const MIN_FOLD: usize = 12;
    let mut rows = Vec::new();
    let mut index = 0;
    let mut fold_id = 0;
    while index < lines.len() {
        if lines[index].kind != DiffLineKind::Equal {
            rows.push(DiffRow::Line(lines[index].clone()));
            index += 1;
            continue;
        }
        let start = index;
        while index < lines.len() && lines[index].kind == DiffLineKind::Equal {
            index += 1;
        }
        let count = index - start;
        if count < MIN_FOLD {
            rows.extend(lines[start..index].iter().cloned().map(DiffRow::Line));
            continue;
        }
        rows.extend(
            lines[start..start + MARGIN]
                .iter()
                .cloned()
                .map(DiffRow::Line),
        );
        rows.push(DiffRow::Fold {
            id: fold_id,
            lines: lines[start + MARGIN..index - MARGIN].to_vec(),
        });
        fold_id += 1;
        rows.extend(
            lines[index - MARGIN..index]
                .iter()
                .cloned()
                .map(DiffRow::Line),
        );
    }
    rows
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    #[test]
    fn unified_diff_marks_replacements_and_inline_changes() {
        let rows = unified_diff_rows(
            "same\nold value\nend\n",
            "same\nnew value\nend\n",
            Language::Rust,
            0,
            0,
            true,
        );
        let lines = rows
            .iter()
            .filter_map(|row| match row {
                DiffRow::Line(line) => Some(line),
                DiffRow::Fold { .. } => None,
            })
            .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line.kind == DiffLineKind::Delete));
        assert!(lines.iter().any(|line| line.kind == DiffLineKind::Insert));
        assert!(lines.iter().any(|line| {
            line.kind != DiffLineKind::Equal
                && line.segments.iter().any(|segment| segment.emphasized)
        }));
    }

    #[test]
    fn unified_diff_applies_fragment_line_offsets() {
        let rows = unified_diff_rows("old", "new", Language::Rust, 9, 19, false);
        let lines = rows
            .iter()
            .filter_map(|row| match row {
                DiffRow::Line(line) => Some(line),
                DiffRow::Fold { .. } => None,
            })
            .collect::<Vec<_>>();

        assert!(lines
            .iter()
            .any(|line| line.kind == DiffLineKind::Delete && line.old_line == Some(10)));
        assert!(lines
            .iter()
            .any(|line| line.kind == DiffLineKind::Insert && line.new_line == Some(20)));
    }

    #[test]
    fn unified_diff_collapses_large_unchanged_regions() {
        let mut original = String::new();
        for line in 1..=30 {
            writeln!(original, "line {line}").expect("writing to a String cannot fail");
        }
        let current = original.replace("line 15\n", "changed 15\n");
        let rows = unified_diff_rows(&original, &current, Language::Rust, 0, 0, true);

        assert!(rows.iter().any(|row| matches!(row, DiffRow::Fold { .. })));
        assert!(rows
            .iter()
            .any(|row| matches!(row, DiffRow::Line(line) if line.kind == DiffLineKind::Delete)));
        assert!(rows
            .iter()
            .any(|row| matches!(row, DiffRow::Line(line) if line.kind == DiffLineKind::Insert)));
    }
}
