use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock},
};

use dioxus::prelude::ServerFnError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use syntaxis_workspace::{WorkspaceId, WorkspaceRecord};

use super::RunCommand;

const MAX_COMMANDS: usize = 200;
const MAX_CONFIG_BYTES: u64 = 2 * 1024 * 1024;
const MAX_LABEL_BYTES: usize = 120;
const MAX_COMMAND_BYTES: usize = 4 * 1024;

static COMMAND_STORE: OnceLock<Result<Mutex<CommandStore>, String>> = OnceLock::new();

#[derive(Default, Deserialize, Serialize)]
struct CommandStoreFile {
    workspaces: HashMap<String, WorkspaceCommands>,
}

#[derive(Clone, Deserialize, Serialize)]
struct WorkspaceCommands {
    commands: Vec<RunCommand>,
    #[serde(default = "first_custom_id")]
    next_custom_id: u64,
}

impl Default for WorkspaceCommands {
    fn default() -> Self {
        Self {
            commands: Vec::new(),
            next_custom_id: first_custom_id(),
        }
    }
}

const fn first_custom_id() -> u64 {
    1
}

struct CommandStore {
    path: PathBuf,
    file: CommandStoreFile,
}

impl CommandStore {
    fn open(path: PathBuf) -> Result<Self, String> {
        let file = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| format!("Could not read saved terminal commands: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                CommandStoreFile::default()
            }
            Err(error) => return Err(format!("Could not read saved terminal commands: {error}")),
        };
        Ok(Self { path, file })
    }

    fn save(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&self.file)
            .map_err(|error| format!("Could not encode terminal commands: {error}"))?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, bytes)
            .and_then(|()| fs::rename(&temporary, &self.path))
            .map_err(|error| format!("Could not save terminal commands: {error}"))
    }
}

pub(super) async fn list(workspace_id: WorkspaceId) -> Result<Vec<RunCommand>, ServerFnError> {
    let workspace = workspace(&workspace_id).await?;
    let mut store = store()?;
    if !store.file.workspaces.contains_key(&workspace_id.0) {
        let commands = discover(Path::new(&workspace.root));
        store.file.workspaces.insert(
            workspace_id.0.clone(),
            WorkspaceCommands {
                commands,
                next_custom_id: 1,
            },
        );
        store.save().map_err(internal_error)?;
    }
    Ok(store.file.workspaces[&workspace_id.0].commands.clone())
}

pub(super) async fn refresh(workspace_id: WorkspaceId) -> Result<Vec<RunCommand>, ServerFnError> {
    let workspace = workspace(&workspace_id).await?;
    let detected = discover(Path::new(&workspace.root));
    let mut store = store()?;
    let catalog = store
        .file
        .workspaces
        .entry(workspace_id.0)
        .or_insert_with(WorkspaceCommands::default);
    replace_detected_commands(catalog, detected);
    let commands = catalog.commands.clone();
    store.save().map_err(internal_error)?;
    Ok(commands)
}

pub(super) async fn add(
    workspace_id: WorkspaceId,
    label: String,
    command: String,
) -> Result<Vec<RunCommand>, ServerFnError> {
    let _workspace = workspace(&workspace_id).await?;
    let label = label.trim();
    let command = command.trim();
    validate_custom_command(label, command)?;

    let mut store = store()?;
    let catalog = store
        .file
        .workspaces
        .entry(workspace_id.0)
        .or_insert_with(WorkspaceCommands::default);
    if catalog
        .commands
        .iter()
        .any(|existing| existing.command == command)
    {
        return Err(request_error(
            "That command is already in the project command list.",
            409,
        ));
    }
    let id = format!("custom:{}", catalog.next_custom_id);
    catalog.next_custom_id = catalog.next_custom_id.saturating_add(1).max(1);
    catalog.commands.push(RunCommand {
        id,
        label: if label.is_empty() {
            command.to_owned()
        } else {
            label.to_owned()
        },
        command: command.to_owned(),
        custom: true,
    });
    let commands = catalog.commands.clone();
    store.save().map_err(internal_error)?;
    Ok(commands)
}

pub(super) async fn delete(
    workspace_id: WorkspaceId,
    command_id: String,
) -> Result<Vec<RunCommand>, ServerFnError> {
    let _workspace = workspace(&workspace_id).await?;
    let mut store = store()?;
    let catalog = store
        .file
        .workspaces
        .get_mut(&workspace_id.0)
        .ok_or_else(|| request_error("The project command list was not found.", 404))?;
    let index = catalog
        .commands
        .iter()
        .position(|command| command.id == command_id)
        .ok_or_else(|| request_error("That command was not found.", 404))?;
    if !catalog.commands[index].custom {
        return Err(request_error(
            "Detected commands can be removed by changing the project file and refreshing.",
            400,
        ));
    }
    catalog.commands.remove(index);
    let commands = catalog.commands.clone();
    store.save().map_err(internal_error)?;
    Ok(commands)
}

async fn workspace(workspace_id: &WorkspaceId) -> Result<WorkspaceRecord, ServerFnError> {
    crate::workspace::api::server::workspace_by_id(workspace_id).await
}

fn store() -> Result<MutexGuard<'static, CommandStore>, ServerFnError> {
    COMMAND_STORE
        .get_or_init(|| {
            CommandStore::open(
                crate::workspace::api::server::data_directory().join("terminal-commands.json"),
            )
            .map(Mutex::new)
        })
        .as_ref()
        .map_err(|error| internal_error(error.clone()))?
        .lock()
        .map_err(|_| internal_error("The terminal command store is unavailable."))
}

fn validate_custom_command(label: &str, command: &str) -> Result<(), ServerFnError> {
    if command.is_empty() {
        return Err(request_error("Enter a command to run.", 400));
    }
    if command.contains(['\n', '\r']) || label.contains(['\n', '\r']) {
        return Err(request_error("Commands and labels must use one line.", 400));
    }
    if command.len() > MAX_COMMAND_BYTES {
        return Err(request_error("The command is too long.", 400));
    }
    if label.len() > MAX_LABEL_BYTES {
        return Err(request_error("The command label is too long.", 400));
    }
    Ok(())
}

fn request_error(message: impl Into<String>, code: u16) -> ServerFnError {
    ServerFnError::ServerError {
        message: message.into(),
        code,
        details: None,
    }
}

fn internal_error(message: impl Into<String>) -> ServerFnError {
    request_error(message, 500)
}

fn discover(root: &Path) -> Vec<RunCommand> {
    let mut commands = Vec::new();
    let mut seen = HashSet::new();

    discover_just(root, &mut commands, &mut seen);
    discover_package_json(root, &mut commands, &mut seen);
    discover_make(root, &mut commands, &mut seen);
    discover_taskfile(root, &mut commands, &mut seen);
    discover_toml_tasks(root, &mut commands, &mut seen);
    discover_json_tasks(root, &mut commands, &mut seen);
    discover_rake(root, &mut commands, &mut seen);
    discover_gradle(root, &mut commands, &mut seen);
    discover_common_projects(root, &mut commands, &mut seen);

    commands
}

fn discover_just(root: &Path, commands: &mut Vec<RunCommand>, seen: &mut HashSet<String>) {
    let Some(contents) = read_first(root, &["Justfile", "justfile", ".justfile"]) else {
        return;
    };
    for line in contents.lines().filter(|line| {
        !line.starts_with(char::is_whitespace) && !line.trim_start().starts_with('#')
    }) {
        let Some((header, suffix)) = line.split_once(':') else {
            continue;
        };
        if suffix.starts_with('=') {
            continue;
        }
        let Some(name) = header.split_whitespace().next() else {
            continue;
        };
        if is_just_recipe_name(name) {
            add_detected(commands, seen, "just", name, format!("just {name}"));
        }
    }
}

fn discover_package_json(root: &Path, commands: &mut Vec<RunCommand>, seen: &mut HashSet<String>) {
    let Some(contents) = read_file(&root.join("package.json")) else {
        return;
    };
    let Ok(package) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return;
    };
    let Some(scripts) = package
        .get("scripts")
        .and_then(serde_json::Value::as_object)
    else {
        return;
    };
    let declared_manager = package
        .get("packageManager")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let runner = if root.join("bun.lock").exists()
        || root.join("bun.lockb").exists()
        || declared_manager.starts_with("bun@")
    {
        "bun run"
    } else if root.join("pnpm-lock.yaml").exists() || declared_manager.starts_with("pnpm@") {
        "pnpm run"
    } else if root.join("yarn.lock").exists() || declared_manager.starts_with("yarn@") {
        "yarn run"
    } else {
        "npm run"
    };
    let source = runner.split_whitespace().next().unwrap_or("package");
    for name in scripts.keys() {
        add_detected(commands, seen, source, name, format!("{runner} {name}"));
    }
}

fn discover_make(root: &Path, commands: &mut Vec<RunCommand>, seen: &mut HashSet<String>) {
    let Some(contents) = read_first(root, &["GNUmakefile", "Makefile", "makefile"]) else {
        return;
    };
    let target = Regex::new(r"(?m)^([A-Za-z0-9][A-Za-z0-9_.-]*):(?:[^=]|$)")
        .expect("valid make target regex");
    for capture in target.captures_iter(&contents) {
        let name = &capture[1];
        add_detected(commands, seen, "make", name, format!("make {name}"));
    }
}

fn discover_taskfile(root: &Path, commands: &mut Vec<RunCommand>, seen: &mut HashSet<String>) {
    let Some(contents) = read_first(
        root,
        &[
            "Taskfile.yml",
            "Taskfile.yaml",
            "taskfile.yml",
            "taskfile.yaml",
        ],
    ) else {
        return;
    };
    let mut in_tasks = false;
    for line in contents.lines() {
        if line.trim() == "tasks:" && !line.starts_with(char::is_whitespace) {
            in_tasks = true;
            continue;
        }
        if !in_tasks {
            continue;
        }
        if !line.is_empty() && !line.starts_with(char::is_whitespace) {
            break;
        }
        let indentation = line.len().saturating_sub(line.trim_start().len());
        if indentation != 2 {
            continue;
        }
        let Some((name, _)) = line.trim().split_once(':') else {
            continue;
        };
        if is_task_name(name) {
            add_detected(commands, seen, "task", name, format!("task {name}"));
        }
    }
}

fn discover_toml_tasks(root: &Path, commands: &mut Vec<RunCommand>, seen: &mut HashSet<String>) {
    if let Some(contents) = read_file(&root.join("mise.toml")) {
        add_toml_table_tasks(&contents, &["tasks"], "mise", "mise run", commands, seen);
    }
    if let Some(contents) = read_file(&root.join("pixi.toml")) {
        add_toml_table_tasks(&contents, &["tasks"], "pixi", "pixi run", commands, seen);
    }
    if let Some(contents) = read_file(&root.join("pyproject.toml")) {
        add_toml_table_tasks(
            &contents,
            &["tool", "poe", "tasks"],
            "poe",
            "poe",
            commands,
            seen,
        );
    }
}

fn add_toml_table_tasks(
    contents: &str,
    path: &[&str],
    source: &str,
    runner: &str,
    commands: &mut Vec<RunCommand>,
    seen: &mut HashSet<String>,
) {
    let Ok(value) = contents.parse::<toml::Value>() else {
        return;
    };
    let mut current = &value;
    for segment in path {
        let Some(next) = current.get(*segment) else {
            return;
        };
        current = next;
    }
    let Some(tasks) = current.as_table() else {
        return;
    };
    for name in tasks.keys().filter(|name| is_task_name(name)) {
        add_detected(commands, seen, source, name, format!("{runner} {name}"));
    }
}

fn discover_json_tasks(root: &Path, commands: &mut Vec<RunCommand>, seen: &mut HashSet<String>) {
    if let Some(contents) =
        read_file(&root.join("deno.json")).or_else(|| read_file(&root.join("deno.jsonc")))
    {
        if let Ok(config) = serde_json::from_str::<serde_json::Value>(&contents) {
            if let Some(tasks) = config.get("tasks").and_then(serde_json::Value::as_object) {
                for name in tasks.keys() {
                    add_detected(commands, seen, "deno", name, format!("deno task {name}"));
                }
            }
        }
    }
    if let Some(contents) = read_file(&root.join("composer.json")) {
        if let Ok(config) = serde_json::from_str::<serde_json::Value>(&contents) {
            if let Some(scripts) = config.get("scripts").and_then(serde_json::Value::as_object) {
                for name in scripts.keys() {
                    add_detected(
                        commands,
                        seen,
                        "composer",
                        name,
                        format!("composer run-script {name}"),
                    );
                }
            }
        }
    }
}

fn discover_rake(root: &Path, commands: &mut Vec<RunCommand>, seen: &mut HashSet<String>) {
    let Some(contents) = read_first(root, &["Rakefile", "rakefile", "Rakefile.rb"]) else {
        return;
    };
    let task = Regex::new(r#"(?m)^\s*task\s+(?::([A-Za-z0-9_:.-]+)|["']([A-Za-z0-9_:.-]+)["'])"#)
        .expect("valid rake task regex");
    for capture in task.captures_iter(&contents) {
        if let Some(name) = capture.get(1).or_else(|| capture.get(2)) {
            add_detected(
                commands,
                seen,
                "rake",
                name.as_str(),
                format!("bundle exec rake {}", name.as_str()),
            );
        }
    }
}

fn discover_gradle(root: &Path, commands: &mut Vec<RunCommand>, seen: &mut HashSet<String>) {
    let Some(contents) = read_first(root, &["build.gradle.kts", "build.gradle"]) else {
        return;
    };
    let runner = if root.join("gradlew").exists() {
        "./gradlew"
    } else {
        "gradle"
    };
    let task = Regex::new(
        r#"(?m)^\s*(?:tasks?\.(?:register|create)\(["']([^"']+)["']|task\s+([A-Za-z][A-Za-z0-9_-]*))"#,
    )
    .expect("valid gradle task regex");
    for capture in task.captures_iter(&contents) {
        if let Some(name) = capture.get(1).or_else(|| capture.get(2)) {
            add_detected(
                commands,
                seen,
                "gradle",
                name.as_str(),
                format!("{runner} {}", name.as_str()),
            );
        }
    }
    for name in ["build", "test"] {
        add_detected(commands, seen, "gradle", name, format!("{runner} {name}"));
    }
}

fn discover_common_projects(
    root: &Path,
    commands: &mut Vec<RunCommand>,
    seen: &mut HashSet<String>,
) {
    if root.join("Cargo.toml").exists() {
        for (label, command) in [
            ("check", "cargo check --workspace"),
            ("test", "cargo test --workspace"),
            ("clippy", "cargo clippy --workspace --all-targets"),
            ("build", "cargo build --workspace"),
        ] {
            add_detected(commands, seen, "cargo", label, command.to_owned());
        }
    }
    if root.join("go.mod").exists() {
        add_detected(commands, seen, "go", "test", "go test ./...".into());
        add_detected(commands, seen, "go", "build", "go build ./...".into());
    }
    if root.join("manage.py").exists() {
        add_detected(
            commands,
            seen,
            "django",
            "runserver",
            "python manage.py runserver".into(),
        );
        add_detected(
            commands,
            seen,
            "django",
            "test",
            "python manage.py test".into(),
        );
    }
    if root.join("pom.xml").exists() {
        let runner = if root.join("mvnw").exists() {
            "./mvnw"
        } else {
            "mvn"
        };
        add_detected(commands, seen, "maven", "test", format!("{runner} test"));
        add_detected(
            commands,
            seen,
            "maven",
            "package",
            format!("{runner} package"),
        );
    }
    if root.join("mix.exs").exists() {
        add_detected(commands, seen, "mix", "test", "mix test".into());
        if root.join("lib").join("mix").exists() || root.join("assets").exists() {
            add_detected(commands, seen, "mix", "server", "mix phx.server".into());
        }
    }
    if root.join("compose.yaml").exists()
        || root.join("compose.yml").exists()
        || root.join("docker-compose.yaml").exists()
        || root.join("docker-compose.yml").exists()
    {
        add_detected(commands, seen, "compose", "up", "docker compose up".into());
        add_detected(
            commands,
            seen,
            "compose",
            "build",
            "docker compose build".into(),
        );
        add_detected(
            commands,
            seen,
            "compose",
            "down",
            "docker compose down".into(),
        );
    }
}

fn add_detected(
    commands: &mut Vec<RunCommand>,
    seen: &mut HashSet<String>,
    source: &str,
    name: &str,
    command: String,
) {
    if commands.len() >= MAX_COMMANDS || !seen.insert(command.clone()) {
        return;
    }
    commands.push(RunCommand {
        id: format!("detected:{source}:{command}"),
        label: format!("{source} · {name}"),
        command,
        custom: false,
    });
}

fn replace_detected_commands(catalog: &mut WorkspaceCommands, detected: Vec<RunCommand>) {
    let custom = catalog
        .commands
        .iter()
        .filter(|command| command.custom)
        .cloned()
        .collect::<Vec<_>>();
    catalog.commands = detected;
    catalog.commands.extend(custom);
    if catalog.next_custom_id == 0 {
        catalog.next_custom_id = first_custom_id();
    }
}

fn read_first(root: &Path, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| read_file(&root.join(name)))
}

fn read_file(path: &Path) -> Option<String> {
    let metadata = path.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn is_task_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_.:-".contains(character))
}

fn is_just_recipe_name(name: &str) -> bool {
    name.starts_with(|character: char| character.is_ascii_alphabetic())
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-".contains(character))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_project_task_runners_and_uses_the_package_lockfile() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("package.json"),
            r#"{"scripts":{"dev":"vite","test":"vitest"}}"#,
        )
        .unwrap();
        fs::write(
            directory.path().join("pnpm-lock.yaml"),
            "lockfileVersion: 9",
        )
        .unwrap();
        fs::write(
            directory.path().join("Justfile"),
            "check: format lint\n    cargo check\n",
        )
        .unwrap();
        fs::write(
            directory.path().join("Makefile"),
            "release:\n\tcargo build --release\n",
        )
        .unwrap();

        let commands = discover(directory.path());
        let command_text = commands
            .iter()
            .map(|command| command.command.as_str())
            .collect::<Vec<_>>();

        assert!(command_text.contains(&"just check"));
        assert!(command_text.contains(&"pnpm run dev"));
        assert!(command_text.contains(&"pnpm run test"));
        assert!(command_text.contains(&"make release"));
    }

    #[test]
    fn refresh_keeps_custom_commands_separate_from_detected_commands() {
        let detected = RunCommand {
            id: "detected:just:test".into(),
            label: "just · test".into(),
            command: "just test".into(),
            custom: false,
        };
        let custom = RunCommand {
            id: "custom:1".into(),
            label: "Preview".into(),
            command: "preview --open".into(),
            custom: true,
        };
        let catalog = WorkspaceCommands {
            commands: vec![detected, custom.clone()],
            next_custom_id: 2,
        };

        let replacement = RunCommand {
            id: "detected:make:test".into(),
            label: "make · test".into(),
            command: "make test".into(),
            custom: false,
        };
        let mut catalog = catalog;
        replace_detected_commands(&mut catalog, vec![replacement.clone()]);

        assert_eq!(catalog.commands, vec![replacement, custom]);
    }

    #[test]
    fn command_store_survives_reopening() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("terminal-commands.json");
        let mut store = CommandStore::open(path.clone()).unwrap();
        store.file.workspaces.insert(
            "project".into(),
            WorkspaceCommands {
                commands: vec![RunCommand {
                    id: "custom:1".into(),
                    label: "Serve".into(),
                    command: "serve".into(),
                    custom: true,
                }],
                next_custom_id: 2,
            },
        );
        store.save().unwrap();

        let reopened = CommandStore::open(path).unwrap();
        assert_eq!(
            reopened.file.workspaces["project"].commands[0].command,
            "serve"
        );
    }
}
