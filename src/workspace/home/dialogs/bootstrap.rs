use std::collections::{BTreeSet, HashSet};

use dioxus::prelude::*;
use syntaxis_editor::{
    LanguageServerDefinition, language_server_by_id, language_servers_for_language,
    language_slug_for_path, profile_language_id,
};
use syntaxis_ui::prelude::{
    Button, ButtonKind, DialogActions, Field, Modal, TextArea, TextAreaResize,
};
use syntaxis_workspace::{EntryKind, FileEntry, RelativePath, WorkspaceProfile, WorkspaceRecord};

use crate::{
    terminal::ProjectInitializerTerminal,
    workspace::{api, client, home::HomeDialog},
};

const MISE_CHECK: &str = r"if ! command -v mise >/dev/null 2>&1; then
    echo 'mise is not installed in this runtime.' >&2
    exit 127
fi";

const CONFIGURED_BOOTSTRAP_COMMAND: &str = r"if ! command -v mise >/dev/null 2>&1; then
    echo 'mise is not installed in this runtime.' >&2
    exit 127
fi

echo 'Found a mise-compatible project configuration.'
mise trust --yes
mise install --yes
echo 'Project toolchain is ready.'";

#[derive(Clone, Debug, Eq, PartialEq)]
enum BootstrapPlan {
    Configured,
    Inferred(Vec<&'static str>),
}

#[component]
pub(super) fn BootstrapProjectDialog(
    index: usize,
    mut dialog: Signal<HomeDialog>,
    workspaces: Vec<WorkspaceRecord>,
    on_notice: EventHandler<String>,
    on_changed: EventHandler<()>,
) -> Element {
    let workspace = workspaces[index].clone();
    let workspace_for_plan = workspace.clone();
    let mut plan = use_resource(move || {
        let workspace = workspace_for_plan.clone();
        async move { detect_bootstrap_plan(workspace).await }
    });
    let mut selected_command = use_signal(|| None::<String>);
    let mut inferred_command = use_signal(String::new);
    let mut initialized_command = use_signal(|| false);
    let mut result = use_signal(|| None::<bool>);

    use_effect(move || {
        if !initialized_command()
            && let Some(Ok(BootstrapPlan::Inferred(tools))) = plan()
        {
            let mise_command = inferred_mise_command(&tools);
            if tools.is_empty() {
                inferred_command.set(mise_command);
            } else {
                selected_command.set(Some(inferred_bootstrap_command(&mise_command)));
            }
            initialized_command.set(true);
        }
    });

    let detected_plan = plan();
    let command = match detected_plan.as_ref() {
        Some(Ok(BootstrapPlan::Configured)) => Some(CONFIGURED_BOOTSTRAP_COMMAND.to_owned()),
        Some(Ok(BootstrapPlan::Inferred(_))) => selected_command(),
        Some(Err(_)) | None => None,
    };
    let running = command.is_some();
    let needs_manual_command = matches!(
        detected_plan.as_ref(),
        Some(Ok(BootstrapPlan::Inferred(tools))) if tools.is_empty()
    );
    rsx! {
        Modal {
            title: format!("Bootstrap {}", workspace.name),
            description: format!("Install the toolchain required by {} with mise.", workspace.root),
            content_class: if running { "max-w-180" } else { "max-w-2xl" },
            on_close: move |()| {
                if running && result().is_none() {
                    on_notice.call("Project bootstrap continues in its terminal session".into());
                }
                dialog.set(HomeDialog::None);
            },
            if let Some(command) = command {
                div { class: "px-5 pt-3 pb-5",
                    div { class: "h-[min(34rem,calc(100svh-13rem))] min-h-72 overflow-hidden rounded-lg border border-border bg-background",
                        ProjectInitializerTerminal {
                            workspace_id: workspace.id.0.clone(),
                            workspace_slug: workspace.slug.clone(),
                            command,
                            label: "Bootstrap with mise".to_owned(),
                            on_finished: {
                                let workspace_id = workspace.id.0.clone();
                                let workspace_name = workspace.name.clone();
                                move |success| {
                                    result.set(Some(success));
                                    let workspace_id = workspace_id.clone();
                                    let workspace_name = workspace_name.clone();
                                    spawn(async move {
                                        let _ = api::refresh_workspace(workspace_id).await;
                                        on_changed.call(());
                                        if success {
                                            on_notice.call(format!("{workspace_name} is ready"));
                                        }
                                    });
                                }
                            },
                        }
                    }
                    p { class: "mt-2.5 flex items-center gap-2 text-xs text-muted-foreground",
                        span { class: if result() == Some(true) { "size-2 rounded-full bg-success" } else if result() == Some(false) { "size-2 rounded-full bg-destructive" } else { "size-2 animate-pulse rounded-full bg-primary" } }
                        if result() == Some(true) {
                            "Bootstrap finished successfully."
                        } else if result() == Some(false) {
                            "Bootstrap exited with an error. Review the terminal output above."
                        } else {
                            "You can leave this dialog while installation continues."
                        }
                    }
                    div { class: "mt-4 flex justify-end",
                        Button {
                            label: "Close",
                            kind: ButtonKind::Primary,
                            onclick: move |_| dialog.set(HomeDialog::None),
                        }
                    }
                }
            } else if detected_plan.is_none() {
                div { class: "flex min-h-36 items-center justify-center gap-2 px-5 text-sm text-muted-foreground",
                    span { class: "size-4 animate-spin rounded-full border-2 border-border border-t-primary" }
                    "Inspecting the project…"
                }
            } else if let Some(Err(error)) = detected_plan.as_ref() {
                div { class: "space-y-4 px-5 pt-3 pb-5",
                    p {
                        class: "rounded-md border border-destructive/35 bg-destructive/10 px-3 py-2 text-sm text-destructive",
                        role: "alert",
                        "Could not inspect this project: {error}"
                    }
                    DialogActions {
                        Button {
                            label: "Cancel",
                            kind: ButtonKind::Ghost,
                            onclick: move |_| dialog.set(HomeDialog::None),
                        }
                        Button {
                            label: "Try again",
                            kind: ButtonKind::Primary,
                            onclick: move |_| plan.restart(),
                        }
                    }
                }
            } else if needs_manual_command {
                div { class: "space-y-4 px-5 pt-3 pb-5",
                    p { class: "text-sm leading-relaxed text-muted-foreground",
                        "No supported project markers were found. Enter the mise command you want to run."
                    }
                    Field {
                        control_id: "bootstrap-mise-command",
                        label: "Command to execute",
                        TextArea {
                            class: "min-h-24 font-mono text-xs",
                            rows: 4,
                            resize: TextAreaResize::None,
                            value: inferred_command(),
                            autofocus: true,
                            placeholder: "mise use --yes --env local node@lts",
                            oninput: move |event: FormEvent| inferred_command.set(event.value()),
                        }
                    }
                    p { class: "text-xs leading-relaxed text-muted-foreground",
                        "This creates a checkout-local mise.local.toml. For Git repositories, that file and its lockfile are also excluded from commits."
                    }
                    DialogActions {
                        Button {
                            label: "Cancel",
                            kind: ButtonKind::Ghost,
                            onclick: move |_| dialog.set(HomeDialog::None),
                        }
                        Button {
                            label: "Run command",
                            kind: ButtonKind::Primary,
                            disabled: inferred_command().trim().is_empty(),
                            onclick: move |_| {
                                selected_command.set(Some(inferred_bootstrap_command(&inferred_command())));
                            },
                        }
                    }
                }
            } else {
                div { class: "flex min-h-36 items-center justify-center gap-2 px-5 text-sm text-muted-foreground",
                    span { class: "size-4 animate-spin rounded-full border-2 border-border border-t-primary" }
                    "Starting bootstrap…"
                }
            }
        }
    }
}

async fn detect_bootstrap_plan(workspace: WorkspaceRecord) -> Result<BootstrapPlan, String> {
    let root = client::list_files(workspace.clone(), RelativePath::root()).await?;
    if has_root_mise_config(&root) || has_nested_mise_config(workspace.clone(), &root).await {
        return Ok(BootstrapPlan::Configured);
    }

    Ok(BootstrapPlan::Inferred(infer_tools(
        &root,
        &workspace.profile,
    )))
}

fn has_root_mise_config(entries: &[FileEntry]) -> bool {
    const CONFIGS: [&str; 5] = [
        "mise.toml",
        ".mise.toml",
        "mise.local.toml",
        ".mise.local.toml",
        ".tool-versions",
    ];
    entries
        .iter()
        .any(|entry| entry.kind == EntryKind::File && CONFIGS.contains(&entry.name.as_str()))
}

async fn has_nested_mise_config(workspace: WorkspaceRecord, root: &[FileEntry]) -> bool {
    let root_names = entry_names(root);
    for directory in ["mise", ".mise"] {
        if root_names.contains(directory)
            && list_contains_file(workspace.clone(), directory, "config.toml").await
        {
            return true;
        }
    }
    if !root_names.contains(".config") {
        return false;
    }

    let config = list_directory(workspace.clone(), ".config").await;
    if config
        .iter()
        .any(|entry| entry.kind == EntryKind::File && entry.name == "mise.toml")
    {
        return true;
    }
    if !entry_names(&config).contains("mise") {
        return false;
    }

    let mise = list_directory(workspace.clone(), ".config/mise").await;
    if mise
        .iter()
        .any(|entry| entry.kind == EntryKind::File && entry.name == "config.toml")
    {
        return true;
    }
    if !entry_names(&mise).contains("conf.d") {
        return false;
    }

    list_directory(workspace, ".config/mise/conf.d")
        .await
        .iter()
        .any(|entry| {
            entry.kind == EntryKind::File
                && std::path::Path::new(&entry.name)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
        })
}

async fn list_contains_file(workspace: WorkspaceRecord, path: &str, name: &str) -> bool {
    list_directory(workspace, path)
        .await
        .iter()
        .any(|entry| entry.kind == EntryKind::File && entry.name == name)
}

async fn list_directory(workspace: WorkspaceRecord, path: &str) -> Vec<FileEntry> {
    let Ok(path) = RelativePath::try_from(path) else {
        return Vec::new();
    };
    client::list_files(workspace, path)
        .await
        .unwrap_or_default()
}

fn entry_names(entries: &[FileEntry]) -> HashSet<&str> {
    entries.iter().map(|entry| entry.name.as_str()).collect()
}

fn infer_tools(entries: &[FileEntry], profile: &WorkspaceProfile) -> Vec<&'static str> {
    let files = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File)
        .map(|entry| entry.name.as_str())
        .collect::<BTreeSet<_>>();
    let has = |names: &[&str]| names.iter().any(|name| files.contains(name));
    let has_extension = |extension: &str| files.iter().any(|name| name.ends_with(extension));
    let mut tools = Vec::new();

    if has(&["Cargo.toml", "rust-toolchain", "rust-toolchain.toml"]) {
        tools.push("rust@stable");
    }
    if has(&["deno.json", "deno.jsonc", "deno.lock"]) {
        tools.push("deno@latest");
    } else if has(&["bun.lock", "bun.lockb"]) {
        tools.push("bun@latest");
    } else if has(&["package.json"]) {
        tools.push("node@lts");
        if has(&["pnpm-lock.yaml"]) {
            tools.push("pnpm@latest");
        } else if has(&["yarn.lock"]) {
            tools.push("yarn@latest");
        }
    }
    if has(&[
        "pyproject.toml",
        "requirements.txt",
        "setup.py",
        "setup.cfg",
        "Pipfile",
    ]) {
        tools.push("python@latest");
        if has(&["uv.lock"]) {
            tools.push("uv@latest");
        }
    }
    if has(&["go.mod", "go.work"]) {
        tools.push("go@latest");
    }
    if has(&["global.json"]) || has_extension(".csproj") || has_extension(".sln") {
        tools.push("dotnet@latest");
    }
    if has(&["pom.xml", "build.gradle", "build.gradle.kts", "gradlew"]) {
        tools.push("java@latest");
    }
    if has(&["Gemfile", ".ruby-version"]) {
        tools.push("ruby@latest");
    }
    if has(&["composer.json"]) {
        tools.extend(["php@latest", "composer@latest"]);
    }
    if has_extension(".tf") || has(&[".terraform.lock.hcl"]) {
        tools.push("terraform@latest");
    }
    if has(&["Justfile", "justfile"]) {
        tools.push("just@latest");
    }

    let mut language_ids = profile
        .languages
        .iter()
        .filter_map(|language| profile_language_id(&language.name))
        .collect::<Vec<_>>();
    language_ids.extend(
        files
            .iter()
            .map(|name| language_slug_for_path(name))
            .filter(|language| *language != "plaintext"),
    );
    language_ids.sort_unstable();
    language_ids.dedup();
    for language_id in language_ids {
        for server in language_servers_for_language(language_id, &profile.technologies) {
            push_server_tools(&mut tools, server);
        }
    }
    for server_id in profile
        .technologies
        .iter()
        .filter_map(|technology| match technology {
            syntaxis_workspace::WorkspaceTechnology::Astro => Some("astro"),
            syntaxis_workspace::WorkspaceTechnology::Svelte => Some("svelte"),
            syntaxis_workspace::WorkspaceTechnology::Tailwind => Some("tailwindcss"),
            syntaxis_workspace::WorkspaceTechnology::Vue => Some("vue"),
            _ => None,
        })
    {
        if let Some(server) = language_server_by_id(server_id) {
            push_server_tools(&mut tools, server);
        }
    }

    tools
}

fn push_server_tools(tools: &mut Vec<&'static str>, server: &'static LanguageServerDefinition) {
    for tool in server.mise_tools {
        if !tools.contains(tool) {
            tools.push(tool);
        }
    }
}

fn inferred_mise_command(tools: &[&str]) -> String {
    if tools.is_empty() {
        String::new()
    } else {
        format!("mise use --yes --env local {}", tools.join(" "))
    }
}

fn inferred_bootstrap_command(command: &str) -> String {
    format!(
        r#"{MISE_CHECK}

if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    exclude_path=$(git rev-parse --git-path info/exclude)
    for ignored in mise.local.toml mise.local.lock; do
        if ! grep -qxF "$ignored" "$exclude_path"; then
            printf '%s\n' "$ignored" >> "$exclude_path"
        fi
    done
fi
echo 'Writing mise.local.toml so the inferred setup stays local to this checkout.'
{command}
echo 'Project toolchain is ready.'"#,
        command = command.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::{
        CONFIGURED_BOOTSTRAP_COMMAND, infer_tools, inferred_bootstrap_command,
        inferred_mise_command,
    };
    use syntaxis_workspace::{
        EntryKind, FileEntry, RelativePath, WorkspaceLanguage, WorkspaceProfile,
        WorkspaceTechnology,
    };

    fn files(names: &[&str]) -> Vec<FileEntry> {
        names
            .iter()
            .map(|name| FileEntry {
                path: RelativePath::try_from(*name).unwrap(),
                name: (*name).to_owned(),
                kind: EntryKind::File,
                size: 0,
                version: None,
            })
            .collect()
    }

    #[test]
    fn configured_bootstrap_trusts_and_installs_project_config() {
        assert!(CONFIGURED_BOOTSTRAP_COMMAND.contains("mise trust --yes"));
        assert!(CONFIGURED_BOOTSTRAP_COMMAND.contains("mise install --yes"));
    }

    #[test]
    fn manifests_are_inferred_into_a_local_mise_command() {
        let profile = WorkspaceProfile {
            technologies: Vec::new(),
            languages: vec![
                WorkspaceLanguage {
                    name: "Rust".into(),
                    bytes: 100,
                },
                WorkspaceLanguage {
                    name: "TypeScript".into(),
                    bytes: 80,
                },
                WorkspaceLanguage {
                    name: "Python".into(),
                    bytes: 60,
                },
            ],
        };
        let tools = infer_tools(
            &files(&[
                "Cargo.toml",
                "package.json",
                "pnpm-lock.yaml",
                "pyproject.toml",
                "uv.lock",
            ]),
            &profile,
        );

        for expected in [
            "rust@stable",
            "node@lts",
            "pnpm@latest",
            "python@latest",
            "uv@latest",
            "rust-analyzer@latest",
            "npm:typescript@latest",
            "npm:pyright@latest",
            "taplo@latest",
            "npm:vscode-langservers-extracted@latest",
        ] {
            assert!(
                tools.contains(&expected),
                "missing inferred tool {expected}"
            );
        }
        assert!(inferred_mise_command(&tools).starts_with("mise use --yes --env local "));
    }

    #[test]
    fn deno_projects_use_the_builtin_server_instead_of_typescript_lsp() {
        let profile = WorkspaceProfile {
            technologies: vec![WorkspaceTechnology::Deno],
            languages: vec![WorkspaceLanguage {
                name: "TypeScript".into(),
                bytes: 100,
            }],
        };
        let tools = infer_tools(&files(&["deno.json"]), &profile);

        assert!(tools.contains(&"deno@latest"));
        assert!(!tools.contains(&"npm:typescript@latest"));
    }

    #[test]
    fn web_frameworks_install_their_language_servers() {
        let profile = WorkspaceProfile {
            technologies: vec![
                WorkspaceTechnology::Vue,
                WorkspaceTechnology::Svelte,
                WorkspaceTechnology::Astro,
                WorkspaceTechnology::Tailwind,
            ],
            languages: vec![WorkspaceLanguage {
                name: "TypeScript".into(),
                bytes: 100,
            }],
        };
        let tools = infer_tools(&files(&["package.json"]), &profile);

        for expected in [
            "npm:@vue/language-server@latest",
            "npm:svelte-language-server@latest",
            "npm:@astrojs/language-server@latest",
            "npm:@tailwindcss/language-server@latest",
        ] {
            assert!(
                tools.contains(&expected),
                "missing inferred tool {expected}"
            );
        }
    }

    #[test]
    fn edited_command_is_embedded_in_local_bootstrap_wrapper() {
        let command = inferred_bootstrap_command("mise use --env local node@22");
        assert!(command.contains("mise use --env local node@22"));
        assert!(command.contains("mise.local.toml mise.local.lock"));
    }
}
