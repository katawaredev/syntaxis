use super::RunCommand;
use regex::Regex;
use std::{collections::HashSet, fs, path::Path};
use syntaxis_terminal::{justfile_commands, makefile_commands, package_json_commands};
const MAX_COMMANDS: usize = 200;
const MAX_CONFIG_BYTES: u64 = 2 * 1024 * 1024;
pub(super) fn discover(root: &Path) -> Vec<RunCommand> {
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
    add_shared(commands, seen, justfile_commands(&contents));
}
fn discover_package_json(root: &Path, commands: &mut Vec<RunCommand>, seen: &mut HashSet<String>) {
    let Some(contents) = read_file(&root.join("package.json")) else {
        return;
    };
    let sibling_names = ["bun.lock", "bun.lockb", "pnpm-lock.yaml", "yarn.lock"]
        .into_iter()
        .filter(|name| root.join(name).exists())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    add_shared(
        commands,
        seen,
        package_json_commands(&contents, &sibling_names),
    );
}
fn discover_make(root: &Path, commands: &mut Vec<RunCommand>, seen: &mut HashSet<String>) {
    let Some(contents) = read_first(root, &["GNUmakefile", "Makefile", "makefile"]) else {
        return;
    };
    add_shared(commands, seen, makefile_commands(&contents));
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
        && let Ok(config) = serde_json::from_str::<serde_json::Value>(&contents)
        && let Some(tasks) = config.get("tasks").and_then(serde_json::Value::as_object)
    {
        for name in tasks.keys() {
            add_detected(commands, seen, "deno", name, format!("deno task {name}"));
        }
    }
    if let Some(contents) = read_file(&root.join("composer.json"))
        && let Ok(config) = serde_json::from_str::<serde_json::Value>(&contents)
        && let Some(scripts) = config.get("scripts").and_then(serde_json::Value::as_object)
    {
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
fn add_shared(
    commands: &mut Vec<RunCommand>,
    seen: &mut HashSet<String>,
    detected: impl IntoIterator<Item = RunCommand>,
) {
    for command in detected {
        if commands.len() >= MAX_COMMANDS {
            return;
        }
        if seen.insert(command.command.clone()) {
            commands.push(command);
        }
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
