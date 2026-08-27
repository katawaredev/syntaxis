use super::*;

const MAX_ADVANCED_SETTINGS_BYTES: usize = 256 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsState {
    values: Value,
    available_setters: Vec<String>,
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
    let definition = PI_SETTING_DEFINITIONS
        .iter()
        .find(|definition| definition.path == path)
        .ok_or_else(|| {
            client_error("This Pi setting is not available in the settings interface")
        })?;
    validate_setting_value(definition.kind, &value)?;
    let (node, sdk) = settings_manager_module()?;
    let script = r"import { pathToFileURL } from 'node:url';
const [sdkPath, cwd, agentDir, setter, encoded] = process.argv.slice(1);
const { SettingsManager } = await import(pathToFileURL(sdkPath).href);
const manager = SettingsManager.create(cwd, agentDir, { projectTrusted: false });
if (typeof manager[setter] !== 'function') throw new Error(`This Pi version does not support ${setter}`);
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
            .arg(sdk)
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

pub(crate) async fn pi_advanced_settings(
    workspace_id: WorkspaceId,
    scope: PiResourceScope,
) -> Result<PiAdvancedSettingsSnapshot, ServerFnError> {
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    advanced_settings_snapshot(Path::new(&workspace.root), scope).await
}

pub(crate) async fn save_pi_advanced_settings(
    workspace_id: WorkspaceId,
    scope: PiResourceScope,
    content: String,
    expected_revision: String,
) -> Result<PiAdvancedSettingsSnapshot, ServerFnError> {
    if content.len() > MAX_ADVANCED_SETTINGS_BYTES {
        return Err(client_error("Pi settings must be smaller than 256 KiB"));
    }
    let parsed: Value = serde_json::from_str(&content)
        .map_err(|error| client_error(format!("Invalid JSON: {error}")))?;
    if !parsed.is_object() {
        return Err(client_error("Pi settings must be a JSON object"));
    }
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let root = Path::new(&workspace.root);
    let path = settings_path(root, scope);
    let current = read_settings_content(&path)?;
    if content_revision(current.as_bytes()) != expected_revision {
        return Err(conflict_error(
            "Pi settings changed since they were opened. Reload before saving.",
        ));
    }
    let content = if content.ends_with('\n') {
        content
    } else {
        format!("{content}\n")
    };
    write_settings_atomically(&path, content.as_bytes())?;
    advanced_settings_snapshot(root, scope).await
}

async fn settings_snapshot(root: &Path) -> Result<PiSettingsSnapshot, ServerFnError> {
    let pi_version = pi_version(root).await?;
    let state = settings_state(root).await?;
    Ok(PiSettingsSnapshot {
        pi_version,
        available_setters: state.available_setters,
        values: state.values,
    })
}

async fn settings_state(root: &Path) -> Result<SettingsState, ServerFnError> {
    let (node, sdk) = settings_manager_module()?;
    let setters = PI_SETTING_DEFINITIONS
        .iter()
        .map(|definition| definition.setter)
        .collect::<BTreeSet<_>>();
    let script = r"import { pathToFileURL } from 'node:url';
const [sdkPath, cwd, agentDir, settersJson] = process.argv.slice(1);
const { SettingsManager } = await import(pathToFileURL(sdkPath).href);
const manager = SettingsManager.create(cwd, agentDir, { projectTrusted: false });
const errors = manager.drainErrors();
if (errors.length) throw errors[0].error;
const availableSetters = JSON.parse(settersJson).filter((setter) => typeof manager[setter] === 'function');
process.stdout.write(JSON.stringify({ values: manager.getGlobalSettings(), availableSetters }));";
    let output =
        tokio::time::timeout(
            COMMAND_TIMEOUT,
            tokio::process::Command::new(node)
                .args(["--input-type=module", "--eval", script])
                .arg(sdk)
                .arg(root)
                .arg(agent_dir(root))
                .arg(serde_json::to_string(&setters).map_err(|error| {
                    server_error(format!("Could not prepare Pi settings: {error}"))
                })?)
                .env("NO_COLOR", "1")
                .stdin(Stdio::null())
                .output(),
        )
        .await
        .map_err(|_| server_error("Loading Pi settings timed out"))?
        .map_err(|error| server_error(format!("Could not start Pi's settings manager: {error}")))?;
    if !output.status.success() {
        return Err(server_error(command_failure(&output)));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| server_error(format!("Pi returned invalid settings data: {error}")))
}

async fn advanced_settings_snapshot(
    root: &Path,
    scope: PiResourceScope,
) -> Result<PiAdvancedSettingsSnapshot, ServerFnError> {
    let path = settings_path(root, scope);
    let content = read_settings_content(&path)?;
    Ok(PiAdvancedSettingsSnapshot {
        scope,
        path: path.display().to_string(),
        revision: content_revision(content.as_bytes()),
        content,
        documentation: settings_documentation(root).await?,
    })
}

async fn settings_documentation(root: &Path) -> Result<String, ServerFnError> {
    let (node, sdk) = settings_manager_module()?;
    let script = r"import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
const [sdkPath] = process.argv.slice(1);
const { getDocsPath } = await import(pathToFileURL(sdkPath).href);
const documentation = await readFile(join(getDocsPath(), 'settings.md'), 'utf8');
process.stdout.write(documentation.slice(0, 256 * 1024));";
    let output = tokio::time::timeout(
        COMMAND_TIMEOUT,
        tokio::process::Command::new(node)
            .args(["--input-type=module", "--eval", script])
            .arg(sdk)
            .current_dir(root)
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .output(),
    )
    .await
    .map_err(|_| server_error("Loading Pi settings documentation timed out"))?
    .map_err(|error| server_error(format!("Could not start Pi: {error}")))?;
    if !output.status.success() {
        return Err(server_error(command_failure(&output)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn settings_path(root: &Path, scope: PiResourceScope) -> PathBuf {
    match scope {
        PiResourceScope::Global => agent_dir(root).join("settings.json"),
        PiResourceScope::Project => root.join(".pi/settings.json"),
    }
}

fn read_settings_content(path: &Path) -> Result<String, ServerFnError> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.len() > MAX_ADVANCED_SETTINGS_BYTES as u64 => {
            return Err(client_error("Pi settings must be smaller than 256 KiB"));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok("{}\n".into()),
        Err(error) => {
            return Err(server_error(format!(
                "Could not inspect {}: {error}",
                path.display()
            )));
        }
    }
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) => Err(server_error(format!(
            "Could not read {}: {error}",
            path.display()
        ))),
    }
}

fn content_revision(content: &[u8]) -> String {
    let hash = content
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    format!("{hash:016x}")
}

fn write_settings_atomically(path: &Path, content: &[u8]) -> Result<(), ServerFnError> {
    use std::io::Write;

    let parent = path
        .parent()
        .ok_or_else(|| server_error("Pi settings path has no parent directory"))?;
    fs::create_dir_all(parent)
        .map_err(|error| server_error(format!("Could not create {}: {error}", parent.display())))?;
    let backup = path.with_file_name("settings.json.syntaxis-backup");
    if path.is_file() {
        fs::copy(path, &backup).map_err(|error| {
            server_error(format!("Could not back up {}: {error}", path.display()))
        })?;
    }
    let temp = parent.join(format!(
        ".settings.json.syntaxis-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let result = (|| -> std::io::Result<()> {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(content)?;
        file.sync_all()?;
        if let Ok(metadata) = fs::metadata(path) {
            fs::set_permissions(&temp, metadata.permissions())?;
        }
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        return Err(server_error(format!(
            "Could not save {}: {error}",
            path.display()
        )));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_revisions_are_stable_and_sensitive_to_changes() {
        assert_eq!(content_revision(b"{}\n"), content_revision(b"{}\n"));
        assert_ne!(content_revision(b"{}\n"), content_revision(b"{ }\n"));
    }

    #[test]
    fn generated_setting_values_are_type_checked() {
        validate_setting_value(PiSettingKind::Toggle, &json!(true))
            .expect("boolean toggle values must be accepted");
        validate_setting_value(PiSettingKind::Toggle, &json!("true"))
            .expect_err("string toggle values must be rejected");
        validate_setting_value(PiSettingKind::Select(&["auto", "sse"]), &json!("auto"))
            .expect("known select values must be accepted");
        validate_setting_value(PiSettingKind::Select(&["auto", "sse"]), &json!("other"))
            .expect_err("unknown select values must be rejected");
        validate_setting_value(PiSettingKind::StringArray, &json!(["mise", "npm"]))
            .expect("string arrays must be accepted");
        validate_setting_value(PiSettingKind::StringArray, &json!("npm"))
            .expect_err("scalar strings must be rejected for string arrays");
    }
}
