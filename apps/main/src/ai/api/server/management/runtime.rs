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
    pi_sdk_module()
}

pub(super) fn pi_runtime_module() -> Result<(PathBuf, PathBuf), ServerFnError> {
    pi_sdk_module()
}

pub(super) async fn verify_pi_integration(root: &Path) -> Result<String, ServerFnError> {
    let version = pi_version(root).await?;
    let (node, settings_manager) = settings_manager_module()?;
    let (_, model_runtime) = pi_runtime_module()?;
    let script = r"import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
const [settingsPath, runtimePath, agentDir] = process.argv.slice(1);
const [{ SettingsManager }, { ModelRuntime }] = await Promise.all([
  import(pathToFileURL(settingsPath).href),
  import(pathToFileURL(runtimePath).href),
]);
if (typeof SettingsManager?.create !== 'function' || typeof ModelRuntime?.create !== 'function') {
  throw new Error('Pi integration modules do not expose the expected API');
}
let settings = {};
try {
  settings = JSON.parse(await readFile(join(agentDir, 'settings.json'), 'utf8'));
} catch (error) {
  if (error?.code !== 'ENOENT') throw error;
}
const runtime = await ModelRuntime.create({
  authPath: join(agentDir, 'auth.json'),
  modelsPath: join(agentDir, 'models.json'),
  allowModelNetwork: false,
});
if (settings.defaultProvider) {
  const status = runtime.getProviderAuthStatus(settings.defaultProvider);
  if (!status.configured) throw new Error(`No credentials found for ${settings.defaultProvider}`);
}";
    let output = tokio::time::timeout(
        COMMAND_TIMEOUT,
        tokio::process::Command::new(node)
            .args(["--input-type=module", "--eval", script])
            .arg(settings_manager)
            .arg(model_runtime)
            .arg(agent_dir(root))
            .current_dir(root)
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .output(),
    )
    .await
    .map_err(|_| server_error("Pi integration check timed out"))?
    .map_err(|error| {
        server_error(format!(
            "Could not check the updated Pi integration: {error}"
        ))
    })?;
    if !output.status.success() {
        return Err(server_error(format!(
            "Pi {version} was installed, but its integration check failed: {}",
            command_failure(&output)
        )));
    }
    Ok(version)
}

fn pi_sdk_module() -> Result<(PathBuf, PathBuf), ServerFnError> {
    let command = resolve_command(&pi_command()).ok_or_else(|| {
        server_error("Could not locate the Pi executable used by the agent runtime")
    })?;
    let command = fs::canonicalize(command)
        .map_err(|error| server_error(format!("Could not resolve the Pi executable: {error}")))?;
    let package_root = pi_package_root(&command).ok_or_else(|| {
        server_error(format!(
            "Could not locate the npm package for this Pi installation"
        ))
    })?;
    let module = pi_public_entrypoint(package_root)?;
    let node = resolve_command(Path::new("node"))
        .ok_or_else(|| server_error("Node.js is unavailable for Pi integration"))?;
    Ok((node, module))
}

fn pi_public_entrypoint(package_root: &Path) -> Result<PathBuf, ServerFnError> {
    let manifest_path = package_root.join("package.json");
    let manifest = fs::read(&manifest_path).map_err(|error| {
        server_error(format!(
            "Could not read {}: {error}",
            manifest_path.display()
        ))
    })?;
    let manifest = serde_json::from_slice::<Value>(&manifest).map_err(|error| {
        server_error(format!(
            "Could not parse {}: {error}",
            manifest_path.display()
        ))
    })?;
    let relative = manifest
        .get("exports")
        .and_then(|exports| exports.get("."))
        .and_then(|root| {
            root.as_str()
                .or_else(|| root.get("import").and_then(Value::as_str))
        })
        .ok_or_else(|| server_error("This Pi installation does not expose its public SDK"))?;
    let module = package_root.join(relative.trim_start_matches("./"));
    let module = fs::canonicalize(&module).map_err(|error| {
        server_error(format!(
            "Could not resolve Pi's public SDK entry point: {error}"
        ))
    })?;
    let package_root = fs::canonicalize(package_root).map_err(|error| {
        server_error(format!(
            "Could not resolve the Pi package directory: {error}"
        ))
    })?;
    if !module.starts_with(package_root) || !module.is_file() {
        return Err(server_error(
            "Pi's public SDK entry point is outside its package",
        ));
    }
    Ok(module)
}

fn pi_package_root(command: &Path) -> Option<&Path> {
    command
        .ancestors()
        .find(|ancestor| is_pi_package_root(ancestor))
}

fn is_pi_package_root(path: &Path) -> bool {
    fs::read(path.join("package.json"))
        .ok()
        .and_then(|contents| serde_json::from_slice::<Value>(&contents).ok())
        .and_then(|manifest| {
            manifest
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .is_some_and(|name| name == "@earendil-works/pi-coding-agent")
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
    if let Some(command) = env::var_os("SYNTAXIS_PI_COMMAND") {
        return PathBuf::from(command);
    }
    resolve_command(Path::new("pi"))
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".local/bin/pi"))
                .filter(|candidate| candidate.is_file())
        })
        .unwrap_or_else(|| PathBuf::from("pi"))
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

pub(super) fn conflict_error(message: impl Into<String>) -> ServerFnError {
    ServerFnError::ServerError {
        message: message.into(),
        code: 409,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_root_is_found_for_bundled_cli_layout() {
        let directory = tempfile::tempdir().expect("temporary directory should be available");
        let package = directory.path().join("pi-package");
        let command = package.join("dist/bundle/cli.js");
        fs::create_dir_all(command.parent().expect("command should have a parent"))
            .expect("bundle directory should be created");
        fs::write(
            package.join("package.json"),
            r#"{"name":"@earendil-works/pi-coding-agent"}"#,
        )
        .expect("package manifest should be written");
        fs::write(&command, "#!/usr/bin/env node").expect("fixture command should be written");

        assert_eq!(pi_package_root(&command), Some(package.as_path()));
    }

    #[test]
    fn unrelated_ancestor_package_is_not_used() {
        let directory = tempfile::tempdir().expect("temporary directory should be available");
        let command = directory.path().join("dist/cli.js");
        fs::create_dir_all(command.parent().expect("command should have a parent"))
            .expect("command directory should be created");
        fs::write(directory.path().join("package.json"), r#"{"name":"other"}"#)
            .expect("package manifest should be written");

        assert_eq!(pi_package_root(&command), None);
    }

    #[test]
    fn public_sdk_entrypoint_is_resolved_from_package_exports() {
        let directory = tempfile::tempdir().expect("temporary directory should be available");
        let entrypoint = directory.path().join("dist/index.js");
        fs::create_dir_all(
            entrypoint
                .parent()
                .expect("entrypoint should have a parent"),
        )
        .expect("dist directory should be created");
        fs::write(
            directory.path().join("package.json"),
            r#"{"exports":{".":{"import":"./dist/index.js"}}}"#,
        )
        .expect("package manifest should be written");
        fs::write(&entrypoint, "export {};").expect("entrypoint should be written");

        assert_eq!(
            pi_public_entrypoint(directory.path()).expect("public entrypoint should resolve"),
            fs::canonicalize(entrypoint).expect("entrypoint should canonicalize")
        );
    }
}
