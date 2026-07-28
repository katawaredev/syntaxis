use dioxus::prelude::*;
use syntaxis_agent::{
    AgentSessionSummary, AgentSnapshot, ChatItem, ClientMessage, ExtensionUiRequest, ServerMessage,
};

pub(super) fn apply_server_message(
    message: ServerMessage,
    sessions: &mut Signal<Vec<AgentSessionSummary>>,
    selected_id: &mut Signal<Option<String>>,
    snapshot: &mut Signal<AgentSnapshot>,
    draft: &mut Signal<String>,
    error: &mut Signal<Option<String>>,
    extension_request: &mut Signal<Option<ExtensionUiRequest>>,
) {
    match message {
        ServerMessage::Sessions { sessions: next } => sessions.set(next),
        ServerMessage::SelectedSession {
            session_id,
            snapshot: next,
        } => {
            selected_id.set(Some(session_id));
            extension_request.set(next.pending_extension_request.clone());
            snapshot.set(next);
        }
        ServerMessage::SessionEvent { session_id, event } => {
            if selected_id().as_deref() == Some(session_id.as_str()) {
                apply_agent_event(*event, snapshot, draft, error, extension_request);
            }
        }
        event => apply_agent_event(event, snapshot, draft, error, extension_request),
    }
}

fn apply_agent_event(
    message: ServerMessage,
    snapshot: &mut Signal<AgentSnapshot>,
    draft: &mut Signal<String>,
    error: &mut Signal<Option<String>>,
    extension_request: &mut Signal<Option<ExtensionUiRequest>>,
) {
    match message {
        ServerMessage::Snapshot { snapshot: next } => {
            extension_request.set(next.pending_extension_request.clone());
            snapshot.set(next);
        }
        ServerMessage::ItemAdded { item } => snapshot.write().items.push(item),
        ServerMessage::ItemDelta {
            item_id,
            text,
            thinking,
        } => {
            if let Some(ChatItem::Assistant {
                text: content,
                thinking: reasoning,
                ..
            }) = snapshot
                .write()
                .items
                .iter_mut()
                .find(|item| item.id() == item_id)
            {
                if thinking {
                    reasoning.push_str(&text);
                } else {
                    content.push_str(&text);
                }
            }
        }
        ServerMessage::ItemUpdated { item } => {
            let mut state = snapshot.write();
            if let Some(existing) = state
                .items
                .iter_mut()
                .find(|existing| existing.id() == item.id())
            {
                *existing = item;
            } else {
                state.items.push(item);
            }
        }
        ServerMessage::Status {
            status,
            message,
            pending_messages,
        } => {
            let mut state = snapshot.write();
            state.status = status;
            state.status_message = message;
            state.pending_messages = pending_messages;
        }
        ServerMessage::SessionChanged {
            session_id,
            session_name,
        } => {
            let mut state = snapshot.write();
            state.session_id = session_id;
            state.session_name = session_name;
        }
        ServerMessage::ModelChanged {
            model,
            thinking_level,
        } => {
            let mut state = snapshot.write();
            state.model = model;
            state.thinking_level = thinking_level;
        }
        ServerMessage::Models { models } => snapshot.write().models = models,
        ServerMessage::Commands { commands } => snapshot.write().commands = commands,
        ServerMessage::SessionStats { stats } => snapshot.write().session_stats = Some(stats),
        ServerMessage::ExtensionUiRequest { request } => extension_request.set(Some(request)),
        ServerMessage::ComposerText { text } => draft.set(text),
        ServerMessage::Error { error: agent_error } => error.set(Some(agent_error.message)),
        ServerMessage::Hello { .. }
        | ServerMessage::Sessions { .. }
        | ServerMessage::SelectedSession { .. }
        | ServerMessage::SessionEvent { .. }
        | ServerMessage::Pong { .. } => {}
    }
}

pub(super) fn session_action(session_id: String, action: ClientMessage) -> ClientMessage {
    ClientMessage::SessionAction {
        session_id,
        action: Box::new(action),
    }
}

pub(super) fn initial_session_request(
    available: &[AgentSessionSummary],
    selected_id: Option<String>,
    force_new: bool,
) -> Option<ClientMessage> {
    if force_new {
        return None;
    }
    selected_id
        .filter(|id| available.iter().any(|session| session.id == *id))
        .or_else(|| available.first().map(|session| session.id.clone()))
        .map(|session_id| ClientMessage::SelectSession { session_id })
}

pub(super) fn pending_session_request(available: &[AgentSessionSummary]) -> ClientMessage {
    available
        .iter()
        .find(|session| session.title == "New chat")
        .map_or(ClientMessage::CreateSession, |session| {
            ClientMessage::SelectSession {
                session_id: session.id.clone(),
            }
        })
}

#[cfg(test)]
mod tests {
    use syntaxis_agent::AgentStatus;

    use super::*;

    fn session(id: &str) -> AgentSessionSummary {
        AgentSessionSummary {
            id: id.into(),
            title: id.into(),
            updated_at_ms: 0,
            status: AgentStatus::Ready,
            status_message: "Ready".into(),
            running: false,
        }
    }

    #[test]
    fn isolated_handoff_starts_an_unpersisted_draft() {
        let available = vec![session("saved")];
        assert_eq!(
            initial_session_request(&available, Some("saved".into()), true),
            None,
        );
    }

    #[test]
    fn ordinary_connection_resumes_the_selected_or_first_session() {
        let available = vec![session("first"), session("selected")];
        assert_eq!(
            initial_session_request(&available, Some("selected".into()), false),
            Some(ClientMessage::SelectSession {
                session_id: "selected".into(),
            }),
        );
        assert_eq!(
            initial_session_request(&available, Some("missing".into()), false),
            Some(ClientMessage::SelectSession {
                session_id: "first".into(),
            }),
        );
    }

    #[test]
    fn empty_session_list_starts_an_unpersisted_draft() {
        assert_eq!(initial_session_request(&[], None, false), None);
    }

    #[test]
    fn submitted_draft_resumes_an_unfinished_session_or_creates_one() {
        let available = vec![session("saved"), session("New chat")];
        assert_eq!(
            pending_session_request(&available),
            ClientMessage::SelectSession {
                session_id: "New chat".into(),
            }
        );
        assert_eq!(pending_session_request(&[]), ClientMessage::CreateSession);
    }
}
