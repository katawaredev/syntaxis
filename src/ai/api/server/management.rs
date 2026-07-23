use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use dioxus::prelude::ServerFnError;
use futures_util::{stream, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use syntaxis_workspace::WorkspaceId;

use crate::ai::{
    api::{
        PiOperationResult, PiPackageAction, PiPackageSearch, PiPackageSummary, PiResourceScope,
        PiSettingsSnapshot, PiSkill, PromptTemplate, SkillCatalogView, SkillSearchPage,
        SkillSearchResult,
    },
    generated_settings::{PiSettingKind, PI_SETTINGS_SCHEMA_VERSION, PI_SETTING_DEFINITIONS},
};

const COMMAND_TIMEOUT: Duration = Duration::from_mins(3);
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);
const PACKAGE_PAGE_SIZE: usize = 20;
const SKILL_PAGE_SIZE: usize = 20;
const MAX_RESOURCE_BYTES: usize = 512 * 1024;
const MAX_SKILL_DOWNLOAD_BYTES: usize = 8 * 1024 * 1024;

pub(crate) async fn pi_packages(
    workspace_id: WorkspaceId,
    query: String,
    offset: usize,
) -> Result<PiPackageSearch, ServerFnError> {
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let installed = configured_packages(Path::new(&workspace.root));
    let client = http_client()?;
    let mut search = "keywords:pi-package".to_owned();
    let query = query.trim();
    if !query.is_empty() {
        search.push(' ');
        search.push_str(query);
    }
    let response = client
        .get("https://registry.npmjs.org/-/v1/search")
        .query(&[
            ("text", search.as_str()),
            ("size", &PACKAGE_PAGE_SIZE.to_string()),
            ("from", &offset.to_string()),
            ("quality", "0"),
            ("popularity", "1"),
            ("maintenance", "0"),
        ])
        .send()
        .await
        .map_err(|error| server_error(format!("Could not search npm: {error}")))?
        .error_for_status()
        .map_err(|error| server_error(format!("npm rejected the package search: {error}")))?
        .json::<Value>()
        .await
        .map_err(|error| server_error(format!("npm returned invalid package data: {error}")))?;
    let catalog_total = response
        .get("total")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or_default();
    let candidates = response
        .get("objects")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let candidate_count = candidates.len();
    let mut packages: Vec<PiPackageSummary> = stream::iter(candidates)
        .map(|candidate| {
            let client = client.clone();
            let installed = installed.clone();
            async move {
                let package = candidate.get("package")?;
                let name = package.get("name")?.as_str()?.to_owned();
                let (manifest, monthly_downloads) = tokio::join!(
                    fetch_manifest(&client, &name),
                    fetch_monthly_downloads(&client, &name)
                );
                let manifest = manifest.ok()?;
                let kinds = package_kinds(&manifest, package);
                Some(package_summary(
                    package,
                    &manifest,
                    &installed,
                    kinds,
                    monthly_downloads.unwrap_or_default(),
                ))
            }
        })
        .buffer_unordered(8)
        .filter_map(std::future::ready)
        .collect()
        .await;
    packages.sort_by(|left, right| {
        right
            .monthly_downloads
            .cmp(&left.monthly_downloads)
            .then_with(|| left.name.cmp(&right.name))
    });
    let next_offset = offset.saturating_add(candidate_count);
    Ok(PiPackageSearch {
        packages,
        catalog_total,
        start_offset: offset,
        next_offset,
        has_more: candidate_count == PACKAGE_PAGE_SIZE && next_offset < catalog_total,
    })
}

pub(crate) async fn manage_pi_package(
    workspace_id: WorkspaceId,
    name: String,
    action: PiPackageAction,
) -> Result<PiOperationResult, ServerFnError> {
    validate_npm_name(&name)?;
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let source = format!("npm:{name}");
    let arguments = match action {
        PiPackageAction::Install => vec!["install", source.as_str(), "--no-approve"],
        PiPackageAction::Uninstall => vec!["remove", source.as_str(), "--no-approve"],
    };
    let output = run_pi(&workspace.root, &arguments, true).await?;
    Ok(PiOperationResult {
        message: if output.is_empty() {
            match action {
                PiPackageAction::Install => format!("Installed {name}"),
                PiPackageAction::Uninstall => format!("Uninstalled {name}"),
            }
        } else {
            output
        },
    })
}

pub(crate) async fn pi_settings(
    workspace_id: WorkspaceId,
) -> Result<PiSettingsSnapshot, ServerFnError> {
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    settings_snapshot(Path::new(&workspace.root)).await
}

pub(crate) async fn update_pi_setting(
    workspace_id: WorkspaceId,
    path: String,
    value: Value,
) -> Result<PiSettingsSnapshot, ServerFnError> {
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let root = Path::new(&workspace.root);
    let version = pi_version(root).await?;
    if version != PI_SETTINGS_SCHEMA_VERSION {
        return Err(client_error(format!(
            "Settings editing supports Pi {PI_SETTINGS_SCHEMA_VERSION}; the server has Pi {version}"
        )));
    }
    let definition = PI_SETTING_DEFINITIONS
        .iter()
        .find(|definition| definition.path == path)
        .ok_or_else(|| client_error("This Pi setting is not exposed by Syntaxis"))?;
    validate_setting_value(definition.kind, &value)?;
    let (node, manager) = settings_manager_module()?;
    let script = r"import { pathToFileURL } from 'node:url';
const [managerPath, cwd, agentDir, setter, encoded] = process.argv.slice(1);
const { SettingsManager } = await import(pathToFileURL(managerPath).href);
const manager = SettingsManager.create(cwd, agentDir, { projectTrusted: false });
let value = JSON.parse(encoded);
if ((setter === 'setDefaultProvider' || setter === 'setDefaultModel') && value === '') value = undefined;
manager[setter](value);
await manager.flush();
const errors = manager.drainErrors();
if (errors.length) throw errors[0].error;";
    let output = tokio::time::timeout(
        COMMAND_TIMEOUT,
        tokio::process::Command::new(node)
            .args(["--input-type=module", "--eval", script])
            .arg(manager)
            .arg(root)
            .arg(agent_dir(root))
            .arg(definition.setter)
            .arg(value.to_string())
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .output(),
    )
    .await
    .map_err(|_| server_error("Pi settings update timed out"))?
    .map_err(|error| server_error(format!("Could not start Pi's settings manager: {error}")))?;
    if !output.status.success() {
        return Err(server_error(command_failure(&output)));
    }
    settings_snapshot(root).await
}

pub(crate) async fn update_pi(
    workspace_id: WorkspaceId,
) -> Result<PiOperationResult, ServerFnError> {
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let output = run_pi(
        &workspace.root,
        &["update", "--self", "--no-approve"],
        false,
    )
    .await?;
    Ok(PiOperationResult {
        message: if output.is_empty() {
            "Pi is already up to date".into()
        } else {
            output
        },
    })
}

fn prompt_directory(root: &Path, scope: PiResourceScope) -> PathBuf {
    match scope {
        PiResourceScope::Global => agent_dir(root).join("prompts"),
        PiResourceScope::Project => root.join(".pi/prompts"),
    }
}

fn skill_directory(root: &Path, scope: PiResourceScope) -> PathBuf {
    match scope {
        PiResourceScope::Global => agent_dir(root).join("skills"),
        PiResourceScope::Project => root.join(".pi/skills"),
    }
}

fn validate_resource_name(name: &str) -> Result<(), ServerFnError> {
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

fn validate_prompt_name(name: &str) -> Result<(), ServerFnError> {
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

fn validate_remote_segment(value: &str) -> Result<(), ServerFnError> {
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

fn validate_resource_text(value: &str, max_bytes: usize, label: &str) -> Result<(), ServerFnError> {
    if value.len() <= max_bytes {
        Ok(())
    } else {
        Err(client_error(format!("The {label} is too large")))
    }
}

fn split_frontmatter(source: &str) -> (&str, &str) {
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

fn metadata_value(metadata: &str, key: &str) -> Option<String> {
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

fn safe_relative_path(value: &str) -> Result<PathBuf, ServerFnError> {
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

fn reject_symlink(path: &Path) -> Result<(), ServerFnError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(client_error("Syntaxis will not modify a linked skill"))
        }
        Ok(_) | Err(_) => Ok(()),
    }
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), ServerFnError> {
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

async fn settings_snapshot(root: &Path) -> Result<PiSettingsSnapshot, ServerFnError> {
    let pi_version = pi_version(root).await?;
    let compatible = pi_version == PI_SETTINGS_SCHEMA_VERSION && settings_manager_module().is_ok();
    let compatibility_message = if pi_version != PI_SETTINGS_SCHEMA_VERSION {
        Some(format!(
            "This Syntaxis build generated its settings UI from Pi {PI_SETTINGS_SCHEMA_VERSION}; the server runs Pi {pi_version}. Update Syntaxis before editing settings."
        ))
    } else if settings_manager_module().is_err() {
        Some("This Pi installation does not expose the SettingsManager module required for locked writes. Reading remains available.".into())
    } else {
        None
    };
    let path = agent_dir(root).join("settings.json");
    let values = match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).map_err(|error| {
            server_error(format!("Could not parse {}: {error}", path.display()))
        })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(error) => {
            return Err(server_error(format!(
                "Could not read {}: {error}",
                path.display()
            )));
        }
    };
    Ok(PiSettingsSnapshot {
        pi_version,
        schema_version: PI_SETTINGS_SCHEMA_VERSION.into(),
        compatible,
        compatibility_message,
        values,
    })
}

fn http_client() -> Result<reqwest::Client, ServerFnError> {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent("syntaxis-pi-package-browser/0.1")
        .build()
        .map_err(|error| server_error(format!("Could not initialize the package browser: {error}")))
}

async fn fetch_manifest(client: &reqwest::Client, name: &str) -> Result<Value, String> {
    let encoded = name.replace('/', "%2f");
    client
        .get(format!("https://registry.npmjs.org/{encoded}/latest"))
        .send()
        .await
        .map_err(|error| format!("Could not load {name}: {error}"))?
        .error_for_status()
        .map_err(|error| format!("npm rejected {name}: {error}"))?
        .json()
        .await
        .map_err(|error| format!("npm returned invalid metadata for {name}: {error}"))
}

async fn fetch_monthly_downloads(client: &reqwest::Client, name: &str) -> Result<u64, String> {
    let encoded = name.replace('/', "%2f");
    client
        .get(format!(
            "https://api.npmjs.org/downloads/point/last-month/{encoded}"
        ))
        .send()
        .await
        .map_err(|error| format!("Could not load download counts for {name}: {error}"))?
        .error_for_status()
        .map_err(|error| format!("npm rejected download counts for {name}: {error}"))?
        .json::<Value>()
        .await
        .map_err(|error| format!("npm returned invalid download counts for {name}: {error}"))?
        .get("downloads")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("npm omitted download counts for {name}"))
}

fn package_summary(
    search: &Value,
    manifest: &Value,
    installed: &BTreeMap<String, BTreeSet<String>>,
    kinds: Vec<String>,
    monthly_downloads: u64,
) -> PiPackageSummary {
    let name = string_at(manifest, "name")
        .or_else(|| string_at(search, "name"))
        .unwrap_or_default();
    let version = string_at(manifest, "version")
        .or_else(|| string_at(search, "version"))
        .unwrap_or_default();
    PiPackageSummary {
        version: version.clone(),
        description: string_at(manifest, "description")
            .or_else(|| string_at(search, "description"))
            .unwrap_or_default(),
        publisher: manifest
            .get("publisher")
            .and_then(|publisher| publisher.get("username"))
            .and_then(Value::as_str)
            .or_else(|| {
                search
                    .get("publisher")
                    .and_then(|publisher| publisher.get("username"))
                    .and_then(Value::as_str)
            })
            .or_else(|| manifest.get("author").and_then(author_name))
            .or_else(|| search.get("author").and_then(author_name))
            .unwrap_or("unknown")
            .to_owned(),
        published_at: string_at(search, "date")
            .or_else(|| {
                search
                    .get("time")
                    .and_then(|time| time.get(&version))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_default(),
        monthly_downloads,
        installed_scopes: installed
            .get(&package_identity(&name))
            .map_or_else(Vec::new, |scopes| scopes.iter().cloned().collect()),
        name,
        kinds,
    }
}

fn package_kinds(manifest: &Value, search: &Value) -> Vec<String> {
    let mut kinds = BTreeSet::new();
    if let Some(pi) = manifest.get("pi") {
        for (field, kind) in [
            ("extensions", "extension"),
            ("skills", "skill"),
            ("prompts", "prompt"),
            ("themes", "theme"),
        ] {
            if !string_array(pi.get(field)).is_empty() {
                kinds.insert(kind.to_owned());
            }
        }
    }
    for keyword in string_array(search.get("keywords")) {
        let normalized = keyword.to_ascii_lowercase();
        for (kind, aliases) in [
            ("extension", ["extension", "pi-extension"]),
            ("skill", ["skill", "pi-skill"]),
            ("prompt", ["prompt", "pi-prompt"]),
            ("theme", ["theme", "pi-theme"]),
        ] {
            if aliases.contains(&normalized.as_str()) {
                kinds.insert(kind.to_owned());
            }
        }
    }
    kinds.into_iter().collect()
}

fn configured_packages(workspace: &Path) -> BTreeMap<String, BTreeSet<String>> {
    let mut installed = BTreeMap::<String, BTreeSet<String>>::new();
    for (scope, path) in [
        ("user", agent_dir(workspace).join("settings.json")),
        ("project", workspace.join(".pi/settings.json")),
    ] {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        let Ok(settings) = serde_json::from_str::<Value>(&contents) else {
            continue;
        };
        let Some(packages) = settings.get("packages").and_then(Value::as_array) else {
            continue;
        };
        for package in packages {
            let source = package
                .as_str()
                .or_else(|| package.get("source").and_then(Value::as_str));
            if let Some(source) = source {
                installed
                    .entry(package_identity(source))
                    .or_default()
                    .insert(scope.into());
            }
        }
    }
    installed
}

fn package_identity(source: &str) -> String {
    let source = source.strip_prefix("npm:").unwrap_or(source);
    let version_separator = source
        .rfind('@')
        .filter(|index| *index > source.rfind('/').unwrap_or_default());
    version_separator
        .map_or(source, |index| &source[..index])
        .to_ascii_lowercase()
}

fn validate_npm_name(name: &str) -> Result<(), ServerFnError> {
    let valid = !name.is_empty()
        && name.len() <= 214
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"@/._-".contains(&byte))
        && (!name.starts_with('@') || name.matches('/').count() == 1);
    if valid {
        Ok(())
    } else {
        Err(client_error("Invalid npm package name"))
    }
}

fn validate_setting_value(kind: PiSettingKind, value: &Value) -> Result<(), ServerFnError> {
    let valid = match kind {
        PiSettingKind::Toggle => value.is_boolean(),
        PiSettingKind::Select(options) => {
            value.as_str().is_some_and(|value| options.contains(&value))
        }
        PiSettingKind::Number => value.as_u64().is_some_and(|value| value <= 86_400_000),
        PiSettingKind::Text => value
            .as_str()
            .is_some_and(|value| value.len() <= 512 && !value.contains(['\n', '\r'])),
        PiSettingKind::StringArray => value.as_array().is_some_and(|values| {
            values.len() <= 64
                && values.iter().all(|value| {
                    value
                        .as_str()
                        .is_some_and(|value| value.len() <= 512 && !value.contains(['\n', '\r']))
                })
        }),
    };
    if valid {
        Ok(())
    } else {
        Err(client_error("Invalid value for this Pi setting"))
    }
}

async fn pi_version(root: &Path) -> Result<String, ServerFnError> {
    let output = run_pi(&root.to_string_lossy(), &["--version"], true).await?;
    Ok(output.trim().trim_start_matches('v').to_owned())
}

async fn run_pi(
    root: &str,
    arguments: &[&str],
    skip_version_check: bool,
) -> Result<String, ServerFnError> {
    let mut command = tokio::process::Command::new(pi_command());
    command
        .args(arguments)
        .current_dir(root)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null());
    if skip_version_check {
        command.env("PI_SKIP_VERSION_CHECK", "1");
    }
    let output = tokio::time::timeout(COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| server_error("Pi command timed out"))?
        .map_err(|error| server_error(format!("Could not start Pi: {error}")))?;
    if !output.status.success() {
        return Err(server_error(command_failure(&output)));
    }
    Ok(truncate(
        &String::from_utf8_lossy(&output.stdout),
        64 * 1024,
    ))
}

fn settings_manager_module() -> Result<(PathBuf, PathBuf), ServerFnError> {
    let command = resolve_command(&pi_command()).ok_or_else(|| {
        server_error("Could not locate the Pi executable used by the agent runtime")
    })?;
    let command = fs::canonicalize(command)
        .map_err(|error| server_error(format!("Could not resolve the Pi executable: {error}")))?;
    let package_root = command
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| server_error("Pi is not installed from a loadable npm package"))?;
    let manager = package_root.join("dist/core/settings-manager.js");
    if !manager.is_file() {
        return Err(server_error(
            "Pi is not installed from a loadable npm package",
        ));
    }
    let node = resolve_command(Path::new("node"))
        .ok_or_else(|| server_error("Node.js is unavailable for Pi settings writes"))?;
    Ok((node, manager))
}

fn resolve_command(command: &Path) -> Option<PathBuf> {
    if command.components().count() > 1 {
        return Some(command.to_owned());
    }
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|path| path.join(command))
            .find(|candidate| candidate.is_file())
    })
}

fn pi_command() -> PathBuf {
    env::var_os("SYNTAXIS_PI_COMMAND").map_or_else(|| PathBuf::from("pi"), PathBuf::from)
}

fn agent_dir(root: &Path) -> PathBuf {
    let directory = env::var_os("PI_CODING_AGENT_DIR").map_or_else(
        || {
            env::var_os("HOME")
                .map_or_else(|| PathBuf::from("."), PathBuf::from)
                .join(".pi/agent")
        },
        PathBuf::from,
    );
    if directory.is_absolute() {
        directory
    } else {
        root.join(directory)
    }
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn string_at(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn author_name(author: &Value) -> Option<&str> {
    author
        .as_str()
        .or_else(|| author.get("name").and_then(Value::as_str))
}

fn truncate(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.trim().to_owned();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n\n…truncated by Syntaxis…", value[..end].trim())
}

fn command_failure(output: &std::process::Output) -> String {
    let stderr = truncate(&String::from_utf8_lossy(&output.stderr), 16 * 1024);
    let stdout = truncate(&String::from_utf8_lossy(&output.stdout), 16 * 1024);
    if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("Pi exited with {}", output.status)
    }
}

fn client_error(message: impl Into<String>) -> ServerFnError {
    ServerFnError::ServerError {
        message: message.into(),
        code: 400,
        details: None,
    }
}

fn server_error(message: impl Into<String>) -> ServerFnError {
    ServerFnError::ServerError {
        message: message.into(),
        code: 500,
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_package_identity_ignores_source_prefix_and_version() {
        assert_eq!(package_identity("npm:pi-web-access@1.2.3"), "pi-web-access");
        assert_eq!(
            package_identity("npm:@scope/pi-extension@2.0.0"),
            "@scope/pi-extension"
        );
        assert_eq!(
            package_identity("@scope/pi-extension"),
            "@scope/pi-extension"
        );
    }

    #[test]
    fn manifest_resources_identify_extensions_without_keyword_hints() {
        let manifest = json!({
            "pi": {
                "extensions": ["./index.ts"],
                "skills": ["./skills"]
            }
        });
        assert_eq!(package_kinds(&manifest, &json!({})), ["extension", "skill"]);
    }

    #[test]
    fn generated_setting_values_are_type_checked() {
        assert!(validate_setting_value(PiSettingKind::Toggle, &json!(true)).is_ok());
        assert!(validate_setting_value(PiSettingKind::Toggle, &json!("true")).is_err());
        assert!(
            validate_setting_value(PiSettingKind::Select(&["auto", "sse"]), &json!("auto")).is_ok()
        );
        assert!(
            validate_setting_value(PiSettingKind::Select(&["auto", "sse"]), &json!("other"))
                .is_err()
        );
        assert!(
            validate_setting_value(PiSettingKind::StringArray, &json!(["mise", "npm"])).is_ok()
        );
        assert!(validate_setting_value(PiSettingKind::StringArray, &json!("npm")).is_err());
    }

    #[test]
    fn pi_resource_names_and_download_paths_are_restricted() {
        assert!(validate_resource_name("code-review").is_ok());
        assert!(validate_resource_name("../review").is_err());
        assert!(validate_resource_name("CodeReview").is_err());
        assert!(validate_prompt_name("review_PR.v2").is_ok());
        assert!(validate_prompt_name("../review").is_err());
        assert!(safe_relative_path("references/guide.md").is_ok());
        assert!(safe_relative_path("../SKILL.md").is_err());
        assert!(safe_relative_path("/tmp/SKILL.md").is_err());
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
