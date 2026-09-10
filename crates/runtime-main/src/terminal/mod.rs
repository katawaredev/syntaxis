mod adapter;
pub(crate) mod api;
mod ports;

pub use adapter::{MainTerminalCommands, MainTerminalCommandsTransport};
pub(crate) use ports::terminal_ports;
