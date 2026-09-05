use std::fmt;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AiQuery {
    pub session_id: Option<String>,
}

impl AiQuery {
    pub fn with_session(session_id: String) -> Self {
        Self {
            session_id: Some(session_id),
        }
    }
}

impl From<&str> for AiQuery {
    fn from(query: &str) -> Self {
        let session_id = url::form_urlencoded::parse(query.as_bytes()).find_map(|(key, value)| {
            matches!(key.as_ref(), "sessionId" | "session_id")
                .then(|| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        });
        Self { session_id }
    }
}

impl fmt::Display for AiQuery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        if let Some(session_id) = self.session_id.as_deref() {
            serializer.append_pair("sessionId", session_id);
        }
        formatter.write_str(&serializer.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_queries_accept_legacy_snake_case() {
        assert_eq!(
            AiQuery::from("session_id=conversation-1")
                .session_id
                .as_deref(),
            Some("conversation-1")
        );
    }
}
