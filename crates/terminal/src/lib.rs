//! Platform-neutral terminal protocol and operation models.
mod commands;
mod protocol;
pub use commands::{RunCommand, justfile_commands, makefile_commands, package_json_commands};
pub use protocol::{
    ClientMessage, Lifecycle, MAX_INPUT_BYTES, MAX_SESSION_ID_BYTES, MAX_SESSION_NAME_BYTES,
    PROTOCOL_VERSION, ServerMessage, SessionId, SessionSummary, TerminalError, TerminalErrorCode,
    TerminalSize,
};
