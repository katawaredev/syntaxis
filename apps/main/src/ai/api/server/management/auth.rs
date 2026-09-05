use std::{
    collections::BTreeMap,
    path::Path,
    process::Stdio,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use dioxus::prelude::ServerFnError;
use serde::Deserialize;
use serde_json::json;
use syntaxis_workspace::WorkspaceId;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::ChildStdin,
    sync::Mutex,
};

use crate::ai::api::{PiAuthEvent, PiAuthFlow, PiAuthPrompt, PiAuthType, PiProviderAuth};

use super::{
    COMMAND_TIMEOUT, agent_dir, client_error, command_failure, pi_runtime_module, server_error,
};

static AUTH_FLOWS: OnceLock<Mutex<BTreeMap<String, Arc<AuthFlowProcess>>>> = OnceLock::new();
static NEXT_AUTH_FLOW_ID: AtomicU64 = AtomicU64::new(1);

struct AuthFlowProcess {
    stdin: Mutex<ChildStdin>,
    state: Mutex<PiAuthFlow>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AuthFlowOutput {
    Prompt { prompt: PiAuthPrompt },
    Event { event: PiAuthEvent },
    Done,
    Error { message: String },
}

fn auth_flows() -> &'static Mutex<BTreeMap<String, Arc<AuthFlowProcess>>> {
    AUTH_FLOWS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(crate) async fn pi_providers(
    workspace_id: WorkspaceId,
) -> Result<Vec<PiProviderAuth>, ServerFnError> {
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let (node, runtime) = pi_runtime_module()?;
    let root = Path::new(&workspace.root);
    let script = r"import { pathToFileURL } from 'node:url';
const [runtimePath, authPath, modelsPath] = process.argv.slice(1);
const { ModelRuntime } = await import(pathToFileURL(runtimePath).href);
const runtime = await ModelRuntime.create({
  authPath,
  modelsPath,
  allowModelNetwork: false,
});
const storedProviders = new Set(
  (await runtime.listCredentials()).map(credential => credential.providerId),
);
const providers = await Promise.all(runtime.getProviders().map(async provider => {
  const status = runtime.getProviderAuthStatus(provider.id);
  const check = await runtime.checkAuth(provider.id);
  const methods = [];
  if (provider.auth.apiKey?.login) {
    methods.push({ auth_type: 'api_key', label: provider.auth.apiKey.name });
  }
  if (provider.auth.oauth) {
    methods.push({
      auth_type: 'oauth',
      label: provider.auth.oauth.loginLabel ?? provider.auth.oauth.name,
    });
  }
  return {
    id: provider.id,
    name: provider.name,
    configured: status.configured,
    can_logout: storedProviders.has(provider.id),
    status: status.configured
      ? (status.label ?? check?.source ?? status.source ?? 'Configured')
      : 'Not configured',
    methods,
  };
}));
providers.sort((left, right) => left.name.localeCompare(right.name));
process.stdout.write(JSON.stringify(providers));";
    let output = tokio::time::timeout(
        COMMAND_TIMEOUT,
        tokio::process::Command::new(node)
            .args(["--input-type=module", "--eval", script])
            .arg(runtime)
            .arg(agent_dir(root).join("auth.json"))
            .arg(agent_dir(root).join("models.json"))
            .current_dir(root)
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .output(),
    )
    .await
    .map_err(|_| server_error("Loading Pi providers timed out"))?
    .map_err(|error| server_error(format!("Could not start Pi's model runtime: {error}")))?;
    if !output.status.success() {
        return Err(server_error(command_failure(&output)));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| server_error(format!("Pi returned invalid provider metadata: {error}")))
}

#[expect(
    clippy::too_many_lines,
    reason = "the login bridge keeps Pi's subprocess protocol and Rust-side process setup together"
)]
pub(crate) async fn start_pi_provider_login(
    workspace_id: WorkspaceId,
    provider_id: String,
    auth_type: PiAuthType,
) -> Result<PiAuthFlow, ServerFnError> {
    let provider_id = provider_id.trim().to_owned();
    if provider_id.is_empty() || provider_id.len() > 100 {
        return Err(client_error("A valid Pi provider is required"));
    }
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let (node, runtime) = pi_runtime_module()?;
    let root = Path::new(&workspace.root);
    let auth_type = match auth_type {
        PiAuthType::ApiKey => "api_key",
        PiAuthType::Oauth => "oauth",
    };
    let script = r"import { pathToFileURL } from 'node:url';
import { createInterface } from 'node:readline';
const [runtimePath, authPath, modelsPath, providerId, authType] = process.argv.slice(1);
const { ModelRuntime } = await import(pathToFileURL(runtimePath).href);
const write = value => process.stdout.write(`${JSON.stringify(value)}\n`);
const abort = new AbortController();
const pending = new Map();
let nextPromptId = 1;
const input = createInterface({ input: process.stdin, crlfDelay: Infinity });
input.on('line', line => {
  try {
    const message = JSON.parse(line);
    if (message.type === 'cancel') {
      abort.abort();
      for (const entry of pending.values()) entry.reject(new Error('Login cancelled'));
      pending.clear();
      return;
    }
    if (message.type === 'response') {
      const entry = pending.get(message.prompt_id);
      if (entry) {
        pending.delete(message.prompt_id);
        entry.resolve(message.value);
      }
    }
  } catch {}
});
try {
  const runtime = await ModelRuntime.create({
    authPath,
    modelsPath,
    allowModelNetwork: false,
  });
  const provider = runtime.getProvider(providerId);
  const method = authType === 'oauth' ? provider?.auth.oauth : provider?.auth.apiKey;
  if (!provider || !method || (authType === 'api_key' && !method.login)) {
    throw new Error(`Authentication method is unavailable for ${providerId}`);
  }
  await runtime.login(providerId, authType, {
    signal: abort.signal,
    prompt: prompt => {
      const id = nextPromptId++;
      write({
        type: 'prompt',
        prompt: {
          id,
          kind: prompt.type,
          message: prompt.message,
          placeholder: prompt.placeholder ?? '',
          options: (prompt.options ?? []).map(option => ({
            id: option.id,
            label: option.label,
            description: option.description ?? '',
          })),
        },
      });
      return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
    },
    notify: event => {
      let message = event.message ?? event.instructions ?? '';
      let url = event.url ?? event.verificationUri ?? '';
      let userCode = event.userCode ?? '';
      write({
        type: 'event',
        event: { kind: event.type, message, url, user_code: userCode },
      });
    },
  });
  write({ type: 'done' });
} catch (error) {
  write({
    type: 'error',
    message: error instanceof Error ? error.message : String(error),
  });
  process.exitCode = 1;
} finally {
  input.close();
}";
    let mut child = tokio::process::Command::new(node)
        .args(["--input-type=module", "--eval", script])
        .arg(runtime)
        .arg(agent_dir(root).join("auth.json"))
        .arg(agent_dir(root).join("models.json"))
        .arg(&provider_id)
        .arg(auth_type)
        .current_dir(root)
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| server_error(format!("Could not start Pi's login flow: {error}")))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| server_error("Pi's login flow has no input stream"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| server_error("Pi's login flow has no output stream"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| server_error("Pi's login flow has no error stream"))?;
    let flow_id = new_auth_flow_id();
    let initial = PiAuthFlow {
        id: flow_id.clone(),
        provider_id,
        prompt: None,
        events: Vec::new(),
        complete: false,
        error: None,
    };
    let process = Arc::new(AuthFlowProcess {
        stdin: Mutex::new(stdin),
        state: Mutex::new(initial.clone()),
    });
    auth_flows()
        .lock()
        .await
        .insert(flow_id, Arc::clone(&process));
    tokio::spawn(monitor_auth_flow(child, stdout, stderr, process));
    Ok(initial)
}

pub(crate) async fn pi_provider_login_status(flow_id: String) -> Result<PiAuthFlow, ServerFnError> {
    let process = auth_flows()
        .lock()
        .await
        .get(&flow_id)
        .cloned()
        .ok_or_else(|| client_error("This Pi login flow is no longer available"))?;
    let snapshot = process.state.lock().await.clone();
    Ok(snapshot)
}

pub(crate) async fn respond_to_pi_provider_login(
    flow_id: String,
    prompt_id: u64,
    value: String,
) -> Result<(), ServerFnError> {
    if value.len() > 128 * 1024 {
        return Err(client_error("Pi login responses are limited to 128 KiB"));
    }
    let process = auth_flows()
        .lock()
        .await
        .get(&flow_id)
        .cloned()
        .ok_or_else(|| client_error("This Pi login flow is no longer available"))?;
    {
        let state = process.state.lock().await;
        if state.prompt.as_ref().map(|prompt| prompt.id) != Some(prompt_id) {
            return Err(client_error("That Pi login prompt is no longer active"));
        }
    }
    let message = json!({
        "type": "response",
        "prompt_id": prompt_id,
        "value": value,
    });
    let mut stdin = process.stdin.lock().await;
    stdin
        .write_all(format!("{message}\n").as_bytes())
        .await
        .map_err(|error| server_error(format!("Could not answer Pi's login prompt: {error}")))?;
    process.state.lock().await.prompt = None;
    Ok(())
}

pub(crate) async fn cancel_pi_provider_login(flow_id: String) -> Result<(), ServerFnError> {
    let Some(process) = auth_flows().lock().await.remove(&flow_id) else {
        return Ok(());
    };
    let _ = process
        .stdin
        .lock()
        .await
        .write_all(b"{\"type\":\"cancel\"}\n")
        .await;
    Ok(())
}

pub(crate) async fn logout_pi_provider(
    workspace_id: WorkspaceId,
    provider_id: String,
) -> Result<(), ServerFnError> {
    let provider_id = provider_id.trim().to_owned();
    if provider_id.is_empty() || provider_id.len() > 100 {
        return Err(client_error("A valid Pi provider is required"));
    }
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let (node, runtime) = pi_runtime_module()?;
    let root = Path::new(&workspace.root);
    let script = r"import { pathToFileURL } from 'node:url';
const [runtimePath, authPath, modelsPath, providerId] = process.argv.slice(1);
const { ModelRuntime } = await import(pathToFileURL(runtimePath).href);
const runtime = await ModelRuntime.create({
  authPath,
  modelsPath,
  allowModelNetwork: false,
});
if (!runtime.getProvider(providerId)) throw new Error(`Unknown provider: ${providerId}`);
await runtime.logout(providerId);";
    let output = tokio::time::timeout(
        COMMAND_TIMEOUT,
        tokio::process::Command::new(node)
            .args(["--input-type=module", "--eval", script])
            .arg(runtime)
            .arg(agent_dir(root).join("auth.json"))
            .arg(agent_dir(root).join("models.json"))
            .arg(provider_id)
            .current_dir(root)
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .output(),
    )
    .await
    .map_err(|_| server_error("Pi provider logout timed out"))?
    .map_err(|error| server_error(format!("Could not start Pi's model runtime: {error}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(server_error(command_failure(&output)))
    }
}

async fn monitor_auth_flow(
    mut child: tokio::process::Child,
    stdout: tokio::process::ChildStdout,
    mut stderr: tokio::process::ChildStderr,
    process: Arc<AuthFlowProcess>,
) {
    let stderr_task = tokio::spawn(async move {
        let mut output = String::new();
        let _ = stderr.read_to_string(&mut output).await;
        output
    });
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(output) = serde_json::from_str::<AuthFlowOutput>(&line) else {
            continue;
        };
        let mut state = process.state.lock().await;
        match output {
            AuthFlowOutput::Prompt { prompt } => state.prompt = Some(prompt),
            AuthFlowOutput::Event { event } => {
                if state.events.len() == 32 {
                    state.events.remove(0);
                }
                state.events.push(event);
            }
            AuthFlowOutput::Done => {
                state.complete = true;
                state.prompt = None;
            }
            AuthFlowOutput::Error { message } => {
                state.error = Some(message);
                state.prompt = None;
            }
        }
    }
    let status = child.wait().await;
    let stderr = stderr_task.await.unwrap_or_default();
    let mut state = process.state.lock().await;
    if !state.complete && state.error.is_none() {
        state.error = Some(match status {
            Ok(status) if !stderr.trim().is_empty() => {
                format!("Pi login failed ({status}): {}", stderr.trim())
            }
            Ok(status) => format!("Pi login stopped ({status})"),
            Err(error) => format!("Could not wait for Pi's login flow: {error}"),
        });
    }
}

fn new_auth_flow_id() -> String {
    let unix_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let sequence = NEXT_AUTH_FLOW_ID.fetch_add(1, Ordering::Relaxed);
    format!("{unix_millis:x}-{sequence:x}")
}
