//! Conversation event projection and stream lifetime, independent of chat layout.

use dioxus::prelude::*;

use crate::{AiActivity, AiConversation, AiEvent, AiMessage, AiMessageStatus, AiRole};

pub(crate) async fn consume_ai_events(
    mut events: Box<dyn crate::AiEventStream>,
    conversation_id: &str,
    mut conversation: Signal<AiConversation>,
    mut pending: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut list_refresh: Signal<u64>,
) {
    loop {
        if conversation.peek().id != conversation_id {
            return;
        }
        let next = events.receive().await;
        // Navigation may have selected another conversation while awaiting an event.
        // An old stream must never append messages or reset the new chat's busy state.
        if conversation.peek().id != conversation_id {
            return;
        }
        match next {
            Ok(Some(event)) => {
                let delta = matches!(
                    &event,
                    AiEvent::AssistantDelta { .. } | AiEvent::AssistantThinkingDelta { .. }
                );
                apply_event_to_conversation(&mut conversation.write(), &event);
                if delta {
                    dioxus_sdk_time::sleep(std::time::Duration::from_millis(16)).await;
                }
            }
            Ok(None) => {
                finish_inflight(&mut conversation.write(), AiMessageStatus::Stopped);
                *list_refresh.write() += 1;
                break;
            }
            Err(problem) => {
                conversation.with_mut(|conversation| {
                    finish_inflight(conversation, AiMessageStatus::Failed);
                    conversation.status_message.clone_from(&problem.message);
                });
                error.set(Some(problem.message));
                break;
            }
        }
    }
    pending.set(false);
}

fn finish_inflight(conversation: &mut AiConversation, status: AiMessageStatus) {
    conversation.running = false;
    conversation.pending_messages = 0;
    conversation.steering_queue.clear();
    conversation.follow_up_queue.clear();
    for message in &mut conversation.messages {
        if matches!(message.status, AiMessageStatus::Streaming | AiMessageStatus::Running) {
            message.status = status;
        }
    }
    for activity in &mut conversation.activity {
        if let AiActivity::Tool { status: current, .. } = activity
            && matches!(*current, AiMessageStatus::Streaming | AiMessageStatus::Running)
        {
            *current = status;
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "the exhaustive event projection is clearer as one auditable state transition"
)]
pub(crate) fn apply_event_to_conversation(conversation: &mut AiConversation, event: &AiEvent) {
    match event {
        AiEvent::UserMessage(message) => {
            if conversation.messages.iter().any(|item| item.id == message.id) {
                return;
            }
            if message.entry_id.is_some()
                && let Some(index) = conversation.messages.iter().rposition(|item| {
                    item.role == AiRole::User
                        && item.entry_id.is_none()
                        && item.content == message.content
                        && item.images == message.images
                })
            {
                let placeholder_id = conversation.messages[index].id.clone();
                conversation.messages[index] = message.clone();
                if let Some(order_index) = conversation.item_order.iter().position(|id| id == &placeholder_id) {
                    conversation.item_order[order_index].clone_from(&message.id);
                }
            } else {
                conversation.messages.push(message.clone());
                conversation.item_order.push(message.id.clone());
            }
        }
        AiEvent::AssistantDelta { message_id, text } => {
            if let Some(message) = conversation.messages.iter_mut().find(|item| item.id == *message_id) {
                message.content.push_str(text);
            } else {
                conversation.messages.push(AiMessage {
                    id: message_id.clone(),
                    role: AiRole::Assistant,
                    content: text.clone(),
                    status: AiMessageStatus::Streaming,
                    ..AiMessage::default()
                });
                conversation.item_order.push(message_id.clone());
            }
        }
        AiEvent::AssistantThinkingDelta { message_id, text } => {
            if let Some(message) = conversation.messages.iter_mut().find(|item| item.id == *message_id) {
                message.thinking.push_str(text);
            } else {
                conversation.messages.push(AiMessage {
                    id: message_id.clone(),
                    role: AiRole::Assistant,
                    thinking: text.clone(),
                    status: AiMessageStatus::Streaming,
                    ..AiMessage::default()
                });
                conversation.item_order.push(message_id.clone());
            }
        }
        AiEvent::AssistantCompleted(message) => {
            if let Some(existing) = conversation.messages.iter_mut().find(|item| item.id == message.id) {
                *existing = message.clone();
            } else {
                conversation.messages.push(message.clone());
                conversation.item_order.push(message.id.clone());
            }
        }
        AiEvent::UsageUpdated { input_tokens, output_tokens } => {
            let usage = conversation.usage.get_or_insert_default();
            usage.total_tokens = input_tokens.saturating_add(*output_tokens);
        }
        AiEvent::ToolStarted { id, name } => upsert_tool(
            conversation, id, name, String::new(), AiMessageStatus::Running,
        ),
        AiEvent::ToolUpdated { id, output } => upsert_tool(
            conversation, id, "Tool", output.clone(), AiMessageStatus::Running,
        ),
        AiEvent::ToolCompleted { id, output } => upsert_tool(
            conversation, id, "Tool", output.clone(), AiMessageStatus::Complete,
        ),
        AiEvent::ActivityUpdated(activity) => {
            if let Some(existing) = conversation.activity.iter_mut().find(|existing| activity_id(existing) == activity_id(activity)) {
                *existing = activity.clone();
            } else {
                conversation.activity.push(activity.clone());
                conversation.item_order.push(activity_id(activity).to_owned());
            }
        }
        AiEvent::StatusChanged { running, message, pending_messages } => {
            conversation.running = *running;
            conversation.status_message.clone_from(message);
            conversation.pending_messages = *pending_messages;
        }
        AiEvent::QueueChanged { steering, follow_up } => {
            conversation.steering_queue.clone_from(steering);
            conversation.follow_up_queue.clone_from(follow_up);
            conversation.pending_messages = steering.len().saturating_add(follow_up.len());
        }
        AiEvent::ExtensionRequested(request) => {
            conversation.extension_request = Some(request.clone());
        }
        AiEvent::ExtensionSurfaces { title, statuses, widgets } => {
            conversation.extension_title.clone_from(title);
            conversation.extension_statuses.clone_from(statuses);
            conversation.extension_widgets.clone_from(widgets);
        }
        AiEvent::ComposerText(text) => {
            conversation.requested_composer_text = Some(text.clone());
        }
        AiEvent::Failed { message } => {
            let id = format!("failure-{}", conversation.item_order.len());
            conversation.activity.push(AiActivity::Notice {
                id: id.clone(),
                text: message.clone(),
                status: AiMessageStatus::Failed,
            });
            conversation.item_order.push(id);
        }
    }
}

pub(crate) fn activity_id(item: &AiActivity) -> &str {
    match item {
        AiActivity::Tool { id, .. }
        | AiActivity::Custom { id, .. }
        | AiActivity::Notice { id, .. } => id,
    }
}

fn upsert_tool(
    conversation: &mut AiConversation,
    id: &str,
    fallback_name: &str,
    output: String,
    status: AiMessageStatus,
) {
    if let Some(AiActivity::Tool { name, output: current_output, status: current_status, .. }) =
        conversation.activity.iter_mut().find(|activity| activity_id(activity) == id)
    {
        if name == "Tool" && fallback_name != "Tool" {
            *name = fallback_name.into();
        }
        *current_output = output;
        *current_status = status;
        return;
    }
    conversation.activity.push(AiActivity::Tool {
        id: id.into(),
        name: fallback_name.into(),
        summary: String::new(),
        output,
        args: None,
        details: None,
        args_truncated: false,
        details_truncated: false,
        status,
    });
    conversation.item_order.push(id.into());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnected_stream_finishes_partial_messages_without_changing_completed_ones() {
        let mut conversation = AiConversation {
            running: true,
            pending_messages: 1,
            steering_queue: vec!["queued".into()],
            ..AiConversation::default()
        };
        for (id, status) in [("done", AiMessageStatus::Complete), ("partial", AiMessageStatus::Streaming)] {
            conversation.messages.push(AiMessage { id: id.into(), status, ..AiMessage::default() });
        }
        upsert_tool(&mut conversation, "tool", "bash", String::new(), AiMessageStatus::Running);
        finish_inflight(&mut conversation, AiMessageStatus::Failed);
        assert!(!conversation.running);
        assert_eq!(conversation.pending_messages, 0);
        assert!(conversation.steering_queue.is_empty());
        assert_eq!(conversation.messages[0].status, AiMessageStatus::Complete);
        assert_eq!(conversation.messages[1].status, AiMessageStatus::Failed);
        assert!(matches!(conversation.activity[0], AiActivity::Tool { status: AiMessageStatus::Failed, .. }));
    }
}
