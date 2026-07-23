use super::*;

pub(crate) async fn prompt_templates(
    workspace_id: WorkspaceId,
) -> Result<Vec<PromptTemplate>, ServerFnError> {
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let root = Path::new(&workspace.root);
    let mut templates = Vec::new();
    for scope in [PiResourceScope::Global, PiResourceScope::Project] {
        let directory = prompt_directory(root, scope);
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("md") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let Ok(source) = fs::read_to_string(&path) else {
                continue;
            };
            let (metadata, content) = split_frontmatter(&source);
            let description = metadata_value(metadata, "description").unwrap_or_else(|| {
                content
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or_default()
                    .trim()
                    .to_owned()
            });
            templates.push(PromptTemplate {
                name: name.to_owned(),
                description,
                argument_hint: metadata_value(metadata, "argument-hint").unwrap_or_default(),
                content: content.trim().to_owned(),
                scope,
            });
        }
    }
    templates.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.scope.cmp(&right.scope))
    });
    Ok(templates)
}

pub(crate) async fn save_prompt_template(
    workspace_id: WorkspaceId,
    original_name: Option<String>,
    template: PromptTemplate,
) -> Result<(), ServerFnError> {
    validate_prompt_name(&template.name)?;
    validate_resource_text(&template.description, 1024, "description")?;
    validate_resource_text(&template.argument_hint, 256, "argument hint")?;
    validate_resource_text(&template.content, MAX_RESOURCE_BYTES, "prompt")?;
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let directory = prompt_directory(Path::new(&workspace.root), template.scope);
    fs::create_dir_all(&directory).map_err(|error| {
        server_error(format!("Could not create {}: {error}", directory.display()))
    })?;
    let destination = directory.join(format!("{}.md", template.name));
    if original_name.as_deref() != Some(template.name.as_str()) && destination.exists() {
        return Err(client_error(
            "A prompt template with this name already exists",
        ));
    }
    let source = format!(
        "---\ndescription: {}\n{}---\n\n{}\n",
        serde_json::to_string(&template.description).unwrap_or_else(|_| "\"\"".into()),
        if template.argument_hint.is_empty() {
            String::new()
        } else {
            format!(
                "argument-hint: {}\n",
                serde_json::to_string(&template.argument_hint).unwrap_or_else(|_| "\"\"".into())
            )
        },
        template.content.trim()
    );
    write_atomic(&destination, source.as_bytes())?;
    if let Some(original_name) = original_name {
        validate_prompt_name(&original_name)?;
        let original = directory.join(format!("{original_name}.md"));
        if original != destination && original.exists() {
            fs::remove_file(&original).map_err(|error| {
                server_error(format!("Could not remove {}: {error}", original.display()))
            })?;
        }
    }
    Ok(())
}

pub(crate) async fn delete_prompt_template(
    workspace_id: WorkspaceId,
    name: String,
    scope: PiResourceScope,
) -> Result<(), ServerFnError> {
    validate_prompt_name(&name)?;
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let path = prompt_directory(Path::new(&workspace.root), scope).join(format!("{name}.md"));
    fs::remove_file(&path)
        .map_err(|error| server_error(format!("Could not remove {}: {error}", path.display())))
}
