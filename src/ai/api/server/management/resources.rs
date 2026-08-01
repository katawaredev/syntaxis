use super::*;

pub(super) const SKILL_PAGE_SIZE: usize = 20;
pub(super) const MAX_RESOURCE_BYTES: usize = 512 * 1024;
pub(super) const MAX_SKILL_DOWNLOAD_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn prompt_directory(root: &Path, scope: PiResourceScope) -> PathBuf {
    match scope {
        PiResourceScope::Global => agent_dir(root).join("prompts"),
        PiResourceScope::Project => root.join(".pi/prompts"),
    }
}

pub(super) fn skill_directory(root: &Path, scope: PiResourceScope) -> PathBuf {
    match scope {
        PiResourceScope::Global => agent_dir(root).join("skills"),
        PiResourceScope::Project => root.join(".pi/skills"),
    }
}

pub(super) fn validate_resource_name(name: &str) -> Result<(), ServerFnError> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if valid {
        Ok(())
    } else {
        Err(client_error(
            "Names must use 1–64 lowercase letters, numbers, or single hyphens",
        ))
    }
}

pub(super) fn validate_prompt_name(name: &str) -> Result<(), ServerFnError> {
    let valid = !name.is_empty()
        && name.len() <= 100
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte));
    if valid {
        Ok(())
    } else {
        Err(client_error(
            "Template names may use letters, numbers, dots, underscores, and hyphens",
        ))
    }
}

pub(super) fn validate_remote_segment(value: &str) -> Result<(), ServerFnError> {
    let valid = !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte));
    if valid {
        Ok(())
    } else {
        Err(client_error("Invalid skill source"))
    }
}

pub(super) fn validate_resource_text(
    value: &str,
    max_bytes: usize,
    label: &str,
) -> Result<(), ServerFnError> {
    if value.len() <= max_bytes {
        Ok(())
    } else {
        Err(client_error(format!("The {label} is too large")))
    }
}

pub(super) fn split_frontmatter(source: &str) -> (&str, &str) {
    let Some(rest) = source.strip_prefix("---\n") else {
        return ("", source);
    };
    let Some(end) = rest.find("\n---") else {
        return ("", source);
    };
    (
        &rest[..end],
        rest[end + 4..].trim_start_matches(['\r', '\n']),
    )
}

pub(super) fn metadata_value(metadata: &str, key: &str) -> Option<String> {
    metadata.lines().find_map(|line| {
        let (candidate, value) = line.split_once(':')?;
        if candidate.trim() != key {
            return None;
        }
        let value = value.trim();
        serde_json::from_str::<String>(value)
            .ok()
            .or_else(|| Some(value.trim_matches(['\'', '"']).to_owned()))
    })
}

pub(super) fn safe_relative_path(value: &str) -> Result<PathBuf, ServerFnError> {
    let path = Path::new(value);
    let safe = !value.is_empty()
        && value.len() <= 512
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)));
    if safe {
        Ok(path.to_owned())
    } else {
        Err(server_error("The skill download contains an unsafe path"))
    }
}

pub(super) fn reject_symlink(path: &Path) -> Result<(), ServerFnError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(client_error("Syntaxis will not modify a linked resource"))
        }
        Ok(_) | Err(_) => Ok(()),
    }
}

pub(super) fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), ServerFnError> {
    let Some(parent) = path.parent() else {
        return Err(server_error("Invalid resource path"));
    };
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("resource");
    let temporary = parent.join(format!(".{file_name}.syntaxis-{}", std::process::id()));
    fs::write(&temporary, contents).map_err(|error| {
        server_error(format!("Could not write {}: {error}", temporary.display()))
    })?;
    fs::rename(&temporary, path)
        .map_err(|error| server_error(format!("Could not save {}: {error}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_names_and_download_paths_are_restricted() {
        validate_resource_name("code-review").expect("kebab-case resource names must be accepted");
        validate_resource_name("../review").expect_err("path traversal must be rejected");
        validate_resource_name("CodeReview")
            .expect_err("uppercase resource names must be rejected");
        validate_prompt_name("review_PR.v2").expect("valid prompt names must be accepted");
        validate_prompt_name("../review").expect_err("prompt path traversal must be rejected");
        safe_relative_path("references/guide.md").expect("safe relative paths must be accepted");
        safe_relative_path("../SKILL.md").expect_err("parent traversal must be rejected");
        safe_relative_path("/tmp/SKILL.md").expect_err("absolute paths must be rejected");
    }

    #[test]
    fn frontmatter_fields_are_read_without_losing_the_body() {
        let source =
            "---\nname: \"review\"\ndescription: Review changes\nargument-hint: \"<path>\"\n---\n\nDo it.";
        let (metadata, body) = split_frontmatter(source);
        assert_eq!(metadata_value(metadata, "name").as_deref(), Some("review"));
        assert_eq!(
            metadata_value(metadata, "argument-hint").as_deref(),
            Some("<path>")
        );
        assert_eq!(body, "Do it.");
    }
}
