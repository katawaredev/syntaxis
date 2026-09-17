//! Canonical Terminal UI, controller, and runtime ports.

mod command_view;
mod ports;
mod query;
mod renderer;
mod runtime;
mod session;
mod view;

pub use ports::{
    TerminalCommandMutationsPort, TerminalCommandResult, TerminalCommandRunnerPort,
    TerminalCommandsPort, TerminalPorts, TerminalRendererPort, TerminalRendererSession,
    TerminalSessionPort, TerminalSocket, TerminalTransportPort,
};
pub use query::TerminalQuery;
pub use renderer::{RendererAction, RendererActionResult, SourceLink, TerminalRendererEvent};
pub use view::{ProjectInitializerTerminal, TerminalView};
