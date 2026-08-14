use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
};

use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

pub const MAX_LSP_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_HEADER_BYTES: usize = 8 * 1024;
const MAX_PACKAGE_MANIFEST_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedLanguageServer(ResolvedLanguageServerSource);

#[derive(Clone, Debug, Eq, PartialEq)]
enum ResolvedLanguageServerSource {
    Mise(String),
    ProjectLocal(PathBuf),
}

impl ResolvedLanguageServer {
    fn executable(&self, root: &Path) -> Result<OsString, String> {
        match &self.0 {
            ResolvedLanguageServerSource::Mise(executable) => Ok(executable.into()),
            ResolvedLanguageServerSource::ProjectLocal(executable) => {
                let canonical_root = root
                    .canonicalize()
                    .map_err(|_| "The language-server workspace is unavailable")?;
                let canonical = executable
                    .canonicalize()
                    .map_err(|_| "The project language server is unavailable")?;
                if !canonical.starts_with(canonical_root) || !canonical.is_file() {
                    return Err("The project language server is unavailable".into());
                }
                Ok(canonical.into_os_string())
            }
        }
    }
}

pub struct LanguageServer {
    pub child: Child,
    pub reader: LanguageServerReader,
    pub writer: LanguageServerWriter,
}

pub struct LanguageServerReader {
    stdout: BufReader<ChildStdout>,
}

pub struct LanguageServerWriter {
    stdin: ChildStdin,
}

impl LanguageServer {
    /// Starts an allowlisted language server through the workspace's mise environment.
    ///
    /// The executable and arguments must come from the host application's
    /// language-server registry, never directly from a client.
    ///
    /// # Errors
    ///
    /// Returns an error when the workspace, mise, or configured executable is
    /// unavailable, or the server's standard I/O streams cannot be opened.
    pub fn start_mise(
        root: &Path,
        executable: &ResolvedLanguageServer,
        arguments: &[&str],
    ) -> Result<Self, String> {
        let mut command = Command::new("mise");
        command
            .args(["exec", "--"])
            .arg(executable.executable(root)?)
            .args(arguments);
        Self::start_command(command, root)
    }

    #[cfg(test)]
    fn start(command: &str, arguments: &[&str], root: &Path) -> Result<Self, String> {
        let mut command = Command::new(command);
        command.args(arguments);
        Self::start_command(command, root)
    }

    fn start_command(mut command: Command, root: &Path) -> Result<Self, String> {
        if !root.is_absolute() || !root.is_dir() {
            return Err("The language-server workspace is unavailable".into());
        }
        let mut child = command
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| "Could not start the mise-managed language server".to_owned())?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Could not open language-server input".to_owned())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Could not open language-server output".to_owned())?;
        Ok(Self {
            child,
            reader: LanguageServerReader {
                stdout: BufReader::new(stdout),
            },
            writer: LanguageServerWriter { stdin },
        })
    }
}

impl LanguageServerReader {
    /// Reads one bounded Content-Length-framed JSON-RPC message.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed, oversized, truncated, or non-JSON
    /// language-server output.
    pub async fn receive(&mut self) -> Result<Option<String>, String> {
        let mut content_length = None;
        let mut header_bytes = 0;
        loop {
            let mut line = String::new();
            let read = self
                .stdout
                .read_line(&mut line)
                .await
                .map_err(|_| "Could not read language-server output".to_owned())?;
            if read == 0 {
                return Ok(None);
            }
            header_bytes += read;
            if header_bytes > MAX_HEADER_BYTES {
                return Err("Language-server headers exceeded the safety limit".into());
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
            if let Some(value) = line
                .strip_prefix("Content-Length:")
                .map(str::trim)
                .and_then(|value| value.parse::<usize>().ok())
            {
                content_length = Some(value);
            }
        }
        let length =
            content_length.ok_or_else(|| "Language server omitted Content-Length".to_owned())?;
        if length == 0 || length > MAX_LSP_MESSAGE_BYTES {
            return Err("Language-server message exceeded the safety limit".into());
        }
        let mut message = vec![0_u8; length];
        self.stdout
            .read_exact(&mut message)
            .await
            .map_err(|_| "Language-server output ended unexpectedly".to_owned())?;
        let message = String::from_utf8(message)
            .map_err(|_| "Language server returned invalid UTF-8".to_owned())?;
        validate_json_rpc(&message)?;
        Ok(Some(message))
    }
}

impl LanguageServerWriter {
    /// Writes one bounded JSON-RPC message using LSP stdio framing.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed or oversized JSON, or when the language
    /// server no longer accepts input.
    pub async fn send(&mut self, message: &str) -> Result<(), String> {
        if message.is_empty() || message.len() > MAX_LSP_MESSAGE_BYTES {
            return Err("Language-client message exceeded the safety limit".into());
        }
        validate_json_rpc(message)?;
        let header = format!("Content-Length: {}\r\n\r\n", message.len());
        self.stdin
            .write_all(header.as_bytes())
            .await
            .map_err(|_| "Could not write to the language server".to_owned())?;
        self.stdin
            .write_all(message.as_bytes())
            .await
            .map_err(|_| "Could not write to the language server".to_owned())?;
        self.stdin
            .flush()
            .await
            .map_err(|_| "Could not write to the language server".to_owned())
    }
}

#[must_use]
pub fn mise_available() -> bool {
    command_available("mise")
}

/// Resolves a project-local executable, then falls back to the workspace's Mise
/// configuration.
///
/// # Errors
///
/// Returns an error when the workspace is unavailable or mise cannot inspect it.
pub async fn resolve_language_server(
    root: &Path,
    executable: &str,
    project_package: Option<&str>,
    minimum_project_major: Option<u64>,
) -> Result<Option<ResolvedLanguageServer>, String> {
    if !root.is_absolute() || !root.is_dir() {
        return Err("The language-server workspace is unavailable".into());
    }
    if !mise_available() {
        return Err("mise is not installed in this runtime".into());
    }
    if !is_single_path_component(executable)
        || project_package.is_some_and(|package| !is_package_path(package))
    {
        return Err("The language-server definition is invalid".into());
    }
    if let Some(package) = project_package
        && let Some(executable) =
            project_local_language_server(root, package, executable, minimum_project_major).await
    {
        return Ok(Some(ResolvedLanguageServer(
            ResolvedLanguageServerSource::ProjectLocal(executable),
        )));
    }
    let available = Command::new("mise")
        .args(["which", executable])
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map_err(|_| "mise could not inspect the workspace tools".to_owned())?
        .success();
    Ok(available
        .then(|| ResolvedLanguageServer(ResolvedLanguageServerSource::Mise(executable.to_owned()))))
}

fn is_single_path_component(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none()
}

fn is_package_path(value: &str) -> bool {
    let components = Path::new(value).components().collect::<Vec<_>>();
    !components.is_empty()
        && components.len() <= 2
        && components
            .iter()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

async fn project_local_language_server(
    root: &Path,
    package: &str,
    executable: &str,
    minimum_major: Option<u64>,
) -> Option<PathBuf> {
    let canonical_root = tokio::fs::canonicalize(root).await.ok()?;
    let package_manifest = safe_workspace_file(
        &canonical_root,
        &canonical_root
            .join("node_modules")
            .join(package)
            .join("package.json"),
    )
    .await?;
    if let Some(minimum_major) = minimum_major {
        let major = package_version_major(&package_manifest).await?;
        if major < minimum_major {
            return None;
        }
    }
    safe_workspace_file(
        &canonical_root,
        &canonical_root
            .join("node_modules")
            .join(".bin")
            .join(executable),
    )
    .await
}

async fn safe_workspace_file(canonical_root: &Path, candidate: &Path) -> Option<PathBuf> {
    let canonical = tokio::fs::canonicalize(candidate).await.ok()?;
    let metadata = tokio::fs::metadata(&canonical).await.ok()?;
    (canonical.starts_with(canonical_root) && metadata.is_file()).then_some(canonical)
}

async fn package_version_major(manifest: &Path) -> Option<u64> {
    let file = tokio::fs::File::open(manifest).await.ok()?;
    let mut contents = Vec::new();
    file.take(MAX_PACKAGE_MANIFEST_BYTES + 1)
        .read_to_end(&mut contents)
        .await
        .ok()?;
    if u64::try_from(contents.len()).ok()? > MAX_PACKAGE_MANIFEST_BYTES {
        return None;
    }
    serde_json::from_slice::<serde_json::Value>(&contents)
        .ok()?
        .get("version")?
        .as_str()?
        .split('.')
        .next()?
        .parse()
        .ok()
}

fn command_available(command: &str) -> bool {
    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|directory| directory.join(command).is_file())
    })
}

fn validate_json_rpc(message: &str) -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_str(message).map_err(|_| "Invalid language-server JSON".to_owned())?;
    if value.is_object() {
        Ok(())
    } else {
        Err("Language-server messages must be JSON objects".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    fn json_rpc_validation_rejects_non_objects() {
        validate_json_rpc(r#"{"jsonrpc":"2.0","method":"initialized"}"#).unwrap();
        assert!(validate_json_rpc("[]").is_err());
        assert!(validate_json_rpc("not json").is_err());
    }

    #[test]
    fn missing_command_paths_are_detected() {
        let path = PathBuf::from("/definitely/missing/language-server");
        assert!(!path.is_file());
    }

    #[tokio::test]
    async fn project_local_servers_are_version_gated_and_canonicalized() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let package = root.join("node_modules/typescript");
        let binaries = root.join("node_modules/.bin");
        fs::create_dir_all(&package).unwrap();
        fs::create_dir_all(&binaries).unwrap();
        fs::write(package.join("package.json"), r#"{"version":"6.9.0"}"#).unwrap();
        fs::write(binaries.join("tsc"), "#!/usr/bin/env node").unwrap();

        assert!(
            project_local_language_server(root, "typescript", "tsc", Some(7))
                .await
                .is_none()
        );
        fs::write(package.join("package.json"), r#"{"version":"7.1.0"}"#).unwrap();
        let resolved = project_local_language_server(root, "typescript", "tsc", Some(7))
            .await
            .unwrap();

        assert_eq!(resolved, binaries.join("tsc").canonicalize().unwrap());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn project_local_servers_cannot_escape_the_workspace() {
        use std::os::unix::fs::symlink;

        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("workspace");
        let package = root.join("node_modules/example");
        let binaries = root.join("node_modules/.bin");
        fs::create_dir_all(&package).unwrap();
        fs::create_dir_all(&binaries).unwrap();
        fs::write(package.join("package.json"), r#"{"version":"1.0.0"}"#).unwrap();
        let outside = parent.path().join("outside-server");
        fs::write(&outside, "#!/usr/bin/env node").unwrap();
        symlink(&outside, binaries.join("example-server")).unwrap();

        assert!(
            project_local_language_server(root.as_path(), "example", "example-server", None)
                .await
                .is_none()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn framed_messages_round_trip_through_a_child_process() {
        let root = std::env::current_dir().expect("the test has a current directory");
        let mut server = LanguageServer::start("/bin/cat", &[], &root).expect("cat should run");
        let message = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;

        server.writer.send(message).await.unwrap();
        assert_eq!(
            server.reader.receive().await.unwrap().as_deref(),
            Some(message)
        );
        server.child.kill().await.unwrap();
    }
}
