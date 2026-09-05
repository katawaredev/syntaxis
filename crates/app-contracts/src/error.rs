use std::fmt;

use serde::{Deserialize, Serialize};
use syntaxis_workspace::{ErrorCode as WorkspaceErrorCode, WorkspaceError};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppErrorCode {
    Unsupported,
    InvalidInput,
    NotFound,
    Conflict,
    PermissionDenied,
    TooLarge,
    Offline,
    Cancelled,
    RateLimited,
    Internal,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryAdvice {
    Never,
    Immediate,
    Backoff,
    AfterUserAction,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSource {
    Application,
    Workspace,
    Files,
    Terminal,
    Git,
    Preview,
    Ai,
    Runtime,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppError {
    pub code: AppErrorCode,
    pub message: String,
    pub retry: RetryAdvice,
    pub source: ErrorSource,
}

impl AppError {
    pub fn new(
        code: AppErrorCode,
        message: impl Into<String>,
        retry: RetryAdvice,
        source: ErrorSource,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            retry,
            source,
        }
    }

    pub fn unsupported(message: impl Into<String>, source: ErrorSource) -> Self {
        Self::new(
            AppErrorCode::Unsupported,
            message,
            RetryAdvice::Never,
            source,
        )
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

impl From<WorkspaceError> for AppError {
    fn from(error: WorkspaceError) -> Self {
        let (code, retry) = match error.code {
            WorkspaceErrorCode::InvalidPath | WorkspaceErrorCode::UnsupportedEncoding => {
                (AppErrorCode::InvalidInput, RetryAdvice::Never)
            }
            WorkspaceErrorCode::OutsideAllowedRoot
            | WorkspaceErrorCode::PermissionDenied
            | WorkspaceErrorCode::RootOperationRejected => {
                (AppErrorCode::PermissionDenied, RetryAdvice::AfterUserAction)
            }
            WorkspaceErrorCode::NotFound => (AppErrorCode::NotFound, RetryAdvice::Never),
            WorkspaceErrorCode::AlreadyExists | WorkspaceErrorCode::Conflict => {
                (AppErrorCode::Conflict, RetryAdvice::AfterUserAction)
            }
            WorkspaceErrorCode::TooLarge => (AppErrorCode::TooLarge, RetryAdvice::Never),
            WorkspaceErrorCode::Unavailable => (AppErrorCode::Offline, RetryAdvice::Backoff),
            WorkspaceErrorCode::Internal => (AppErrorCode::Internal, RetryAdvice::Backoff),
        };
        Self::new(code, error.message, retry, ErrorSource::Workspace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_errors_map_without_matching_message_text() {
        let mapped = AppError::from(WorkspaceError::new(
            WorkspaceErrorCode::Conflict,
            "the message remains presentational",
        ));
        assert_eq!(mapped.code, AppErrorCode::Conflict);
        assert_eq!(mapped.retry, RetryAdvice::AfterUserAction);
        assert_eq!(mapped.source, ErrorSource::Workspace);
    }
}
