use super::*;

const PACKAGE_PAGE_SIZE: usize = 20;

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
        .map_err(|error| format!("npm returned invalid package data: {error}"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_identity_ignores_source_prefix_and_version() {
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
}
