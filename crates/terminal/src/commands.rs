use serde::{Deserialize, Serialize};

/// A project command presented by terminal run menus on every platform.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RunCommand {
    pub id: String,
    pub label: String,
    pub command: String,
    pub custom: bool,
}

/// Detect package scripts without depending on a host filesystem.
pub fn package_json_commands(contents: &str, sibling_names: &[String]) -> Vec<RunCommand> {
    let Ok(document) = serde_json::from_str::<serde_json::Value>(contents) else {
        return Vec::new();
    };
    let Some(scripts) = document.get("scripts").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    let manager = document
        .get("packageManager")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let has = |name: &str| sibling_names.iter().any(|candidate| candidate == name);
    let runner = if has("bun.lock") || has("bun.lockb") || manager.starts_with("bun@") {
        "bun run"
    } else if has("pnpm-lock.yaml") || manager.starts_with("pnpm@") {
        "pnpm run"
    } else if has("yarn.lock") || manager.starts_with("yarn@") {
        "yarn run"
    } else {
        "npm run"
    };
    let source = runner.split_whitespace().next().unwrap_or("package");
    scripts
        .keys()
        .map(|name| detected(source, name, format!("{runner} {name}")))
        .collect()
}

/// Detect public Just recipes from already-loaded manifest text.
pub fn justfile_commands(contents: &str) -> Vec<RunCommand> {
    contents
        .lines()
        .filter(|line| {
            !line.starts_with(char::is_whitespace) && !line.trim_start().starts_with('#')
        })
        .filter_map(|line| {
            let (header, suffix) = line.split_once(':')?;
            if suffix.starts_with('=') {
                return None;
            }
            let name = header.split_whitespace().next()?;
            let valid = !name.is_empty()
                && name
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "_-".contains(character));
            valid.then(|| detected("just", name, format!("just {name}")))
        })
        .collect()
}

/// Detect conventional Make targets from already-loaded manifest text.
pub fn makefile_commands(contents: &str) -> Vec<RunCommand> {
    contents
        .lines()
        .filter(|line| !line.starts_with(char::is_whitespace) && !line.starts_with('#'))
        .filter_map(|line| {
            let (name, suffix) = line.split_once(':')?;
            let valid = !name.is_empty()
                && !suffix.starts_with('=')
                && name
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "_.-".contains(character));
            valid.then(|| detected("make", name, format!("make {name}")))
        })
        .collect()
}

fn detected(source: &str, name: &str, command: String) -> RunCommand {
    RunCommand {
        id: format!("detected:{source}:{command}"),
        label: format!("{source} · {name}"),
        command,
        custom: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_scripts_honor_the_declared_manager() {
        let commands = package_json_commands(
            r#"{"packageManager":"pnpm@10","scripts":{"check":"biome check ."}}"#,
            &[],
        );

        assert_eq!(commands[0].command, "pnpm run check");
    }

    #[test]
    fn just_recipes_ignore_private_and_indented_lines() {
        let commands = justfile_commands("# comment\ncheck:\n  cargo check\n_value := 'x'\n");

        assert_eq!(commands.iter().map(|item| item.command.as_str()).collect::<Vec<_>>(), ["just check"]);
    }

    #[test]
    fn make_targets_ignore_assignments() {
        let commands = makefile_commands("build: src\nVALUE:=enabled\n\tcommand\n");

        assert_eq!(commands.iter().map(|item| item.command.as_str()).collect::<Vec<_>>(), ["make build"]);
    }
}
