use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock},
};

use dioxus::prelude::ServerFnError;
use serde::{Deserialize, Serialize};
use syntaxis_workspace::{WorkspaceId, WorkspaceRecord};

use super::RunCommand;

mod discovery;

use discovery::discover;

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
