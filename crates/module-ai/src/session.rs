//! Workspace-scoped chat selection, independent of the conversation backend.

use std::collections::HashMap;

use dioxus::prelude::*;
use syntaxis_app_contracts::AppError;
use syntaxis_workspace::{WorkspaceId, WorkspaceRecord};

use crate::{AiClientPort, AiConversation, AiConversationPort, AiConversationSummary};

/// Keep navigation state above the router so leaving AI does not discard it.
#[derive(Clone, Copy)]
pub struct AiUiState {
    selected: Signal<HashMap<WorkspaceId, String>>,
}

pub fn use_ai_ui_state() -> AiUiState {
    AiUiState {
        selected: use_signal(HashMap::new),
    }
}

impl AiUiState {
    pub(crate) fn selected(self, workspace: &WorkspaceId) -> Option<String> {
        self.selected.peek().get(workspace).cloned()
    }

    pub(crate) fn remember(mut self, workspace: WorkspaceId, conversation_id: String) {
        self.selected.write().insert(workspace, conversation_id);
    }

    pub(crate) fn forget(mut self, workspace: &WorkspaceId, conversation_id: &str) {
        if self.selected(workspace).as_deref() == Some(conversation_id) {
            self.selected.write().remove(workspace);
        }
    }
}

pub(crate) fn selection_key(workspace: &WorkspaceId) -> String {
    format!("syntaxis:ai-selected:{}", workspace.0)
}

pub(crate) async fn restore_conversation(
    port: &dyn AiConversationPort,
    client: Option<&dyn AiClientPort>,
    workspace: &WorkspaceRecord,
    requested: Option<&str>,
    remembered: Option<String>,
) -> Result<AiConversation, AppError> {
    // A failed listing must not be mistaken for an empty workspace.
    let items = port.list(workspace).await?;
    let remembered = match (remembered, client) {
        (Some(id), _) => Some(id),
        (None, Some(client)) => client
            .load_state(&selection_key(&workspace.id))
            .await
            .ok()
            .flatten(),
        (None, None) => None,
    };
    match choose_conversation(&items, requested, remembered.as_deref()) {
        Some(id) => port.open(workspace, &id).await,
        None => port.create(workspace).await,
    }
}

fn choose_conversation(
    items: &[AiConversationSummary],
    requested: Option<&str>,
    remembered: Option<&str>,
) -> Option<String> {
    requested
        .and_then(|id| items.iter().find(|item| item.id == id))
        .or_else(|| remembered.and_then(|id| items.iter().find(|item| item.id == id)))
        .or_else(|| items.first())
        .map(|item| item.id.clone())
}

pub(crate) fn selection_after_delete(
    items: &[AiConversationSummary],
    active: &str,
    deleted: &str,
) -> Option<String> {
    if active != deleted {
        return Some(active.to_owned());
    }
    let index = items.iter().position(|item| item.id == deleted)?;
    items
        .get(index + 1)
        .or_else(|| index.checked_sub(1).and_then(|index| items.get(index)))
        .map(|item| item.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chats() -> Vec<AiConversationSummary> {
        ["first", "second", "third"]
            .into_iter()
            .map(|id| AiConversationSummary {
                id: id.into(),
                title: id.into(),
                message_count: 0,
                updated_at_ms: 0,
                status_message: String::new(),
                running: false,
            })
            .collect()
    }

    #[test]
    fn returning_restores_the_remembered_chat() {
        assert_eq!(
            choose_conversation(&chats(), None, Some("second")),
            Some("second".into()),
        );
    }

    #[test]
    fn explicit_chat_links_override_the_remembered_chat() {
        assert_eq!(
            choose_conversation(&chats(), Some("third"), Some("second")),
            Some("third".into()),
        );
    }

    #[test]
    fn stale_selections_fall_back_to_an_existing_chat() {
        assert_eq!(
            choose_conversation(&chats(), Some("deleted"), Some("second")),
            Some("second".into()),
        );
        assert_eq!(
            choose_conversation(&chats(), None, Some("deleted")),
            Some("first".into()),
        );
    }

    #[test]
    fn only_an_empty_list_needs_a_new_chat() {
        assert_eq!(
            choose_conversation(&chats(), None, None),
            Some("first".into()),
        );
        assert_eq!(
            choose_conversation(&[], Some("deleted"), Some("deleted")),
            None,
        );
    }

    #[test]
    fn deleting_the_active_chat_selects_its_next_neighbor() {
        assert_eq!(
            selection_after_delete(&chats(), "first", "first"),
            Some("second".into()),
        );
        assert_eq!(
            selection_after_delete(&chats(), "second", "second"),
            Some("third".into()),
        );
        assert_eq!(
            selection_after_delete(&chats(), "third", "third"),
            Some("second".into()),
        );
    }

    #[test]
    fn deleting_an_inactive_chat_preserves_selection() {
        assert_eq!(
            selection_after_delete(&chats(), "second", "first"),
            Some("second".into()),
        );
    }

    #[test]
    fn deleting_the_last_chat_leaves_no_selection() {
        assert_eq!(
            selection_after_delete(&chats()[..1], "first", "first"),
            None,
        );
    }

    #[test]
    fn selection_storage_is_workspace_scoped() {
        assert_ne!(
            selection_key(&WorkspaceId("one".into())),
            selection_key(&WorkspaceId("two".into())),
        );
    }
}
