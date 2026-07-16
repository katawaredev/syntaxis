use syntaxis_workspace::{ErrorCode, WorkspaceError};

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
