use super::*;

const SKILL_SOURCE_FILE: &str = ".syntaxis-source.json";

#[derive(Deserialize, Serialize)]
struct InstalledSkillSource {
    slug: String,
}

pub(crate) async fn pi_skills(workspace_id: WorkspaceId) -> Result<Vec<PiSkill>, ServerFnError> {
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let root = Path::new(&workspace.root);
    let mut skills = Vec::new();
    for scope in [PiResourceScope::Global, PiResourceScope::Project] {
        let directory = skill_directory(root, scope);
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let single_file = !path.is_dir();
            let skill_file = if single_file {
                if path.extension().and_then(|value| value.to_str()) != Some("md") {
                    continue;
                }
                path.clone()
            } else {
                path.join("SKILL.md")
            };
            if !skill_file.is_file() {
                continue;
            }
            let Ok(source) = fs::read_to_string(skill_file) else {
                continue;
            };
            let (metadata, content) = split_frontmatter(&source);
            let Some(name) = metadata_value(metadata, "name") else {
                continue;
            };
            let Some(description) = metadata_value(metadata, "description") else {
                continue;
            };
            skills.push(PiSkill {
                name,
                description,
                content: content.trim().to_owned(),
                scope,
                storage_name: path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_owned(),
                single_file,
                extra_frontmatter: metadata
                    .lines()
                    .filter(|line| {
                        line.split_once(':')
                            .is_none_or(|(key, _)| !matches!(key.trim(), "name" | "description"))
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            });
        }
    }
    skills.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.scope.cmp(&right.scope))
    });
    Ok(skills)
}

pub(crate) async fn save_pi_skill(
    workspace_id: WorkspaceId,
    original_name: Option<String>,
    skill: PiSkill,
) -> Result<(), ServerFnError> {
    validate_resource_name(&skill.name)?;
    validate_resource_text(&skill.description, 1024, "description")?;
    if skill.description.trim().is_empty() {
        return Err(client_error("A skill description is required"));
    }
    validate_resource_text(&skill.content, MAX_RESOURCE_BYTES, "skill instructions")?;
    validate_resource_text(&skill.extra_frontmatter, 16 * 1024, "skill frontmatter")?;
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let root = skill_directory(Path::new(&workspace.root), skill.scope);
    fs::create_dir_all(&root)
        .map_err(|error| server_error(format!("Could not create {}: {error}", root.display())))?;
    let destination = if skill.single_file {
        root.join(format!("{}.md", skill.name))
    } else {
        root.join(&skill.name)
    };
    reject_symlink(&destination)?;
    let expected_storage_name = destination
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if original_name.as_deref() != Some(expected_storage_name) && destination.exists() {
        return Err(client_error("A skill with this name already exists"));
    }
    if let Some(original_name) = original_name.as_deref() {
        validate_resource_name(original_name)?;
        let original = if skill.single_file {
            root.join(format!("{original_name}.md"))
        } else {
            root.join(original_name)
        };
        reject_symlink(&original)?;
        if original != destination && original.exists() {
            fs::rename(&original, &destination).map_err(|error| {
                server_error(format!("Could not rename {}: {error}", original.display()))
            })?;
        }
    }
    let skill_file = if skill.single_file {
        destination.clone()
    } else {
        fs::create_dir_all(&destination).map_err(|error| {
            server_error(format!(
                "Could not create {}: {error}",
                destination.display()
            ))
        })?;
        destination.join("SKILL.md")
    };
    let source = format!(
        "---\nname: {}\ndescription: {}\n{}---\n\n{}\n",
        serde_json::to_string(&skill.name).unwrap_or_else(|_| "\"\"".into()),
        serde_json::to_string(&skill.description).unwrap_or_else(|_| "\"\"".into()),
        if skill.extra_frontmatter.is_empty() {
            String::new()
        } else {
            format!("{}\n", skill.extra_frontmatter.trim())
        },
        skill.content.trim()
    );
    write_atomic(&skill_file, source.as_bytes())?;
    Ok(())
}

pub(crate) async fn delete_pi_skill(
    workspace_id: WorkspaceId,
    storage_name: String,
    scope: PiResourceScope,
    single_file: bool,
) -> Result<(), ServerFnError> {
    validate_resource_name(&storage_name)?;
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let path = skill_directory(Path::new(&workspace.root), scope).join(if single_file {
        format!("{storage_name}.md")
    } else {
        storage_name
    });
    reject_symlink(&path)?;
    if single_file {
        fs::remove_file(&path)
    } else {
        fs::remove_dir_all(&path)
    }
    .map_err(|error| server_error(format!("Could not remove {}: {error}", path.display())))
}

pub(crate) async fn search_pi_skills(
    query: String,
    offset: usize,
) -> Result<SkillSearchPage, ServerFnError> {
    let query = query.trim();
    if query.len() < 2 || query.len() > 100 {
        return Ok(SkillSearchPage {
            skills: Vec::new(),
            start_offset: 0,
            next_offset: 0,
            has_more: false,
        });
    }
    let requested = offset.saturating_add(SKILL_PAGE_SIZE).min(100);
    let response = http_client()?
        .get("https://skills.sh/api/search")
        .query(&[("q", query), ("limit", &requested.to_string())])
        .send()
        .await
        .map_err(|error| server_error(format!("Could not search skills.sh: {error}")))?
        .error_for_status()
        .map_err(|error| server_error(format!("skills.sh rejected the search: {error}")))?
        .json::<SkillSearchResponse>()
        .await
        .map_err(|error| server_error(format!("skills.sh returned invalid data: {error}")))?;
    let result_count = response.skills.len();
    let skills = response
        .skills
        .into_iter()
        .skip(offset)
        .map(|skill| SkillSearchResult {
            name: skill.name,
            page_url: skill_page_url(&skill.source, &skill.id),
            installable: skill.source.split('/').count() == 2,
            slug: skill.id,
            source: skill.source,
            installs: skill.installs,
        })
        .collect::<Vec<_>>();
    let next_offset = offset.saturating_add(skills.len());
    Ok(SkillSearchPage {
        skills,
        start_offset: offset,
        next_offset,
        has_more: requested < 100 && result_count == requested && next_offset > offset,
    })
}

pub(crate) async fn browse_pi_skills(
    view: SkillCatalogView,
    offset: usize,
) -> Result<SkillSearchPage, ServerFnError> {
    let token = env::var("VERCEL_OIDC_TOKEN")
        .map_err(|_| client_error("Set VERCEL_OIDC_TOKEN to enable the skills.sh leaderboard"))?;
    fetch_authenticated_skill_page(view, offset, &token).await
}

pub(crate) fn skill_catalog_available() -> bool {
    env::var_os("VERCEL_OIDC_TOKEN").is_some()
}

async fn fetch_authenticated_skill_page(
    view: SkillCatalogView,
    offset: usize,
    token: &str,
) -> Result<SkillSearchPage, ServerFnError> {
    let view = match view {
        SkillCatalogView::AllTime => "all-time",
        SkillCatalogView::Trending => "trending",
        SkillCatalogView::Hot => "hot",
    };
    let response = http_client()?
        .get("https://www.skills.sh/api/v1/skills")
        .bearer_auth(token)
        .query(&[
            ("view", view),
            ("page", &(offset / SKILL_PAGE_SIZE).to_string()),
            ("per_page", &SKILL_PAGE_SIZE.to_string()),
        ])
        .send()
        .await
        .map_err(|error| server_error(format!("Could not load skills.sh: {error}")))?
        .error_for_status()
        .map_err(|error| server_error(format!("skills.sh rejected the catalog request: {error}")))?
        .json::<SkillV1Response>()
        .await
        .map_err(|error| {
            server_error(format!("skills.sh returned invalid catalog data: {error}"))
        })?;
    let skills = response
        .data
        .into_iter()
        .map(|skill| SkillSearchResult {
            name: skill.name,
            page_url: skill.url,
            installable: skill.source.split('/').count() == 2,
            slug: skill.id,
            source: skill.source,
            installs: skill.installs,
        })
        .collect::<Vec<_>>();
    let next_offset = offset.saturating_add(skills.len());
    Ok(SkillSearchPage {
        skills,
        start_offset: offset,
        next_offset,
        has_more: response.pagination.has_more && next_offset > offset,
    })
}

fn skill_page_url(source: &str, slug: &str) -> String {
    if source.contains('/') {
        format!("https://www.skills.sh/{slug}")
    } else {
        let skill_id = slug.rsplit('/').next().unwrap_or(slug);
        format!("https://www.skills.sh/site/{source}/{skill_id}")
    }
}

pub(crate) async fn install_pi_skill(
    workspace_id: WorkspaceId,
    slug: String,
    scope: PiResourceScope,
) -> Result<(), ServerFnError> {
    let (_, _, storage_name) = validate_skill_slug(&slug)?;
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let root = skill_directory(Path::new(&workspace.root), scope);
    let destination = root.join(storage_name);
    reject_symlink(&destination)?;
    if destination.exists() {
        return Err(client_error(
            "This skill is already installed in that scope",
        ));
    }
    let files = download_skill(&slug).await?;
    if let Err(error) = write_downloaded_skill(&destination, &slug, files) {
        let _ = fs::remove_dir_all(&destination);
        return Err(error);
    }
    Ok(())
}

pub(crate) async fn update_tracked_pi_skills(
    workspace_root: &Path,
) -> Result<usize, ServerFnError> {
    let mut tracked = Vec::new();
    for scope in [PiResourceScope::Global, PiResourceScope::Project] {
        tracked.extend(tracked_skills(&skill_directory(workspace_root, scope))?);
    }
    for (destination, slug) in &tracked {
        let files = download_skill(slug).await?;
        replace_downloaded_skill(destination, slug, files)?;
    }
    Ok(tracked.len())
}

fn validate_skill_slug(slug: &str) -> Result<(&str, &str, &str), ServerFnError> {
    let mut parts = slug.split('/');
    let owner = parts
        .next()
        .ok_or_else(|| client_error("Invalid skills.sh skill identifier"))?;
    let repository = parts
        .next()
        .ok_or_else(|| client_error("Invalid skills.sh skill identifier"))?;
    let skill = parts
        .next()
        .ok_or_else(|| client_error("Invalid skills.sh skill identifier"))?;
    if parts.next().is_some() {
        return Err(client_error("Invalid skills.sh skill identifier"));
    }
    validate_remote_segment(owner)?;
    validate_remote_segment(repository)?;
    validate_resource_name(skill)?;
    Ok((owner, repository, skill))
}

async fn download_skill(slug: &str) -> Result<Vec<(PathBuf, String)>, ServerFnError> {
    let (owner, repository, skill) = validate_skill_slug(slug)?;
    let response = http_client()?
        .get(format!(
            "https://skills.sh/api/download/{}/{}/{}",
            owner, repository, skill
        ))
        .send()
        .await
        .map_err(|error| server_error(format!("Could not download skill: {error}")))?
        .error_for_status()
        .map_err(|error| server_error(format!("skills.sh rejected the download: {error}")))?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_SKILL_DOWNLOAD_BYTES as u64)
    {
        return Err(server_error("The skill download is too large"));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| server_error(format!("Could not read skill download: {error}")))?;
    if bytes.len() > MAX_SKILL_DOWNLOAD_BYTES {
        return Err(server_error("The skill download is too large"));
    }
    let snapshot: SkillDownload = serde_json::from_slice(&bytes)
        .map_err(|error| server_error(format!("skills.sh returned invalid data: {error}")))?;
    if !snapshot
        .files
        .iter()
        .any(|file| file.path.eq_ignore_ascii_case("SKILL.md"))
    {
        return Err(server_error("The downloaded skill has no SKILL.md"));
    }
    snapshot
        .files
        .into_iter()
        .map(|file| {
            let path = safe_relative_path(&file.path)?;
            validate_resource_text(&file.contents, MAX_RESOURCE_BYTES, "skill file")?;
            Ok((path, file.contents))
        })
        .collect::<Result<Vec<_>, ServerFnError>>()
}

fn write_downloaded_skill(
    destination: &Path,
    slug: &str,
    files: Vec<(PathBuf, String)>,
) -> Result<(), ServerFnError> {
    fs::create_dir_all(&destination).map_err(|error| {
        server_error(format!(
            "Could not create {}: {error}",
            destination.display()
        ))
    })?;
    for (relative, contents) in files {
        let path = destination.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                server_error(format!("Could not create {}: {error}", parent.display()))
            })?;
        }
        write_atomic(&path, contents.as_bytes())?;
    }
    let source = serde_json::to_vec_pretty(&InstalledSkillSource {
        slug: slug.to_owned(),
    })
    .map_err(|error| server_error(format!("Could not record skill source: {error}")))?;
    write_atomic(&destination.join(SKILL_SOURCE_FILE), &source)?;
    Ok(())
}

fn tracked_skills(root: &Path) -> Result<Vec<(PathBuf, String)>, ServerFnError> {
    let Ok(entries) = fs::read_dir(root) else {
        return Ok(Vec::new());
    };
    let mut tracked = Vec::new();
    for entry in entries.flatten() {
        let destination = entry.path();
        if !destination.is_dir() || destination.is_symlink() {
            continue;
        }
        let marker = destination.join(SKILL_SOURCE_FILE);
        let Ok(source) = fs::read(&marker) else {
            continue;
        };
        let source: InstalledSkillSource = serde_json::from_slice(&source).map_err(|error| {
            server_error(format!("Could not read {}: {error}", marker.display()))
        })?;
        let (_, _, storage_name) = validate_skill_slug(&source.slug)?;
        if destination.file_name().and_then(|name| name.to_str()) != Some(storage_name) {
            return Err(server_error(format!(
                "Tracked skill source does not match {}",
                destination.display()
            )));
        }
        tracked.push((destination, source.slug));
    }
    tracked.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(tracked)
}

fn replace_downloaded_skill(
    destination: &Path,
    slug: &str,
    files: Vec<(PathBuf, String)>,
) -> Result<(), ServerFnError> {
    reject_symlink(destination)?;
    let parent = destination
        .parent()
        .ok_or_else(|| server_error("Skill directory has no parent"))?;
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| server_error("Skill directory has an invalid name"))?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staging = parent.join(format!(".{name}.syntaxis-update-{nonce}"));
    let backup = parent.join(format!(".{name}.syntaxis-backup-{nonce}"));
    if let Err(error) = write_downloaded_skill(&staging, slug, files) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    fs::rename(destination, &backup).map_err(|error| {
        let _ = fs::remove_dir_all(&staging);
        server_error(format!(
            "Could not prepare {} for update: {error}",
            destination.display()
        ))
    })?;
    if let Err(error) = fs::rename(&staging, destination) {
        let _ = fs::rename(&backup, destination);
        let _ = fs::remove_dir_all(&staging);
        return Err(server_error(format!(
            "Could not update {}: {error}",
            destination.display()
        )));
    }
    fs::remove_dir_all(&backup).map_err(|error| {
        server_error(format!(
            "Updated the skill but could not remove {}: {error}",
            backup.display()
        ))
    })
}

#[derive(Deserialize)]
struct SkillSearchResponse {
    skills: Vec<SkillSearchItem>,
}

#[derive(Deserialize)]
struct SkillSearchItem {
    id: String,
    name: String,
    source: String,
    installs: u64,
}

#[derive(Deserialize)]
struct SkillV1Response {
    data: Vec<SkillV1Item>,
    pagination: SkillV1Pagination,
}

#[derive(Deserialize)]
struct SkillV1Item {
    id: String,
    name: String,
    source: String,
    installs: u64,
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillV1Pagination {
    has_more: bool,
}

#[derive(Deserialize)]
struct SkillDownload {
    files: Vec<SkillDownloadFile>,
}

#[derive(Deserialize)]
struct SkillDownloadFile {
    path: String,
    contents: String,
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn downloaded_skills_record_their_source_for_future_updates() {
        let root = tempdir().unwrap();
        let destination = root.path().join("example");
        write_downloaded_skill(
            &destination,
            "owner/repository/example",
            vec![(PathBuf::from("SKILL.md"), "# Example".into())],
        )
        .unwrap();

        assert_eq!(
            tracked_skills(root.path()).unwrap(),
            vec![(destination, "owner/repository/example".into())]
        );
    }

    #[test]
    fn replacing_a_downloaded_skill_removes_files_missing_upstream() {
        let root = tempdir().unwrap();
        let destination = root.path().join("example");
        write_downloaded_skill(
            &destination,
            "owner/repository/example",
            vec![
                (PathBuf::from("SKILL.md"), "old".into()),
                (PathBuf::from("removed.md"), "stale".into()),
            ],
        )
        .unwrap();

        replace_downloaded_skill(
            &destination,
            "owner/repository/example",
            vec![(PathBuf::from("SKILL.md"), "new".into())],
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(destination.join("SKILL.md")).unwrap(),
            "new"
        );
        assert!(!destination.join("removed.md").exists());
        assert_eq!(tracked_skills(root.path()).unwrap().len(), 1);
    }
}
