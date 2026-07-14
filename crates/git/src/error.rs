use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

pub type GitResult<T> = Result<T, GitError>;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitErrorCode {
    InvalidWorkspace,
    NotRepository,
    Authentication,
    SigningPassphraseRequired,
    NonFastForward,
    Conflict,
    TimedOut,
    Cancelled,
    OutputTooLarge,
    Unsupported,
    CommandFailed,
    Parse,
    Unavailable,
    Internal,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitError {
    pub code: GitErrorCode,
    pub message: String,
    pub exit_code: Option<i32>,
}

impl GitError {
    pub fn new(code: GitErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            exit_code: None,
        }
    }

    #[must_use]
    pub fn with_exit_code(mut self, exit_code: Option<i32>) -> Self {
        self.exit_code = exit_code;
        self
    }
}

impl fmt::Display for GitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for GitError {}
