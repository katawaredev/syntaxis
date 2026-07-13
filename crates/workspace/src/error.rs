use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

pub type WorkspaceResult<T> = Result<T, WorkspaceError>;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidPath,
    OutsideAllowedRoot,
    NotFound,
    AlreadyExists,
    PermissionDenied,
    Conflict,
    TooLarge,
    UnsupportedEncoding,
    RootOperationRejected,
    Unavailable,
    Internal,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceError {
    pub code: ErrorCode,
    pub message: String,
}

impl WorkspaceError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_path(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidPath, message)
    }

    pub fn internal() -> Self {
        Self::new(
            ErrorCode::Internal,
            "The workspace operation could not be completed.",
        )
    }
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for WorkspaceError {}
