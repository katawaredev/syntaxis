use dioxus::prelude::{Asset, asset, document};
use serde::Deserialize;

const ARCHIVE_SCRIPT: Asset = asset!("/assets/guest-archive.bundle.js");
const GIT_SCRIPT: Asset = asset!("/assets/guest-git.bundle.js");
const TERMINAL_SCRIPT: Asset = asset!("/assets/guest-terminal.bundle.js");
const BRIDGE_VERSION: u32 = 1;

#[derive(Clone, Copy)]
pub(super) enum BrowserBridge {
    Archive,
    Git,
    Terminal,
}

impl BrowserBridge {
    fn global_name(self) -> &'static str {
        match self {
            Self::Archive => "SyntaxisGuestArchive",
            Self::Git => "SyntaxisGuestGit",
            Self::Terminal => "SyntaxisGuestBash",
        }
    }

    fn script(self) -> Asset {
        match self {
            Self::Archive => ARCHIVE_SCRIPT,
            Self::Git => GIT_SCRIPT,
            Self::Terminal => TERMINAL_SCRIPT,
        }
    }
}

#[derive(Deserialize)]
struct BridgeLoadResponse {
    ok: bool,
    error: Option<String>,
}

/// Loads a browser bridge exactly once per page and verifies its public API version.
pub(super) async fn ensure_bridge(bridge: BrowserBridge) -> Result<(), String> {
    let global_name =
        serde_json::to_string(bridge.global_name()).map_err(|error| error.to_string())?;
    let script_url =
        serde_json::to_string(&bridge.script().to_string()).map_err(|error| error.to_string())?;
    let mut eval = document::eval(&format!(
        r#"
        const globalName = {global_name};
        const scriptUrl = {script_url};
        const version = {BRIDGE_VERSION};
        const ready = () => globalThis[globalName]?.version === version;
        const loadKey = `__syntaxisBridgeLoad:${{globalName}}`;
        try {{
          if (!ready()) {{
            if (!globalThis[loadKey]) {{
              globalThis[loadKey] = new Promise((resolve, reject) => {{
                for (const old of document.querySelectorAll("script[data-syntaxis-bridge]")) {{
                  if (old.dataset.syntaxisBridge === globalName) old.remove();
                }}
                const script = document.createElement("script");
                script.src = scriptUrl;
                script.async = true;
                script.dataset.syntaxisBridge = globalName;
                script.addEventListener("load", resolve, {{ once: true }});
                script.addEventListener(
                  "error",
                  () => reject(new Error(`Could not load ${{globalName}}.`)),
                  {{ once: true }},
                );
                document.head.appendChild(script);
              }}).catch((error) => {{
                delete globalThis[loadKey];
                throw error;
              }});
            }}
            await Promise.race([
              globalThis[loadKey],
              new Promise((_, reject) =>
                setTimeout(() => reject(new Error(`Timed out loading ${{globalName}}.`)), 5000),
              ),
            ]);
          }}
          if (!ready()) throw new Error(`${{globalName}} is unavailable or incompatible.`);
          await dioxus.send({{ ok: true }});
        }} catch (error) {{
          if (!ready()) delete globalThis[loadKey];
          await dioxus.send({{ ok: false, error: error?.message ?? String(error) }});
        }}
        "#,
    ));
    let response = eval
        .recv::<BridgeLoadResponse>()
        .await
        .map_err(|error| format!("Could not verify {}: {error}", bridge.global_name()))?;
    if response.ok {
        Ok(())
    } else {
        Err(response
            .error
            .unwrap_or_else(|| format!("{} is unavailable.", bridge.global_name())))
    }
}
