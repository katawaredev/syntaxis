#![allow(
    clippy::arc_with_non_send_sync,
    reason = "The renderer is confined to the non-Send Dioxus runtime"
)]

use async_trait::async_trait;
use dioxus::prelude::{Asset, asset, document};
use futures::lock::Mutex;
use serde::Deserialize;
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, PortHandle, RetryAdvice};
use syntaxis_module_terminal::{
    RendererAction, TerminalRendererEvent, TerminalRendererPort, TerminalRendererSession,
};

const TERMINAL_SCRIPT: Asset = asset!("/assets/terminal.bundle.js");
const TERMINAL_BRIDGE_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default)]
pub struct DioxusTerminalRenderer;

struct DioxusTerminalRendererSession {
    element_id: String,
    events: Mutex<dioxus::document::Eval>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum BridgeEvent {
    Input {
        data: String,
    },
    Resize {
        columns: u16,
        rows: u16,
        #[serde(rename = "pixelWidth")]
        pixel_width: u16,
        #[serde(rename = "pixelHeight")]
        pixel_height: u16,
    },
    Ready,
    ActionResult {
        action: String,
        ok: bool,
        message: String,
    },
    SourceLink {
        path: String,
        line: usize,
        column: Option<usize>,
        end_line: Option<usize>,
        end_column: Option<usize>,
    },
    Error {
        message: String,
    },
}

#[derive(Deserialize)]
struct BridgeLoadResponse {
    ok: bool,
    error: Option<String>,
}

#[async_trait(?Send)]
impl TerminalRendererPort for DioxusTerminalRenderer {
    async fn mount(
        &self,
        element_id: &str,
    ) -> Result<PortHandle<dyn TerminalRendererSession>, AppError> {
        ensure_terminal_bridge().await?;
        let events = document::eval(
            r#"
            const id = await dioxus.recv();
            const listener = event => {
                if (event.detail?.id === id) dioxus.send(event.detail);
            };
            window.addEventListener("syntaxis-terminal", listener);
            const viewport = window.matchMedia("(pointer: coarse)").matches
                ? window.visualViewport
                : null;
            const container = document.getElementById(id);
            const shell = container?.closest("[data-terminal-shell]");
            const originalMaxHeight = shell?.style.maxHeight ?? "";
            let fitFrame = null;
            const fitVisibleTerminal = () => {
                if (!viewport || !shell) return;
                const visibleBottom = viewport.offsetTop + viewport.height;
                const available = Math.max(160, Math.floor(visibleBottom - shell.getBoundingClientRect().top));
                shell.style.maxHeight = `${available}px`;
                if (fitFrame !== null) cancelAnimationFrame(fitFrame);
                fitFrame = requestAnimationFrame(() => {
                    window.SyntaxisTerminalBridge?.action(id, "fit");
                    fitFrame = null;
                });
            };
            viewport?.addEventListener("resize", fitVisibleTerminal);
            viewport?.addEventListener("scroll", fitVisibleTerminal);
            fitVisibleTerminal();
            await dioxus.recv();
            window.removeEventListener("syntaxis-terminal", listener);
            viewport?.removeEventListener("resize", fitVisibleTerminal);
            viewport?.removeEventListener("scroll", fitVisibleTerminal);
            if (fitFrame !== null) cancelAnimationFrame(fitFrame);
            if (shell) shell.style.maxHeight = originalMaxHeight;
            "#,
        );
        events.send(element_id).map_err(renderer_error)?;
        let mount = document::eval(
            r#"
            const id = await dioxus.recv();
            try {
                if (window.SyntaxisTerminalBridge?.version !== 1) {
                    throw new Error("Terminal renderer is unavailable or incompatible");
                }
                await window.SyntaxisTerminalBridge.mount(id);
            } catch (error) {
                window.dispatchEvent(new CustomEvent("syntaxis-terminal", {
                    detail: { kind: "error", id, message: String(error?.message ?? error) },
                }));
            }
            "#,
        );
        mount.send(element_id).map_err(renderer_error)?;
        Ok(PortHandle::new(DioxusTerminalRendererSession {
            element_id: element_id.to_owned(),
            events: Mutex::new(events),
        }))
    }
}

async fn ensure_terminal_bridge() -> Result<(), AppError> {
    let script_url = serde_json::to_string(&TERMINAL_SCRIPT.to_string()).map_err(renderer_error)?;
    let mut eval = document::eval(&format!(
        r#"
        const scriptUrl = {script_url};
        const version = {TERMINAL_BRIDGE_VERSION};
        const globalName = "SyntaxisTerminalBridge";
        const ready = () => globalThis[globalName]?.version === version;
        const loadKey = "__syntaxisTerminalBridgeLoad";
        try {{
          if (!ready()) {{
            if (!globalThis[loadKey]) {{
              globalThis[loadKey] = new Promise((resolve, reject) => {{
                for (const old of document.querySelectorAll("script[data-syntaxis-terminal-bridge]")) {{
                  old.remove();
                }}
                const script = document.createElement("script");
                script.src = scriptUrl;
                script.async = true;
                script.dataset.syntaxisTerminalBridge = "true";
                script.addEventListener("load", resolve, {{ once: true }});
                script.addEventListener(
                  "error",
                  () => reject(new Error("Could not load the terminal renderer.")),
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
                setTimeout(() => reject(new Error("Timed out loading the terminal renderer.")), 5000),
              ),
            ]);
          }}
          if (!ready()) throw new Error("Terminal renderer is unavailable or incompatible.");
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
        .map_err(renderer_error)?;
    if response.ok {
        Ok(())
    } else {
        Err(renderer_unavailable(response.error.unwrap_or_else(|| {
            "Terminal renderer is unavailable.".to_owned()
        })))
    }
}

#[async_trait(?Send)]
impl TerminalRendererSession for DioxusTerminalRendererSession {
    async fn receive(&self) -> Result<TerminalRendererEvent, AppError> {
        self.events
            .lock()
            .await
            .recv::<BridgeEvent>()
            .await
            .map(map_event)
            .map_err(renderer_error)
    }

    async fn write(&self, data: Vec<u8>) -> Result<(), AppError> {
        let write = document::eval(
            r"
            const [id, data] = await dioxus.recv();
            window.SyntaxisTerminalBridge?.write(id, data);
            ",
        );
        write
            .send((self.element_id.clone(), data))
            .map_err(renderer_error)
    }

    async fn action(&self, action: RendererAction) -> Result<(), AppError> {
        let command = document::eval(
            r#"
            const [id, action] = await dioxus.recv();
            if (action === "focus") {
                const input = document.getElementById(id)?.querySelector("textarea");
                if (input instanceof HTMLTextAreaElement) input.focus({ preventScroll: true });
                else window.SyntaxisTerminalBridge?.action(id, action);
            } else {
                window.SyntaxisTerminalBridge?.action(id, action);
            }
            "#,
        );
        command
            .send((self.element_id.clone(), action.name()))
            .map_err(renderer_error)
    }

    fn close(&self) {
        if let Some(events) = self.events.try_lock() {
            let _ = events.send(true);
        }
        let dispose = document::eval(
            r"
            const id = await dioxus.recv();
            window.SyntaxisTerminalBridge?.dispose(id);
            ",
        );
        let _ = dispose.send(self.element_id.clone());
    }
}

fn map_event(event: BridgeEvent) -> TerminalRendererEvent {
    match event {
        BridgeEvent::Input { data } => TerminalRendererEvent::Input { data },
        BridgeEvent::Resize {
            columns,
            rows,
            pixel_width,
            pixel_height,
        } => TerminalRendererEvent::Resize {
            columns,
            rows,
            pixel_width,
            pixel_height,
        },
        BridgeEvent::Ready => TerminalRendererEvent::Ready,
        BridgeEvent::ActionResult {
            action,
            ok,
            message,
        } => TerminalRendererEvent::ActionResult {
            action,
            ok,
            message,
        },
        BridgeEvent::SourceLink {
            path,
            line,
            column,
            end_line,
            end_column,
        } => TerminalRendererEvent::SourceLink {
            path,
            line,
            column,
            end_line,
            end_column,
        },
        BridgeEvent::Error { message } => TerminalRendererEvent::Error { message },
    }
}

fn renderer_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        AppErrorCode::Internal,
        error.to_string(),
        RetryAdvice::AfterUserAction,
        ErrorSource::Terminal,
    )
}

fn renderer_unavailable(message: impl Into<String>) -> AppError {
    AppError::new(
        AppErrorCode::Offline,
        message,
        RetryAdvice::Backoff,
        ErrorSource::Terminal,
    )
}
