use super::*;

const GLOBAL_INSTRUCTIONS_FILE: &str = "AGENTS.md";

pub(crate) async fn pi_global_instructions(
    workspace_id: WorkspaceId,
) -> Result<String, ServerFnError> {
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    read_instructions(&agent_dir(Path::new(&workspace.root)).join(GLOBAL_INSTRUCTIONS_FILE))
}

pub(crate) async fn save_pi_global_instructions(
    workspace_id: WorkspaceId,
    content: String,
) -> Result<String, ServerFnError> {
    validate_resource_text(&content, MAX_RESOURCE_BYTES, "global instructions")?;
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let path = agent_dir(Path::new(&workspace.root)).join(GLOBAL_INSTRUCTIONS_FILE);
    save_instructions(&path, &content)
}

fn read_instructions(path: &Path) -> Result<String, ServerFnError> {
    reject_symlink(path)?;
    match fs::read_to_string(path) {
        Ok(content) => {
            validate_resource_text(&content, MAX_RESOURCE_BYTES, "global instructions")?;
            Ok(content)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(server_error(format!(
            "Could not read {}: {error}",
            path.display()
        ))),
    }
}

fn save_instructions(path: &Path, content: &str) -> Result<String, ServerFnError> {
    reject_symlink(path)?;
    if content.trim().is_empty() {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(server_error(format!(
                    "Could not remove {}: {error}",
                    path.display()
                )));
            }
        }
        return Ok(String::new());
    }

    let Some(directory) = path.parent() else {
        return Err(server_error("Invalid global instructions path"));
    };
    fs::create_dir_all(directory).map_err(|error| {
        server_error(format!("Could not create {}: {error}", directory.display()))
    })?;
    let normalized = format!("{}\n", content.trim_end_matches(['\r', '\n']));
    write_atomic(path, normalized.as_bytes())?;
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_instructions_round_trip_and_empty_save_removes_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(GLOBAL_INSTRUCTIONS_FILE);

        assert_eq!(read_instructions(&path).unwrap(), "");
        assert_eq!(
            save_instructions(&path, "Keep commands light").unwrap(),
            "Keep commands light\n"
        );
        assert_eq!(read_instructions(&path).unwrap(), "Keep commands light\n");
        assert_eq!(save_instructions(&path, " \n").unwrap(), "");
        assert!(!path.exists());
    }
}
