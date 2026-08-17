use super::*;

pub(super) const COMMAND_TIMEOUT: Duration = Duration::from_mins(3);
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

pub(super) fn http_client() -> Result<reqwest::Client, ServerFnError> {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent("syntaxis-pi-package-browser/0.1")
        .build()
        .map_err(|error| server_error(format!("Could not initialize the package browser: {error}")))
}

pub(super) async fn pi_version(root: &Path) -> Result<String, ServerFnError> {
    let output = run_pi(&root.to_string_lossy(), &["--version"], true).await?;
    Ok(output.trim().trim_start_matches('v').to_owned())
}

pub(super) async fn run_pi(
    root: &str,
    arguments: &[&str],
    skip_version_check: bool,
) -> Result<String, ServerFnError> {
    let mut command = tokio::process::Command::new(pi_command());
    command
        .args(arguments)
        .current_dir(root)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null());
    if skip_version_check {
        command.env("PI_SKIP_VERSION_CHECK", "1");
    }
    let output = tokio::time::timeout(COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| server_error("Pi command timed out"))?
        .map_err(|error| server_error(format!("Could not start Pi: {error}")))?;
    if !output.status.success() {
        return Err(server_error(command_failure(&output)));
    }
    Ok(truncate(
        &String::from_utf8_lossy(&output.stdout),
        64 * 1024,
    ))
}

pub(super) fn settings_manager_module() -> Result<(PathBuf, PathBuf), ServerFnError> {
    pi_package_module("dist/core/settings-manager.js", "settings manager")
}

pub(super) fn pi_runtime_module() -> Result<(PathBuf, PathBuf), ServerFnError> {
    pi_package_module("dist/core/model-runtime.js", "model runtime")
}

fn pi_package_module(
    relative_path: &str,
    module_name: &str,
) -> Result<(PathBuf, PathBuf), ServerFnError> {
    let command = resolve_command(&pi_command()).ok_or_else(|| {
        server_error("Could not locate the Pi executable used by the agent runtime")
    })?;
    let command = fs::canonicalize(command)
        .map_err(|error| server_error(format!("Could not resolve the Pi executable: {error}")))?;
    let package_root = command
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| server_error("Pi is not installed from a loadable npm package"))?;
    let module = package_root.join(relative_path);
    if !module.is_file() {
        return Err(server_error(format!(
            "This Pi installation does not expose its {module_name} module"
        )));
    }
    let node = resolve_command(Path::new("node"))
        .ok_or_else(|| server_error("Node.js is unavailable for Pi integration"))?;
    Ok((node, module))
}

fn resolve_command(command: &Path) -> Option<PathBuf> {
    if command.components().count() > 1 {
        return Some(command.to_owned());
    }
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|path| path.join(command))
        .find(|candidate| candidate.is_file())
}

fn pi_command() -> PathBuf {
    env::var_os("SYNTAXIS_PI_COMMAND").map_or_else(|| PathBuf::from("pi"), PathBuf::from)
}

pub(super) fn agent_dir(root: &Path) -> PathBuf {
    let directory = env::var_os("PI_CODING_AGENT_DIR").map_or_else(
        || {
            env::var_os("HOME")
                .map_or_else(|| PathBuf::from("."), PathBuf::from)
                .join(".pi/agent")
        },
        PathBuf::from,
    );
    if directory.is_absolute() {
        directory
    } else {
        root.join(directory)
    }
}

fn truncate(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.trim().to_owned();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n\n…output truncated…", value[..end].trim())
}

pub(super) fn command_failure(output: &std::process::Output) -> String {
    let stderr = truncate(&String::from_utf8_lossy(&output.stderr), 16 * 1024);
    let stdout = truncate(&String::from_utf8_lossy(&output.stdout), 16 * 1024);
    if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("Pi exited with {}", output.status)
    }
}

pub(super) fn client_error(message: impl Into<String>) -> ServerFnError {
    ServerFnError::ServerError {
        message: message.into(),
        code: 400,
        details: None,
    }
}

pub(super) fn server_error(message: impl Into<String>) -> ServerFnError {
    ServerFnError::ServerError {
        message: message.into(),
        code: 500,
        details: None,
    }
}
