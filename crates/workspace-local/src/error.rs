use rusqlite::ErrorCode as SqliteErrorCode;
use syntaxis_workspace::{ErrorCode, WorkspaceError};

pub(crate) fn map_database_error(error: rusqlite::Error) -> WorkspaceError {
    let constraint = matches!(
        error.sqlite_error_code(),
        Some(SqliteErrorCode::ConstraintViolation)
    );
    let not_found = matches!(error, rusqlite::Error::QueryReturnedNoRows);
    drop(error);
    if constraint {
        WorkspaceError::new(
            ErrorCode::AlreadyExists,
            "That workspace is already registered.",
        )
    } else if not_found {
        WorkspaceError::new(ErrorCode::NotFound, "The workspace was not found.")
    } else {
        WorkspaceError::internal()
    }
}

pub(crate) fn map_io_error(error: std::io::Error) -> WorkspaceError {
    let kind = error.kind();
    drop(error);
    let (code, message) = match kind {
        std::io::ErrorKind::NotFound => (ErrorCode::NotFound, "The requested path was not found."),
        std::io::ErrorKind::PermissionDenied => (
            ErrorCode::PermissionDenied,
            "The runtime does not have permission to access that path.",
        ),
        std::io::ErrorKind::AlreadyExists => (
            ErrorCode::AlreadyExists,
            "An entry already exists at that path.",
        ),
        _ => return WorkspaceError::internal(),
    };
    WorkspaceError::new(code, message)
}
