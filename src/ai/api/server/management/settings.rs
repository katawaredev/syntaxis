use super::*;

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
    if !settings_schema_compatible(PI_SETTINGS_SCHEMA_VERSION, &version) {
        return Err(client_error(format!(
            "Settings editing was generated from Pi {PI_SETTINGS_SCHEMA_VERSION}; the server's Pi {version} has a different major version"
        )));
    }
    let definition = PI_SETTING_DEFINITIONS
        .iter()
        .find(|definition| definition.path == path)
        .ok_or_else(|| {
            client_error("This Pi setting is not available in the settings interface")
        })?;
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

async fn settings_snapshot(root: &Path) -> Result<PiSettingsSnapshot, ServerFnError> {
    let pi_version = pi_version(root).await?;
    let version_compatible = settings_schema_compatible(PI_SETTINGS_SCHEMA_VERSION, &pi_version);
    let manager_available = settings_manager_module().is_ok();
    let compatible = version_compatible && manager_available;
    let compatibility_message = if !version_compatible {
        Some(format!(
            "This settings interface was generated from Pi {PI_SETTINGS_SCHEMA_VERSION}; the server's Pi {pi_version} has a different major version. Update the application before editing settings."
        ))
    } else if !manager_available {
        Some("This Pi installation's internal settings API could not be located, so editing is disabled to avoid conflicting writes. Reading remains available.".into())
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

fn settings_schema_compatible(generated: &str, running: &str) -> bool {
    fn major_version(version: &str) -> Option<u64> {
        version.split('.').next()?.parse().ok()
    }

    major_version(generated)
        .zip(major_version(running))
        .is_some_and(|(generated, running)| generated == running)
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
    fn settings_schema_accepts_minor_and_patch_updates() {
        assert!(settings_schema_compatible("0.84.1", "0.84.2"));
        assert!(settings_schema_compatible("0.84.1", "0.85.0"));
        assert!(settings_schema_compatible("1.3.0", "1.4.0"));
    }

    #[test]
    fn settings_schema_rejects_different_or_invalid_major_versions() {
        assert!(!settings_schema_compatible("1.3.0", "2.0.0"));
        assert!(!settings_schema_compatible("0.84.1", "unknown"));
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
