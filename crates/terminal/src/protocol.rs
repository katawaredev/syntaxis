use serde::{Deserialize, Serialize};
/// Increment when a protocol change is not backward compatible.
pub const PROTOCOL_VERSION: u16 = 2;
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
pub const MAX_SESSION_ID_BYTES: usize = 128;
pub const MAX_SESSION_NAME_BYTES: usize = 256;
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SessionId(pub String);
impl SessionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn is_valid(&self) -> bool {
        !self.0.is_empty() && self.0.len() <= MAX_SESSION_ID_BYTES
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerminalSize {
    pub columns: u16,
    pub rows: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}
impl TerminalSize {
    pub const DEFAULT: Self = Self {
        columns: 80,
        rows: 24,
        pixel_width: 0,
        pixel_height: 0,
    };
    pub const fn is_valid(self) -> bool {
        self.columns > 0 && self.columns <= 1_000 && self.rows > 0 && self.rows <= 1_000
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Starting,
    Running,
    Exited,
    Failed,
    Closing,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionSummary {
    pub id: SessionId,
    pub name: String,
    pub lifecycle: Lifecycle,
    pub size: TerminalSize,
    pub exit_code: Option<u32>,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalErrorCode {
    InvalidProtocol,
    InvalidRequest,
    NotFound,
    PermissionDenied,
    Unavailable,
    OutputLagged,
    Internal,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerminalError {
    pub code: TerminalErrorCode,
    pub message: String,
    pub session_id: Option<SessionId>,
}
impl TerminalError {
    pub fn new(code: TerminalErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            session_id: None,
        }
    }
    #[must_use]
    pub fn for_session(mut self, session_id: SessionId) -> Self {
        self.session_id = Some(session_id);
        self
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Hello {
        version: u16,
    },
    List,
    Create {
        name: Option<String>,
        size: TerminalSize,
    },
    Attach {
        session_id: SessionId,
    },
    Detach {
        session_id: SessionId,
    },
    Write {
        session_id: SessionId,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
    Resize {
        session_id: SessionId,
        size: TerminalSize,
    },
    Close {
        session_id: SessionId,
    },
    CloseAll,
    Ping {
        nonce: u64,
    },
}
impl ClientMessage {
    /// Validate the semantic and allocation bounds of a decoded client message.
    ///
    /// # Errors
    ///
    /// Returns a safe protocol error when a field is invalid or exceeds its bound.
    pub fn validate(&self) -> Result<(), TerminalError> {
        match self {
            Self::Hello { version } if *version != PROTOCOL_VERSION => Err(TerminalError::new(
                TerminalErrorCode::InvalidProtocol,
                "Unsupported terminal protocol version",
            )),
            Self::Create { name, size } => {
                if name
                    .as_ref()
                    .is_some_and(|name| name.len() > MAX_SESSION_NAME_BYTES)
                {
                    return Err(TerminalError::new(
                        TerminalErrorCode::InvalidRequest,
                        "Terminal session name is too long",
                    ));
                }
                validate_size(*size)
            }
            Self::Attach { session_id }
            | Self::Detach { session_id }
            | Self::Close { session_id } => validate_session_id(session_id),
            Self::Write { session_id, data } => {
                validate_session_id(session_id)?;
                if data.is_empty() || data.len() > MAX_INPUT_BYTES {
                    return Err(TerminalError::new(
                        TerminalErrorCode::InvalidRequest,
                        "Terminal input must be between 1 byte and 64 KiB",
                    )
                    .for_session(session_id.clone()));
                }
                Ok(())
            }
            Self::Resize { session_id, size } => {
                validate_session_id(session_id)?;
                validate_size(*size)
            }
            Self::Hello { .. } | Self::List | Self::CloseAll | Self::Ping { .. } => Ok(()),
        }
    }
    /// Validate that this is the only message accepted before a socket is established.
    ///
    /// # Errors
    ///
    /// Returns an invalid-protocol error for missing or incompatible handshakes.
    pub fn validate_handshake(&self) -> Result<(), TerminalError> {
        match self {
            Self::Hello { .. } => self.validate(),
            Self::List
            | Self::Create { .. }
            | Self::Attach { .. }
            | Self::Detach { .. }
            | Self::Write { .. }
            | Self::Resize { .. }
            | Self::Close { .. }
            | Self::CloseAll
            | Self::Ping { .. } => Err(TerminalError::new(
                TerminalErrorCode::InvalidProtocol,
                "Terminal protocol handshake required",
            )),
        }
    }
}
fn validate_session_id(session_id: &SessionId) -> Result<(), TerminalError> {
    if session_id.is_valid() {
        Ok(())
    } else {
        Err(TerminalError::new(
            TerminalErrorCode::InvalidRequest,
            "Invalid terminal session identifier",
        ))
    }
}
fn validate_size(size: TerminalSize) -> Result<(), TerminalError> {
    if size.is_valid() {
        Ok(())
    } else {
        Err(TerminalError::new(
            TerminalErrorCode::InvalidRequest,
            "Invalid terminal dimensions",
        ))
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Hello {
        version: u16,
    },
    Sessions {
        sessions: Vec<SessionSummary>,
    },
    Created {
        session: SessionSummary,
    },
    Attached {
        session: SessionSummary,
    },
    Detached {
        session_id: SessionId,
    },
    Output {
        session_id: SessionId,
        sequence: u64,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
        replay: bool,
    },
    Lifecycle {
        session: SessionSummary,
    },
    Closed {
        session_id: SessionId,
    },
    Error {
        error: TerminalError,
    },
    Pong {
        nonce: u64,
    },
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_size_rejects_zero_and_unbounded_dimensions() {
        assert!(TerminalSize::DEFAULT.is_valid());
        assert!(!TerminalSize {
            columns: 0,
            ..TerminalSize::DEFAULT
        }
        .is_valid(),);
        assert!(!TerminalSize {
            rows: 1_001,
            ..TerminalSize::DEFAULT
        }
        .is_valid(),);
    }
    #[test]
    fn protocol_messages_use_stable_tagged_json() {
        let message = ClientMessage::Resize {
            session_id: SessionId::new("session"),
            size: TerminalSize::DEFAULT,
        };
        let json = serde_json::to_value(&message).expect("client message should serialize");
        assert_eq!(json["type"], "resize");
        assert_eq!(json["session_id"], "session");
        assert_eq!(
            serde_json::from_value::<ClientMessage>(json)
                .expect("serialized client message should deserialize"),
            message
        );
    }
    #[test]
    fn handshake_rejects_missing_and_incompatible_hello_messages() {
        let missing = ClientMessage::List
            .validate_handshake()
            .expect_err("non-handshake messages must be rejected");
        assert_eq!(missing.code, TerminalErrorCode::InvalidProtocol);
        let incompatible = ClientMessage::Hello {
            version: PROTOCOL_VERSION + 1,
        }
        .validate_handshake()
        .expect_err("incompatible protocol versions must be rejected");
        assert_eq!(incompatible.code, TerminalErrorCode::InvalidProtocol);
        ClientMessage::Hello {
            version: PROTOCOL_VERSION,
        }
        .validate_handshake()
        .expect("the current protocol version must be accepted");
    }
    #[test]
    fn validation_rejects_oversized_transport_fields() {
        let session_id = SessionId::new("s".repeat(MAX_SESSION_ID_BYTES + 1));
        assert_eq!(
            ClientMessage::Attach { session_id }
                .validate()
                .expect_err("oversized session identifiers must be rejected")
                .code,
            TerminalErrorCode::InvalidRequest,
        );
        let write = ClientMessage::Write {
            session_id: SessionId::new("session"),
            data: vec![0; MAX_INPUT_BYTES + 1],
        };
        assert_eq!(
            write
                .validate()
                .expect_err("oversized terminal input must be rejected")
                .code,
            TerminalErrorCode::InvalidRequest,
        );
        let create = ClientMessage::Create {
            name: Some("n".repeat(MAX_SESSION_NAME_BYTES + 1)),
            size: TerminalSize::DEFAULT,
        };
        assert_eq!(
            create
                .validate()
                .expect_err("oversized session names must be rejected")
                .code,
            TerminalErrorCode::InvalidRequest,
        );
    }
    #[test]
    fn malformed_and_unknown_messages_do_not_decode() {
        serde_json::from_str::<ClientMessage>(r#"{"type":"unknown"}"#)
            .expect_err("unknown message types must not deserialize");
        serde_json::from_str::<ClientMessage>(r#"{"type":"resize"}"#)
            .expect_err("messages with missing fields must not deserialize");
    }
}
