use dioxus::prelude::*;
use serde::Deserialize;
use std::collections::VecDeque;
use syntaxis_terminal::{SessionId, TerminalSize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct SourceLink {
    pub path: String,
    pub line: usize,
    pub column: Option<usize>,
    pub end_line: Option<usize>,
    pub end_column: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RendererOutput {
    pub session_id: SessionId,
    pub sequence: u64,
    pub data: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RendererOutputBatch {
    pub session_id: SessionId,
    pub revision: u64,
    pub chunks: VecDeque<RendererOutput>,
    bytes: usize,
}
impl RendererOutputBatch {
    pub fn new(session_id: SessionId) -> Self {
        Self {
            session_id,
            revision: 0,
            chunks: VecDeque::new(),
            bytes: 0,
        }
    }

    pub fn push(&mut self, chunk: RendererOutput, byte_limit: usize) {
        self.revision = self.revision.saturating_add(1);
        self.bytes = self.bytes.saturating_add(chunk.data.len());
        self.chunks.push_back(chunk);
        while self.bytes > byte_limit && self.chunks.len() > 1 {
            if let Some(removed) = self.chunks.pop_front() {
                self.bytes = self.bytes.saturating_sub(removed.data.len());
            }
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RendererAction {
    Clear,
    Copy,
    Paste,
    Fit,
    Focus,
}
impl RendererAction {
    const fn name(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Copy => "copy",
            Self::Paste => "paste",
            Self::Fit => "fit",
            Self::Focus => "focus",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RendererCommand {
    pub sequence: u64,
    pub action: RendererAction,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RendererActionResult {
    pub action: String,
    pub ok: bool,
    pub message: String,
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
#[component]
pub fn XtermRenderer(
    session_id: SessionId,
    output: ReadSignal<Option<RendererOutputBatch>>,
    command: ReadSignal<Option<RendererCommand>>,
    on_input: EventHandler<Vec<u8>>,
    on_resize: EventHandler<TerminalSize>,
    on_ready: EventHandler<()>,
    on_action_result: EventHandler<RendererActionResult>,
    on_source_link: EventHandler<SourceLink>,
    on_error: EventHandler<String>,
) -> Element {
    let element_id = format!("xterm-{}", session_id.0);
    let mut last_sequence = use_signal(|| 0_u64);
    let mut event_bridge = use_signal(|| None::<dioxus::document::Eval>);
    use_effect({
        let element_id = element_id.clone();
        move || {
            let mut events = document::eval(
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
            let _ = events.send(element_id.clone());
            event_bridge.set(Some(events));
            spawn(async move {
                while let Ok(event) = events.recv::<BridgeEvent>().await {
                    match event {
                        BridgeEvent::Input { data } => on_input.call(data.into_bytes()),
                        BridgeEvent::Resize {
                            columns,
                            rows,
                            pixel_width,
                            pixel_height,
                        } => on_resize.call(TerminalSize {
                            columns,
                            rows,
                            pixel_width,
                            pixel_height,
                        }),
                        BridgeEvent::Ready => on_ready.call(()),
                        BridgeEvent::ActionResult {
                            action,
                            ok,
                            message,
                        } => on_action_result.call(RendererActionResult {
                            action,
                            ok,
                            message,
                        }),
                        BridgeEvent::SourceLink {
                            path,
                            line,
                            column,
                            end_line,
                            end_column,
                        } => on_source_link.call(SourceLink {
                            path,
                            line,
                            column,
                            end_line,
                            end_column,
                        }),
                        BridgeEvent::Error { message } => on_error.call(message),
                    }
                }
            });
            let mount = document::eval(
                r#"
                const id = await dioxus.recv();
                for (let attempt = 0; attempt < 100 && !window.SyntaxisTerminalBridge; attempt++) {
                    await new Promise(resolve => setTimeout(resolve, 20));
                }
                try {
                    if (!window.SyntaxisTerminalBridge) throw new Error("Terminal renderer did not load");
                    await window.SyntaxisTerminalBridge.mount(id);
                } catch (error) {
                    window.dispatchEvent(new CustomEvent("syntaxis-terminal", {
                        detail: { kind: "error", id, message: String(error?.message ?? error) },
                    }));
                }
                "#,
            );
            let _ = mount.send(element_id.clone());
        }
    });
    use_drop({
        let element_id = element_id.clone();
        move || {
            if let Some(events) = event_bridge() {
                let _ = events.send(true);
            }
            let dispose = document::eval(
                r"
                const id = await dioxus.recv();
                window.SyntaxisTerminalBridge?.dispose(id);
                ",
            );
            let _ = dispose.send(element_id);
        }
    });
    use_effect({
        let element_id = element_id.clone();
        let session_id = session_id.clone();
        move || {
            let output = output.read();
            let Some(output) = output.as_ref() else {
                return;
            };
            if output.session_id != session_id {
                return;
            }
            let mut data = Vec::<u8>::new();
            let mut newest = last_sequence();
            for chunk in &output.chunks {
                if chunk.sequence > newest {
                    newest = chunk.sequence;
                    data.extend_from_slice(&chunk.data);
                }
            }
            if data.is_empty() {
                return;
            }
            last_sequence.set(newest);
            let write = document::eval(
                r"
                const [id, data] = await dioxus.recv();
                window.SyntaxisTerminalBridge?.write(id, data);
                ",
            );
            let _ = write.send((element_id.clone(), data));
        }
    });
    use_effect({
        let element_id = element_id.clone();
        move || {
            let Some(command) = command() else {
                return;
            };
            let action = document::eval(
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
            let _ = action.send((element_id.clone(), command.action.name()));
        }
    });
    rsx! {
        div {
            id: element_id,
            class: "xterm-host relative size-full min-h-0 overflow-hidden bg-card px-3 py-2.5 outline-none focus-visible:-outline-offset-1 focus-visible:outline-1 focus-visible:outline-primary/65",
            role: "application",
            tabindex: "0",
            "aria-label": "Interactive workspace terminal",
        }
    }
}
