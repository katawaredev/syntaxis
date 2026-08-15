use dioxus::prelude::ServerFnError;

/// Returns the actionable server message when one is available, falling back to
/// Dioxus' display representation for transport and serialization failures.
pub(crate) fn server_error_message(error: ServerFnError) -> String {
    match error {
        ServerFnError::ServerError { message, .. } => message,
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_actionable_server_message() {
        let error = ServerFnError::ServerError {
            message: "That branch already exists.".into(),
            code: 422,
            details: None,
        };

        assert_eq!(server_error_message(error), "That branch already exists.");
    }
}
