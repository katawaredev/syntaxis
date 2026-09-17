//! Pure browser AI path and Markdown helpers, also tested on native targets.

use syntaxis_workspace::{RelativePath, WorkspaceError};

pub(crate) fn tool_path(value: &str) -> Result<RelativePath, WorkspaceError> {
    let relative = if value == "/workspace" {
        ""
    } else {
        value.strip_prefix("/workspace/").unwrap_or(value)
    };
    if relative.contains(['\\', '\0']) || relative.starts_with('/') {
        return Err(WorkspaceError::invalid_path(
            "Use a workspace-relative path or /workspace/<path>.",
        ));
    }
    RelativePath::try_from(relative)
}

pub(crate) fn split_markdown(source: &str) -> (String, String) {
    let normalized = source.replace("\r\n", "\n");
    if let Some(rest) = normalized.strip_prefix("---\n") {
        let mut offset = 0;
        for line in rest.split_inclusive('\n') {
            if line.trim_end_matches('\n') == "---" {
                return (
                    rest[..offset].trim_end().into(),
                    rest[offset + line.len()..].trim_start_matches('\n').into(),
                );
            }
            offset += line.len();
        }
    }
    (String::new(), normalized)
}

pub(crate) fn metadata(metadata: &str, key: &str) -> Option<String> {
    metadata.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name.trim() == key).then(|| {
            serde_json::from_str::<String>(value.trim())
                .unwrap_or_else(|_| value.trim().trim_matches(['\'', '"']).to_owned())
        })
    })
}

/// One-pass substitution: argument contents are never interpreted as placeholders.
pub(crate) fn expand_template(body: &str, arguments: &str) -> String {
    let words = arguments.split_whitespace().collect::<Vec<_>>();
    let mut output = String::new();
    let mut remaining = body;
    while let Some(index) = remaining.find('$') {
        output.push_str(&remaining[..index]);
        remaining = &remaining[index + 1..];
        if let Some(rest) = remaining
            .strip_prefix("ARGUMENTS")
            .or_else(|| remaining.strip_prefix('@'))
        {
            output.push_str(arguments);
            remaining = rest;
        } else {
            let digits = remaining.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 {
                output.push('$');
            } else {
                if let Ok(position) = remaining[..digits].parse::<usize>()
                    && let Some(value) = position.checked_sub(1).and_then(|index| words.get(index))
                {
                    output.push_str(value);
                }
                remaining = &remaining[digits..];
            }
        }
    }
    output.push_str(remaining);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_paths_share_the_terminal_root_without_allowing_escape() {
        for path in ["app.js", "./app.js", "/workspace/app.js"] {
            assert_eq!(tool_path(path).unwrap().as_str(), "app.js");
        }
        for path in ["", ".", "/workspace", "/workspace/"] {
            assert!(tool_path(path).unwrap().is_root());
        }
        for path in [
            "/etc/passwd",
            "/workspace-other/file",
            "/workspace/../secret",
            "../secret",
            "a\\b",
            "/workspace//etc",
        ] {
            assert!(tool_path(path).is_err(), "{path}");
        }
    }

    #[test]
    fn markdown_preserves_body_and_handles_windows_newlines() {
        let (front, body) = split_markdown("---\r\nname: review\r\n---\r\n\r\nReview it.\r\n");
        assert_eq!(metadata(&front, "name").as_deref(), Some("review"));
        assert_eq!(body, "Review it.\n");
        assert_eq!(split_markdown("---\nnot closed").1, "---\nnot closed");
    }

    #[test]
    fn arguments_are_not_recursively_expanded() {
        assert_eq!(
            expand_template("$1 / $2 / $ARGUMENTS / $@", "one $1"),
            "one / $1 / one $1 / one $1"
        );
        assert_eq!(
            expand_template("$10 $1 $unknown", "a b c d e f g h i j"),
            "j a $unknown"
        );
    }
}
