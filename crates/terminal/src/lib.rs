//! Platform-neutral terminal protocol and operation models.
mod protocol;
pub use protocol::{
    ClientMessage, Lifecycle, MAX_INPUT_BYTES, MAX_SESSION_ID_BYTES, MAX_SESSION_NAME_BYTES,
    PROTOCOL_VERSION, ServerMessage, SessionId, SessionSummary, TerminalError, TerminalErrorCode,
    TerminalSize,
};
