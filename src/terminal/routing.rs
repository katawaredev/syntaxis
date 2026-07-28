use std::fmt;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalQuery {
    pub(super) session_id: Option<String>,
}

impl TerminalQuery {
    pub(crate) fn with_session(session_id: String) -> Self {
        Self {
            session_id: Some(session_id),
        }
    }
}

impl From<&str> for TerminalQuery {
    fn from(query: &str) -> Self {
        let session_id = url::form_urlencoded::parse(query.as_bytes()).find_map(|(key, value)| {
            matches!(key.as_ref(), "sessionId" | "session_id")
                .then(|| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        });
        Self { session_id }
    }
}

impl fmt::Display for TerminalQuery {
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
    fn links_round_trip_through_the_router() {
        let route = crate::app::Route::Terminal {
            slug: "syntaxis-demo".into(),
            query: TerminalQuery::with_session("session/with spaces".into()),
        };
        let link = route.to_string();
        assert_eq!(
            link,
            "/workspaces/syntaxis-demo/terminal?sessionId=session%2Fwith+spaces"
        );
        assert_eq!(link.parse::<crate::app::Route>().unwrap(), route);
    }
}
