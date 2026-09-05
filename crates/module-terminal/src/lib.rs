//! Canonical Terminal UI, controller, and runtime ports.

mod command_view;
mod ports;
mod query;
mod renderer;
mod runtime;
mod session;
mod view;

pub use ports::{
    TerminalCommandResult, TerminalCommandRunnerPort, TerminalCommandsPort, TerminalPorts,
    TerminalSessionPort, TerminalSocket, TerminalTransportPort,
};
pub use query::TerminalQuery;
pub use view::{ProjectInitializerTerminal, TerminalView};
