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
            snapshot.set(sanitize_snapshot(next));
        }
        ServerMessage::SessionEvent { session_id, event } => {
            if selected_id().as_deref() == Some(session_id.as_str()) {
                apply_agent_event(*event, snapshot, draft, error, extension_request);
            }
        }
        event => apply_agent_event(event, snapshot, draft, error, extension_request),
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "keeping server event projection exhaustive in one match makes client state updates auditable"
)]
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
            snapshot.set(sanitize_snapshot(next));
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
            if pending_messages == 0 {
                state.steering_queue.clear();
                state.follow_up_queue.clear();
            }
        }
        ServerMessage::QueueChanged {
            steering,
            follow_up,
        } => {
            let mut state = snapshot.write();
            state.pending_messages = steering.len().saturating_add(follow_up.len());
            state.steering_queue = steering;
            state.follow_up_queue = follow_up;
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
        ServerMessage::ExtensionSurfaces {
            title,
            statuses,
            widgets,
        } => {
            let mut state = snapshot.write();
            state.extension_title = title;
            state.extension_statuses = statuses;
            state.extension_widgets = widgets;
            sanitize_extension_surfaces(&mut state);
        }
        ServerMessage::ComposerText { text } => draft.set(text),
        ServerMessage::ExportReady {
            filename,
            data_base64,
        } => download_export(filename, data_base64),
        ServerMessage::Error { error: agent_error } => error.set(Some(agent_error.message)),
        ServerMessage::Hello { .. }
        | ServerMessage::Sessions { .. }
        | ServerMessage::SelectedSession { .. }
        | ServerMessage::SessionEvent { .. }
        | ServerMessage::Pong { .. } => {}
    }
}

fn sanitize_snapshot(mut snapshot: AgentSnapshot) -> AgentSnapshot {
    sanitize_extension_surfaces(&mut snapshot);
    snapshot
}

fn sanitize_extension_surfaces(snapshot: &mut AgentSnapshot) {
    if let Some(title) = &mut snapshot.extension_title {
        *title = strip_ansi(title);
    }
    for (_, text) in &mut snapshot.extension_statuses {
        *text = strip_ansi(text);
    }
    for widget in &mut snapshot.extension_widgets {
        for line in &mut widget.lines {
            *line = strip_ansi(line);
        }
    }
}

fn strip_ansi(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            output.push(character);
            continue;
        }
        match characters.next() {
            Some('[') => {
                for code in characters.by_ref() {
                    if ('@'..='~').contains(&code) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(code) = characters.next() {
                    if code == '\u{7}' {
                        break;
                    }
                    if code == '\u{1b}' && characters.next_if_eq(&'\\').is_some() {
                        break;
                    }
                }
            }
            Some(_) | None => {}
        }
    }
    output
}

fn download_export(filename: String, data_base64: String) {
    spawn(async move {
        let script = document::eval(
            r#"
            const filename = await dioxus.recv();
            const encoded = await dioxus.recv();
            const binary = atob(encoded);
            const bytes = new Uint8Array(binary.length);
            for (let index = 0; index < binary.length; index++) bytes[index] = binary.charCodeAt(index);
            const url = URL.createObjectURL(new Blob([bytes], { type: "text/html" }));
            const link = document.createElement("a");
            link.href = url;
            link.download = filename;
            link.style.display = "none";
            document.body.appendChild(link);
            link.click();
            link.remove();
            setTimeout(() => URL.revokeObjectURL(url), 1000);
            "#,
        );
        let _ = script.send(filename);
        let _ = script.send(data_base64);
    });
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
    fn extension_surface_text_strips_ansi_control_sequences() {
        let snapshot = AgentSnapshot {
            extension_title: Some("\u{1b}[1mTools\u{1b}[22m".into()),
            extension_statuses: vec![(
                "lsp".into(),
                "\u{1b}[38;5;241mLSP Inactive\u{1b}[39m".into(),
            )],
            ..AgentSnapshot::default()
        };
        let snapshot = sanitize_snapshot(snapshot);
        assert_eq!(snapshot.extension_title.as_deref(), Some("Tools"));
        assert_eq!(snapshot.extension_statuses[0].1, "LSP Inactive");
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
