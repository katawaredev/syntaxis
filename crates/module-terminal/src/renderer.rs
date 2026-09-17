//! Shared xterm renderer bridge.

use dioxus::prelude::*;
use serde::Deserialize;
use std::collections::VecDeque;
use syntaxis_app_contracts::PortHandle;
use syntaxis_terminal::{SessionId, TerminalSize};

use crate::{TerminalPorts, TerminalRendererSession};

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
    CopyAll,
    Paste,
    Fit,
    Focus,
}
impl RendererAction {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Copy => "copy",
            Self::CopyAll => "copy_all",
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalRendererEvent {
    Input {
        data: String,
    },
    Resize {
        columns: u16,
        rows: u16,
        pixel_width: u16,
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
#[allow(
    clippy::clone_on_ref_ptr,
    reason = "PortHandle is Rc in the browser and Arc in native runtimes"
)]
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
    let ports = use_context::<TerminalPorts>();
    let element_id = format!("xterm-{}", session_id.0);
    let mut last_sequence = use_signal(|| 0_u64);
    let mut renderer = use_signal(|| None::<PortHandle<dyn TerminalRendererSession>>);
    use_effect({
        let element_id = element_id.clone();
        let port = ports.renderer().cloned();
        move || {
            let element_id = element_id.clone();
            let port = port.clone();
            spawn(async move {
                let Some(port) = port else {
                    on_error.call("The interactive terminal renderer is unavailable.".into());
                    return;
                };
                let session = match port.mount(&element_id).await {
                    Ok(session) => session,
                    Err(problem) => {
                        on_error.call(problem.message);
                        return;
                    }
                };
                renderer.set(Some(session.clone()));
                while let Ok(event) = session.receive().await {
                    match event {
                        TerminalRendererEvent::Input { data } => on_input.call(data.into_bytes()),
                        TerminalRendererEvent::Resize {
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
                        TerminalRendererEvent::Ready => on_ready.call(()),
                        TerminalRendererEvent::ActionResult {
                            action,
                            ok,
                            message,
                        } => on_action_result.call(RendererActionResult {
                            action,
                            ok,
                            message,
                        }),
                        TerminalRendererEvent::SourceLink {
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
                        TerminalRendererEvent::Error { message } => on_error.call(message),
                    }
                }
            });
        }
    });
    use_drop(move || {
        if let Some(renderer) = renderer() {
            renderer.close();
        }
    });
    use_effect({
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
            if let Some(renderer) = renderer() {
                spawn(async move {
                    if let Err(problem) = renderer.write(data).await {
                        on_error.call(problem.message);
                    }
                });
            }
        }
    });
    use_effect({
        move || {
            let Some(command) = command() else {
                return;
            };
            if let Some(renderer) = renderer() {
                spawn(async move {
                    if let Err(problem) = renderer.action(command.action).await {
                        on_error.call(problem.message);
                    }
                });
            }
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
