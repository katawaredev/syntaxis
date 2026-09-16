//! Project-local Markdown resources. No host home directory or native executables.

use super::*;
use crate::ai_resource_format::{expand_template, metadata, split_markdown};
use syntaxis_module_ai::{
    AiCommand, AiPromptTemplate, AiResourceScope, AiResourcesPort, AiSkill, AiSkillCatalogView,
    AiSkillSearchPage,
};
use syntaxis_workspace::{EntryKind, ErrorCode, RelativePath, WorkspaceFiles};

const LIMIT: u64 = 128 * 1024;
const MAX_RESOURCES: usize = 100;

fn invalid(message: &str) -> AppError {
    ai_error(AppErrorCode::InvalidInput, message)
}

fn project(scope: AiResourceScope) -> Result<(), AppError> {
    if scope == AiResourceScope::Project {
        Ok(())
    } else {
        Err(invalid(
            "Browser resources belong to this workspace; global resources require the server runtime.",
        ))
    }
}

fn name(value: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > 100
        || value.starts_with('.')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(invalid(
            "Use a resource name containing letters, numbers, dots, underscores, or hyphens.",
        ));
    }
    Ok(())
}

fn path(value: &str) -> Result<RelativePath, AppError> {
    RelativePath::try_from(value).map_err(AppError::from)
}

fn unsupported() -> AppError {
    AppError::unsupported(
        "Browser skills are local Markdown files. Remote installation and native extensions require the server runtime.",
        ErrorSource::Ai,
    )
}

impl BrowserAiAdapter {
    fn resource_files(&self) -> crate::BrowserWorkspaceFiles {
        crate::BrowserWorkspaceFiles::new(self.workspace_events.clone())
    }

    async fn read_resource(
        &self,
        workspace: &WorkspaceRecord,
        location: &str,
    ) -> Result<Option<String>, AppError> {
        match self
            .resource_files()
            .read_text(workspace, &path(location)?, LIMIT)
            .await
        {
            Ok(file) => Ok(Some(file.content)),
            Err(error) if error.code == ErrorCode::NotFound => Ok(None),
            Err(error) => Err(ai_error(
                AppErrorCode::InvalidInput,
                format!("Could not load {location}: {error}"),
            )),
        }
    }

    async fn resource_entries(
        &self,
        workspace: &WorkspaceRecord,
        location: &str,
    ) -> Result<Vec<syntaxis_workspace::FileEntry>, AppError> {
        match self
            .resource_files()
            .list(workspace, &path(location)?)
            .await
        {
            Ok(mut entries) => {
                if entries.len() > MAX_RESOURCES {
                    return Err(invalid(
                        "A browser resource directory may contain at most 100 entries.",
                    ));
                }
                entries.sort_by(|left, right| left.name.cmp(&right.name));
                Ok(entries)
            }
            Err(error) if error.code == ErrorCode::NotFound => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }

    async fn write_resource(
        &self,
        workspace: &WorkspaceRecord,
        location: &str,
        content: &str,
        create: bool,
    ) -> Result<(), AppError> {
        if content.len() > LIMIT as usize {
            return Err(invalid(
                "A browser AI resource may contain at most 128 KiB.",
            ));
        }
        let files = self.resource_files();
        let destination = path(location)?;
        let previous = match files.read_text(workspace, &destination, LIMIT).await {
            Ok(file) => Some(file),
            Err(error) if error.code == ErrorCode::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if create && previous.is_some() {
            return Err(invalid("A resource with this name already exists."));
        }
        let parts = location.split('/').collect::<Vec<_>>();
        for index in 1..parts.len() {
            let directory = path(&parts[..index].join("/"))?;
            match files.stat(workspace, &directory).await {
                Ok(entry) if entry.kind == EntryKind::Directory => {}
                Ok(_) => return Err(invalid("A resource parent is not a directory.")),
                Err(error) if error.code == ErrorCode::NotFound => {
                    files.create_directory(workspace, &directory).await?;
                }
                Err(error) => return Err(error.into()),
            }
        }
        files
            .write_text(
                workspace,
                &destination,
                content,
                previous.as_ref().map(|file| &file.version),
                LIMIT,
            )
            .await?;
        Ok(())
    }

    pub(super) async fn resource_commands(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<Vec<AiCommand>, AppError> {
        let mut commands = self
            .prompt_templates(workspace)
            .await?
            .into_iter()
            .map(|template| AiCommand {
                name: template.name,
                description: template.description,
                argument_hint: Some(template.argument_hint),
                interactive: false,
            })
            .collect::<Vec<_>>();
        commands.extend(
            self.skills(workspace)
                .await?
                .into_iter()
                .map(|skill| AiCommand {
                    name: format!("skill:{}", skill.name),
                    description: skill.description,
                    argument_hint: Some("[task]".into()),
                    interactive: false,
                }),
        );
        Ok(commands)
    }

    pub(super) async fn prepare_resources(
        &self,
        workspace: &WorkspaceRecord,
        prompt: &str,
    ) -> Result<(String, String), AppError> {
        let mut instructions = String::new();
        for location in ["AGENTS.md", ".pi/APPEND_SYSTEM.md"] {
            if let Some(content) = self.read_resource(workspace, location).await? {
                instructions.push_str(&format!(
                    "\n\nWorkspace instructions ({location}):\n{content}"
                ));
            }
        }
        let skills = self.skills(workspace).await?;
        if !skills.is_empty() {
            instructions.push_str("\n\nAvailable workspace skills (Markdown guidance only). When a task matches a skill, read its SKILL.md before using it. Resolve its references relative to the skill file. Never claim native scripts or extensions are supported:\n");
            for skill in &skills {
                instructions.push_str(&format!(
                    "- {}: {} (path: {})\n",
                    skill.name,
                    skill.description,
                    skill_location(skill)?
                ));
            }
        }
        let mut expanded = prompt.to_owned();
        if let Some(command) = prompt.strip_prefix('/') {
            let (command, arguments) = command
                .split_once(char::is_whitespace)
                .unwrap_or((command, ""));
            if let Some(skill_name) = command.strip_prefix("skill:") {
                let skill = skills
                    .iter()
                    .find(|skill| skill.name == skill_name)
                    .ok_or_else(|| invalid("This workspace skill no longer exists."))?;
                expanded = format!(
                    "Use workspace skill {} from {}.\n\n{}\n\nTask: {}",
                    skill.name,
                    skill_location(skill)?,
                    skill.content,
                    arguments.trim()
                );
            } else if let Some(template) = self
                .prompt_templates(workspace)
                .await?
                .iter()
                .find(|template| template.name == command)
            {
                expanded = expand_template(&template.content, arguments.trim());
            }
        }
        if instructions.len() > MAX_CONTEXT_BYTES || expanded.len() > MAX_CONTEXT_BYTES {
            return Err(invalid(
                "Workspace instructions or the expanded prompt exceed the 128 KiB context limit.",
            ));
        }
        Ok((instructions, expanded))
    }
}

fn skill_location(skill: &AiSkill) -> Result<String, AppError> {
    name(&skill.storage_name)?;
    Ok(if skill.single_file {
        format!(".pi/skills/{}.md", skill.storage_name)
    } else {
        format!(".pi/skills/{}/SKILL.md", skill.storage_name)
    })
}

#[async_trait(?Send)]
impl AiResourcesPort for BrowserAiAdapter {
    fn supports_global_resources(&self) -> bool {
        false
    }
    fn supports_skill_discovery(&self) -> bool {
        false
    }

    async fn load_instructions(&self, workspace: &WorkspaceRecord) -> Result<String, AppError> {
        Ok(self
            .read_resource(workspace, "AGENTS.md")
            .await?
            .unwrap_or_default())
    }

    async fn save_instructions(
        &self,
        workspace: &WorkspaceRecord,
        content: &str,
    ) -> Result<(), AppError> {
        self.write_resource(workspace, "AGENTS.md", content, false)
            .await
    }

    async fn prompt_templates(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<Vec<AiPromptTemplate>, AppError> {
        let mut templates = Vec::new();
        for entry in self.resource_entries(workspace, ".pi/prompts").await? {
            if entry.kind != EntryKind::File {
                continue;
            }
            let Some(stem) = entry.name.strip_suffix(".md") else {
                continue;
            };
            name(stem)?;
            let Some(source) = self.read_resource(workspace, entry.path.as_str()).await? else {
                continue;
            };
            let (front, content) = split_markdown(&source);
            templates.push(AiPromptTemplate {
                name: stem.into(),
                description: metadata(&front, "description").unwrap_or_default(),
                argument_hint: metadata(&front, "argument-hint").unwrap_or_default(),
                content,
                scope: AiResourceScope::Project,
            });
        }
        Ok(templates)
    }

    async fn save_prompt_template(
        &self,
        workspace: &WorkspaceRecord,
        original_name: Option<&str>,
        template: AiPromptTemplate,
    ) -> Result<(), AppError> {
        project(template.scope)?;
        name(&template.name)?;
        // Stable storage names avoid destructive rename/overwrite races.
        if original_name.is_some_and(|original| original != template.name) {
            return Err(invalid(
                "Create a copy with a new name instead of renaming a browser template.",
            ));
        }
        let source = format!(
            "---\ndescription: {}\nargument-hint: {}\n---\n\n{}",
            serde_json::json!(template.description),
            serde_json::json!(template.argument_hint),
            template.content
        );
        self.write_resource(
            workspace,
            &format!(".pi/prompts/{}.md", template.name),
            &source,
            original_name.is_none(),
        )
        .await
    }

    async fn delete_prompt_template(
        &self,
        workspace: &WorkspaceRecord,
        template: &AiPromptTemplate,
    ) -> Result<(), AppError> {
        project(template.scope)?;
        name(&template.name)?;
        self.resource_files()
            .delete(
                workspace,
                &path(&format!(".pi/prompts/{}.md", template.name))?,
            )
            .await?;
        Ok(())
    }

    async fn skills(&self, workspace: &WorkspaceRecord) -> Result<Vec<AiSkill>, AppError> {
        let mut skills = Vec::new();
        for entry in self.resource_entries(workspace, ".pi/skills").await? {
            let (storage, single_file, location) = match entry.kind {
                EntryKind::Directory => (
                    entry.name.clone(),
                    false,
                    format!("{}/SKILL.md", entry.path.as_str()),
                ),
                EntryKind::File if entry.name.ends_with(".md") => (
                    entry.name.trim_end_matches(".md").into(),
                    true,
                    entry.path.as_str().into(),
                ),
                _ => continue,
            };
            name(&storage)?;
            let Some(source) = self.read_resource(workspace, &location).await? else {
                continue;
            };
            let (front, content) = split_markdown(&source);
            let skill_name = metadata(&front, "name").unwrap_or_else(|| storage.clone());
            name(&skill_name)?;
            if skills
                .iter()
                .any(|skill: &AiSkill| skill.name == skill_name)
            {
                return Err(invalid("Workspace skills must have unique names."));
            }
            skills.push(AiSkill {
                name: skill_name,
                description: metadata(&front, "description").unwrap_or_default(),
                content,
                scope: AiResourceScope::Project,
                storage_name: storage,
                single_file,
                extra_frontmatter: front
                    .lines()
                    .filter(|line| {
                        line.split_once(':')
                            .is_none_or(|(key, _)| !matches!(key.trim(), "name" | "description"))
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            });
        }
        Ok(skills)
    }

    async fn save_skill(
        &self,
        workspace: &WorkspaceRecord,
        original_storage_name: Option<&str>,
        skill: AiSkill,
    ) -> Result<(), AppError> {
        project(skill.scope)?;
        name(&skill.name)?;
        if skill.description.trim().is_empty() {
            return Err(invalid("A skill description is required."));
        }
        if self.skills(workspace).await?.iter().any(|existing| {
            existing.name == skill.name
                && (Some(existing.storage_name.as_str()) != original_storage_name
                    || existing.single_file != skill.single_file)
        }) {
            return Err(invalid("A skill with this name already exists."));
        }
        if original_storage_name.is_some_and(|original| original != skill.storage_name) {
            return Err(invalid("The skill storage location cannot be changed."));
        }
        if skill.extra_frontmatter.lines().any(|line| {
            line.trim() == "---"
                || line
                    .split_once(':')
                    .is_some_and(|(key, _)| matches!(key.trim(), "name" | "description"))
        }) {
            return Err(invalid(
                "Extra frontmatter cannot redefine name, description, or Markdown delimiters.",
            ));
        }
        let source = format!(
            "---\nname: {}\ndescription: {}\n{}\n---\n\n{}",
            serde_json::json!(skill.name),
            serde_json::json!(skill.description),
            skill.extra_frontmatter,
            skill.content
        );
        self.write_resource(
            workspace,
            &skill_location(&skill)?,
            &source,
            original_storage_name.is_none(),
        )
        .await
    }

    async fn delete_skill(
        &self,
        workspace: &WorkspaceRecord,
        skill: &AiSkill,
    ) -> Result<(), AppError> {
        project(skill.scope)?;
        // Delete only the instruction file, never recursively delete references or scripts.
        self.resource_files()
            .delete(workspace, &path(&skill_location(skill)?)?)
            .await?;
        Ok(())
    }

    async fn skill_catalog_available(&self) -> Result<bool, AppError> {
        Ok(false)
    }
    async fn search_skills(
        &self,
        _query: &str,
        _offset: usize,
    ) -> Result<AiSkillSearchPage, AppError> {
        Err(unsupported())
    }
    async fn browse_skills(
        &self,
        _view: AiSkillCatalogView,
        _offset: usize,
    ) -> Result<AiSkillSearchPage, AppError> {
        Err(unsupported())
    }
    async fn install_skill(
        &self,
        _workspace: &WorkspaceRecord,
        _slug: &str,
        _scope: AiResourceScope,
    ) -> Result<(), AppError> {
        Err(unsupported())
    }
}
