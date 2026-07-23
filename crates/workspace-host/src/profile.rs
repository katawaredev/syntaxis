use std::{collections::HashMap, fs, path::Path};

use linguist_types::LanguageType;
use serde_json::Value as JsonValue;
use syntaxis_workspace::{WorkspaceLanguage, WorkspaceProfile, WorkspaceTechnology as Technology};

use crate::watcher::is_ignored_path;

const MAX_SCANNED_FILES: usize = 100_000;
const MAX_HEURISTIC_BYTES: u64 = 256 * 1024;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

pub fn detect_workspace_profile(root: &Path) -> WorkspaceProfile {
    WorkspaceProfile {
        technologies: detect_technologies(root),
        languages: detect_languages(root),
    }
}

fn detect_languages(root: &Path) -> Vec<WorkspaceLanguage> {
    let mut totals = HashMap::<String, u64>::new();
    let mut pending = vec![root.to_path_buf()];
    let mut scanned = 0_usize;

    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if scanned >= MAX_SCANNED_FILES {
                break;
            }
            let path = entry.path();
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() || is_ignored_path(relative) {
                continue;
            }
            if file_type.is_dir() {
                pending.push(path);
                continue;
            }
            if !file_type.is_file() || linguist::is_vendored(relative).unwrap_or(false) {
                continue;
            }
            scanned = scanned.saturating_add(1);
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let candidates = linguist::detect_language_by_filename(relative)
                .ok()
                .filter(|languages| !languages.is_empty())
                .or_else(|| linguist::detect_language_by_extension(relative).ok())
                .unwrap_or_default();
            let detected = if candidates.len() > 1 && metadata.len() <= MAX_HEURISTIC_BYTES {
                fs::read_to_string(&path)
                    .ok()
                    .and_then(|contents| linguist::disambiguate(relative, &contents).ok())
                    .and_then(|languages| languages.into_iter().next())
                    .or_else(|| candidates.first().cloned())
            } else {
                candidates.first().cloned()
            };
            let Some(detected) = detected else {
                continue;
            };
            if !matches!(
                detected.definition.language_type,
                LanguageType::Programming | LanguageType::Markup
            ) {
                continue;
            }
            let name = detected
                .definition
                .group
                .as_deref()
                .unwrap_or(detected.name)
                .to_owned();
            let total = totals.entry(name).or_default();
            *total = total.saturating_add(metadata.len());
        }
        if scanned >= MAX_SCANNED_FILES {
            break;
        }
    }

    let mut languages = totals
        .into_iter()
        .map(|(name, bytes)| WorkspaceLanguage { name, bytes })
        .collect::<Vec<_>>();
    languages.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left.name.cmp(&right.name))
    });
    languages
}

#[expect(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    reason = "technology detection is a linear declarative catalog kept together for auditability"
)]
fn detect_technologies(root: &Path) -> Vec<Technology> {
    let mut technologies = Vec::new();
    let package = read_json(&root.join("package.json"));
    let package_manager = package
        .as_ref()
        .and_then(|value| value.get("packageManager"))
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    let dependencies = package.as_ref().map_or_else(Vec::new, package_dependencies);

    if root.join("bun.lock").exists()
        || root.join("bun.lockb").exists()
        || package_manager.starts_with("bun@")
    {
        push_unique(&mut technologies, Technology::Bun);
    } else if root.join("pnpm-lock.yaml").exists() || package_manager.starts_with("pnpm@") {
        push_unique(&mut technologies, Technology::Pnpm);
    } else if root.join("yarn.lock").exists() || package_manager.starts_with("yarn@") {
        push_unique(&mut technologies, Technology::Yarn);
    } else if package.is_some() {
        push_unique(&mut technologies, Technology::Npm);
    }
    if root.join("deno.json").exists()
        || root.join("deno.jsonc").exists()
        || root.join("deno.lock").exists()
    {
        push_unique(&mut technologies, Technology::Deno);
    }
    if has_any(
        root,
        &[
            "Dockerfile",
            "compose.yaml",
            "compose.yml",
            "docker-compose.yaml",
            "docker-compose.yml",
        ],
    ) {
        push_unique(&mut technologies, Technology::Docker);
    }
    if has_any(root, &["Justfile", "justfile"]) {
        push_unique(&mut technologies, Technology::Just);
    }
    if package.is_some() {
        push_unique(&mut technologies, Technology::Nodejs);
    }

    for (dependency, technology) in [
        ("next", Technology::Nextjs),
        ("react", Technology::React),
        ("vue", Technology::Vue),
        ("svelte", Technology::Svelte),
        ("@angular/core", Technology::Angular),
        ("astro", Technology::Astro),
        ("vite", Technology::Vite),
        ("tailwindcss", Technology::Tailwind),
        ("@biomejs/biome", Technology::Biome),
        ("eslint", Technology::Eslint),
        ("prettier", Technology::Prettier),
        ("jest", Technology::Jest),
        ("vitest", Technology::Vitest),
        ("playwright", Technology::Playwright),
        ("prisma", Technology::Prisma),
        ("@prisma/client", Technology::Prisma),
        ("graphql", Technology::Graphql),
        ("firebase", Technology::Firebase),
        ("@supabase/supabase-js", Technology::Supabase),
        ("mongodb", Technology::Mongodb),
        ("mongoose", Technology::Mongodb),
        ("redis", Technology::Redis),
        ("ioredis", Technology::Redis),
        ("webpack", Technology::Webpack),
        ("rollup", Technology::Rollup),
    ] {
        if dependencies.iter().any(|name| name == dependency) {
            push_unique(&mut technologies, technology);
        }
    }
    if dependencies
        .iter()
        .any(|name| name.starts_with("@storybook/"))
        || root.join(".storybook").is_dir()
    {
        push_unique(&mut technologies, Technology::Storybook);
    }
    if root.join("vercel.json").exists() {
        push_unique(&mut technologies, Technology::Vercel);
    }
    if root.join(".github/workflows").is_dir() {
        push_unique(&mut technologies, Technology::GithubActions);
    }
    if root.join("nginx.conf").exists() || root.join("nginx").is_dir() {
        push_unique(&mut technologies, Technology::Nginx);
    }
    if root.join("main.tf").exists()
        || root.join("terraform.tf").exists()
        || root.join(".terraform.lock.hcl").exists()
    {
        push_unique(&mut technologies, Technology::Terraform);
    }
    if has_any(root, &["prisma/schema.prisma", "prisma.config.ts"]) {
        push_unique(&mut technologies, Technology::Prisma);
    }
    if has_any(
        root,
        &["vite.config.js", "vite.config.mjs", "vite.config.ts"],
    ) {
        push_unique(&mut technologies, Technology::Vite);
    }
    if has_any(
        root,
        &[
            "tailwind.config.js",
            "tailwind.config.cjs",
            "tailwind.config.ts",
        ],
    ) {
        push_unique(&mut technologies, Technology::Tailwind);
    }
    if root.join("composer.json").exists() {
        push_unique(&mut technologies, Technology::Composer);
    }
    if root.join("manage.py").exists() {
        push_unique(&mut technologies, Technology::Django);
    }
    let pyproject = read_text(&root.join("pyproject.toml")).unwrap_or_default();
    if pyproject.contains("fastapi") {
        push_unique(&mut technologies, Technology::Fastapi);
    }
    let cargo = read_text(&root.join("Cargo.toml"))
        .and_then(|contents| contents.parse::<toml::Value>().ok());
    if cargo
        .as_ref()
        .is_some_and(|manifest| toml_has_key(manifest, "dioxus"))
        || root.join("Dioxus.toml").exists()
    {
        push_unique(&mut technologies, Technology::Dioxus);
    }

    let service_text = read_first(
        root,
        &[
            "compose.yaml",
            "compose.yml",
            "docker-compose.yaml",
            "docker-compose.yml",
        ],
    )
    .unwrap_or_default()
    .to_ascii_lowercase();
    if service_text.contains("postgres") {
        push_unique(&mut technologies, Technology::Postgresql);
    }
    if service_text.contains("mongo") {
        push_unique(&mut technologies, Technology::Mongodb);
    }
    if service_text.contains("redis") {
        push_unique(&mut technologies, Technology::Redis);
    }
    if root.join("firebase.json").exists() {
        push_unique(&mut technologies, Technology::Firebase);
    }
    if root.join("supabase").is_dir() {
        push_unique(&mut technologies, Technology::Supabase);
    }
    if root.join("data.sqlite").exists()
        || root.join("db.sqlite3").exists()
        || dependencies.iter().any(|name| name.contains("sqlite"))
    {
        push_unique(&mut technologies, Technology::Sqlite);
    }

    technologies
}

fn package_dependencies(package: &JsonValue) -> Vec<String> {
    [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ]
    .into_iter()
    .filter_map(|key| package.get(key).and_then(JsonValue::as_object))
    .flat_map(|dependencies| dependencies.keys().cloned())
    .collect()
}

fn toml_has_key(value: &toml::Value, expected: &str) -> bool {
    value.as_table().is_some_and(|table| {
        table.contains_key(expected) || table.values().any(|value| toml_has_key(value, expected))
    })
}

fn push_unique(technologies: &mut Vec<Technology>, technology: Technology) {
    if !technologies.contains(&technology) {
        technologies.push(technology);
    }
}

fn has_any(root: &Path, paths: &[&str]) -> bool {
    paths.iter().any(|path| root.join(path).exists())
}

fn read_json(path: &Path) -> Option<JsonValue> {
    serde_json::from_str(&read_text(path)?).ok()
}

fn read_first(root: &Path, paths: &[&str]) -> Option<String> {
    paths.iter().find_map(|path| read_text(&root.join(path)))
}

fn read_text(path: &Path) -> Option<String> {
    let metadata = path.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_MANIFEST_BYTES {
        return None;
    }
    fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_languages_by_bytes_and_skips_vendored_directories() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("src")).unwrap();
        fs::create_dir_all(root.path().join("node_modules/pkg")).unwrap();
        fs::write(root.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.path().join("src/app.ts"), "export const app = true;\n").unwrap();
        fs::write(
            root.path().join("node_modules/pkg/index.js"),
            "x".repeat(10_000),
        )
        .unwrap();

        let profile = detect_workspace_profile(root.path());

        assert!(profile
            .languages
            .iter()
            .any(|language| language.name == "Rust"));
        assert!(profile
            .languages
            .iter()
            .any(|language| language.name == "TypeScript"));
        assert!(!profile
            .languages
            .iter()
            .any(|language| language.name == "JavaScript"));
    }

    #[test]
    fn detects_manifest_and_marker_technologies_in_display_order() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("package.json"),
            r#"{"packageManager":"bun@1.2.0","dependencies":{"react":"latest","vite":"latest"}}"#,
        )
        .unwrap();
        fs::write(root.path().join("Dockerfile"), "FROM scratch").unwrap();
        fs::write(root.path().join("justfile"), "build:\n  true\n").unwrap();

        let profile = detect_workspace_profile(root.path());

        assert_eq!(
            profile.technologies,
            vec![
                Technology::Bun,
                Technology::Docker,
                Technology::Just,
                Technology::Nodejs,
                Technology::React,
                Technology::Vite,
            ]
        );
    }
}
