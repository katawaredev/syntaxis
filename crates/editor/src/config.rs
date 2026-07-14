#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndentStyle {
    Spaces,
    Tabs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEnding {
    Lf,
    Crlf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorConfig {
    pub indent_style: IndentStyle,
    pub indent_size: usize,
    pub tab_width: usize,
    pub line_ending: Option<LineEnding>,
    pub insert_final_newline: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            indent_style: IndentStyle::Spaces,
            indent_size: 4,
            tab_width: 4,
            line_ending: None,
            insert_final_newline: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorConfigSource {
    /// Workspace-relative directory containing this `.editorconfig`.
    pub directory: String,
    pub contents: String,
}

#[derive(Clone, Debug, Default)]
struct PartialConfig {
    indent_style: Option<IndentStyle>,
    indent_size: Option<usize>,
    indent_size_uses_tab_width: bool,
    tab_width: Option<usize>,
    line_ending: Option<LineEnding>,
    insert_final_newline: Option<bool>,
}

pub fn resolve_editor_config(sources: &[EditorConfigSource], path: &str) -> EditorConfig {
    let mut result = EditorConfig::default();
    for source in sources {
        let relative = relative_to_directory(path, &source.directory);
        let Some(relative) = relative else {
            continue;
        };
        for (pattern, settings) in parse_sections(&source.contents) {
            let candidate = if pattern.contains('/') {
                relative
            } else {
                relative.rsplit('/').next().unwrap_or(relative)
            };
            if pattern_matches(&pattern, candidate) {
                apply_partial(&mut result, &settings);
            }
        }
    }
    result
}

pub fn apply_editor_config(contents: &str, config: &EditorConfig) -> String {
    let normalized = match config.line_ending {
        Some(LineEnding::Lf) => contents.replace("\r\n", "\n").replace('\r', "\n"),
        Some(LineEnding::Crlf) => contents
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\n', "\r\n"),
        None => contents.to_owned(),
    };
    if config.insert_final_newline && !normalized.ends_with('\n') && !normalized.ends_with('\r') {
        let newline = if config.line_ending == Some(LineEnding::Crlf) {
            "\r\n"
        } else {
            "\n"
        };
        format!("{normalized}{newline}")
    } else {
        normalized
    }
}

fn relative_to_directory<'a>(path: &'a str, directory: &str) -> Option<&'a str> {
    let directory = directory.trim_matches('/');
    if directory.is_empty() {
        Some(path.trim_start_matches('/'))
    } else {
        path.strip_prefix(directory)?.strip_prefix('/')
    }
}

fn parse_sections(source: &str) -> Vec<(String, PartialConfig)> {
    let mut sections = Vec::<(String, PartialConfig)>::new();
    for raw_line in source.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(pattern) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            sections.push((pattern.trim().to_owned(), PartialConfig::default()));
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let Some((_, settings)) = sections.last_mut() else {
            continue;
        };
        apply_property(settings, key.trim(), value.trim());
    }
    sections
}

fn apply_property(settings: &mut PartialConfig, key: &str, value: &str) {
    let key = key.to_ascii_lowercase();
    let value = value.to_ascii_lowercase();
    match key.as_str() {
        "indent_style" => {
            settings.indent_style = match value.as_str() {
                "space" => Some(IndentStyle::Spaces),
                "tab" => Some(IndentStyle::Tabs),
                _ => None,
            };
        }
        "indent_size" if value == "tab" => settings.indent_size_uses_tab_width = true,
        "indent_size" => settings.indent_size = positive_usize(&value),
        "tab_width" => settings.tab_width = positive_usize(&value),
        "end_of_line" => {
            settings.line_ending = match value.as_str() {
                "lf" => Some(LineEnding::Lf),
                "crlf" => Some(LineEnding::Crlf),
                _ => None,
            };
        }
        "insert_final_newline" => {
            settings.insert_final_newline = match value.as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            };
        }
        _ => {}
    }
}

fn positive_usize(value: &str) -> Option<usize> {
    value.parse::<usize>().ok().filter(|value| *value > 0)
}

fn apply_partial(result: &mut EditorConfig, settings: &PartialConfig) {
    if let Some(style) = settings.indent_style {
        result.indent_style = style;
    }
    if let Some(width) = settings.tab_width {
        result.tab_width = width;
    }
    if settings.indent_size_uses_tab_width {
        result.indent_size = result.tab_width;
    } else if let Some(size) = settings.indent_size {
        result.indent_size = size;
    }
    if let Some(line_ending) = settings.line_ending {
        result.line_ending = Some(line_ending);
    }
    if let Some(insert) = settings.insert_final_newline {
        result.insert_final_newline = insert;
    }
}

fn pattern_matches(pattern: &str, value: &str) -> bool {
    expand_braces(pattern)
        .iter()
        .any(|pattern| wildcard_matches(pattern.as_bytes(), value.as_bytes()))
}

fn expand_braces(pattern: &str) -> Vec<String> {
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_owned()];
    };
    let Some(relative_close) = pattern[open + 1..].find('}') else {
        return vec![pattern.to_owned()];
    };
    let close = open + relative_close + 1;
    pattern[open + 1..close]
        .split(',')
        .map(|choice| format!("{}{}{}", &pattern[..open], choice, &pattern[close + 1..]))
        .collect()
}

fn wildcard_matches(pattern: &[u8], value: &[u8]) -> bool {
    let mut table = vec![vec![false; value.len() + 1]; pattern.len() + 1];
    table[0][0] = true;
    for i in 1..=pattern.len() {
        if pattern[i - 1] == b'*' {
            table[i][0] = table[i - 1][0];
        }
        for j in 1..=value.len() {
            table[i][j] = match pattern[i - 1] {
                b'*' => table[i - 1][j] || table[i][j - 1],
                b'?' => table[i - 1][j - 1],
                byte => byte == value[j - 1] && table[i - 1][j - 1],
            };
        }
    }
    table[pattern.len()][value.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_sources_override_root_settings() {
        let sources = vec![
            EditorConfigSource {
                directory: String::new(),
                contents: "[*]\nindent_style = space\nindent_size = 2\n[*.rs]\nindent_size = 4\n"
                    .into(),
            },
            EditorConfigSource {
                directory: "generated".into(),
                contents: "[*.rs]\nindent_style = tab\ntab_width = 8\nindent_size = tab\n".into(),
            },
        ];
        let config = resolve_editor_config(&sources, "generated/parser.rs");
        assert_eq!(config.indent_style, IndentStyle::Tabs);
        assert_eq!(config.indent_size, 8);
        assert_eq!(config.tab_width, 8);
    }

    #[test]
    fn brace_patterns_and_line_endings_are_supported() {
        let sources = vec![EditorConfigSource {
            directory: String::new(),
            contents: "[*.{js,ts}]\nend_of_line = crlf\ninsert_final_newline = true\n".into(),
        }];
        let config = resolve_editor_config(&sources, "src/app.ts");
        assert_eq!(apply_editor_config("one\ntwo", &config), "one\r\ntwo\r\n");
    }
}
