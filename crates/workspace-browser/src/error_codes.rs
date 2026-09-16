use syntaxis_workspace::ErrorCode;

pub(crate) fn browser_error_code(name: &str) -> ErrorCode {
    match name {
        "NotFoundError" => ErrorCode::NotFound,
        "NotAllowedError" | "SecurityError" => ErrorCode::PermissionDenied,
        "QuotaExceededError" => ErrorCode::TooLarge,
        "TypeMismatchError" => ErrorCode::InvalidPath,
        "InvalidModificationError" => ErrorCode::Conflict,
        _ => ErrorCode::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_missing_files_are_distinct_from_permission_and_storage_failures() {
        assert_eq!(browser_error_code("NotFoundError"), ErrorCode::NotFound);
        assert_eq!(
            browser_error_code("NotAllowedError"),
            ErrorCode::PermissionDenied
        );
        assert_eq!(
            browser_error_code("SecurityError"),
            ErrorCode::PermissionDenied
        );
        assert_eq!(
            browser_error_code("QuotaExceededError"),
            ErrorCode::TooLarge
        );
        assert_eq!(browser_error_code("UnknownError"), ErrorCode::Unavailable);
    }
}
