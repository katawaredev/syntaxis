use dioxus::prelude::*;
use syntaxis_terminal::{SessionId, SessionSummary, TerminalSize};

pub(super) fn choose_active(
    sessions: &[SessionSummary],
    requested: Option<&SessionId>,
    active: Option<&SessionId>,
    remembered: Option<&SessionId>,
) -> Option<SessionId> {
    requested
        .and_then(|id| sessions.iter().find(|session| &session.id == id))
        .or_else(|| {
            let id = active?;
            sessions.iter().find(|session| &session.id == id)
        })
        .or_else(|| {
            let id = remembered?;
            sessions.iter().find(|session| &session.id == id)
        })
        .or_else(|| sessions.first())
        .map(|session| session.id.clone())
}

pub(super) fn duplicate_session_name_error(
    requested_name: &str,
    sessions: &[SessionSummary],
) -> Option<String> {
    let requested_name = requested_name.trim();
    (!requested_name.is_empty()
        && sessions
            .iter()
            .any(|session| session.name.eq_ignore_ascii_case(requested_name)))
    .then(|| "Name already in use.".into())
}

pub(super) fn upsert_session(
    sessions: &mut Signal<Vec<SessionSummary>>,
    replacement: SessionSummary,
) {
    let mut sessions = sessions.write();
    if let Some(session) = sessions
        .iter_mut()
        .find(|session| session.id == replacement.id)
    {
        *session = replacement;
    } else {
        sessions.push(replacement);
    }
}

pub(super) fn remove_session(
    sessions: &mut Signal<Vec<SessionSummary>>,
    active: &mut Signal<Option<SessionId>>,
    removing: &SessionId,
) {
    let mut current = sessions();
    let next = close_session(&mut current, active().as_ref(), removing);
    sessions.set(current);
    active.set(next);
}

fn close_session(
    sessions: &mut Vec<SessionSummary>,
    active: Option<&SessionId>,
    closing_id: &SessionId,
) -> Option<SessionId> {
    let Some(index) = sessions
        .iter()
        .position(|session| &session.id == closing_id)
    else {
        return active.cloned();
    };
    let closing_active = active == Some(closing_id);
    sessions.remove(index);
    if !closing_active {
        return active.cloned();
    }
    sessions
        .get(index.min(sessions.len().saturating_sub(1)))
        .map(|session| session.id.clone())
}

pub(super) fn update_session_size(
    sessions: &mut Signal<Vec<SessionSummary>>,
    session_id: &SessionId,
    size: TerminalSize,
) {
    if let Some(session) = sessions
        .write()
        .iter_mut()
        .find(|session| &session.id == session_id)
    {
        session.size = size;
    }
}

#[cfg(test)]
mod tests {
    use syntaxis_terminal::Lifecycle;

    use super::*;

    fn session(id: &str) -> SessionSummary {
        SessionSummary {
            id: SessionId::new(id),
            name: id.into(),
            lifecycle: Lifecycle::Running,
            size: TerminalSize::DEFAULT,
            exit_code: None,
        }
    }

    #[test]
    fn closing_active_session_prefers_the_right_neighbor() {
        let mut sessions = vec![session("1"), session("2"), session("3")];
        assert_eq!(
            close_session(
                &mut sessions,
                Some(&SessionId::new("2")),
                &SessionId::new("2"),
            ),
            Some(SessionId::new("3")),
        );
    }

    #[test]
    fn closing_inactive_session_preserves_active_id() {
        let mut sessions = vec![session("1"), session("2")];
        assert_eq!(
            close_session(
                &mut sessions,
                Some(&SessionId::new("2")),
                &SessionId::new("1"),
            ),
            Some(SessionId::new("2")),
        );
    }

    #[test]
    fn remembered_session_wins_when_active_is_missing() {
        let sessions = vec![session("1"), session("2")];
        assert_eq!(
            choose_active(&sessions, None, None, Some(&SessionId::new("2"))),
            Some(SessionId::new("2")),
        );
    }

    #[test]
    fn requested_session_wins_over_remembered_session() {
        let sessions = vec![session("1"), session("2")];
        assert_eq!(
            choose_active(
                &sessions,
                Some(&SessionId::new("1")),
                None,
                Some(&SessionId::new("2")),
            ),
            Some(SessionId::new("1")),
        );
    }

    #[test]
    fn duplicate_session_names_are_rejected_case_insensitively() {
        let sessions = vec![session("shell 1")];
        assert_eq!(
            duplicate_session_name_error("  SHELL 1  ", &sessions),
            Some("Name already in use.".into()),
        );
        assert_eq!(duplicate_session_name_error("shell 2", &sessions), None);
        assert_eq!(duplicate_session_name_error("", &sessions), None);
    }
}
