use super::*;

#[test]
fn failed_tool_retains_its_name_and_failure_status() {
    let mut conversation = AiConversation::default();
    apply_event_to_conversation(
        &mut conversation,
        &AiEvent::ToolStarted {
            id: "read-1".into(),
            name: "read".into(),
        },
    );
    apply_event_to_conversation(
        &mut conversation,
        &AiEvent::ToolFailed {
            id: "read-1".into(),
            output: "File missing".into(),
            duration_ms: Some(1234),
        },
    );
    assert!(
        matches!(&conversation.activity[0], AiActivity::Tool { name, status: AiMessageStatus::Failed, output, duration_ms: Some(1234), .. } if name == "read" && output == "File missing")
    );
}

#[test]
fn reconnect_snapshot_replaces_stale_running_state_and_model() {
    let mut conversation = AiConversation {
        id: "chat".into(),
        running: true,
        ..AiConversation::default()
    };
    let refreshed = AiConversation {
        id: "chat".into(),
        selected_model_id: Some("provider/model".into()),
        status_message: "Ready".into(),
        ..AiConversation::default()
    };
    apply_event_to_conversation(
        &mut conversation,
        &AiEvent::ConversationUpdated(Box::new(refreshed.clone())),
    );
    assert_eq!(conversation, refreshed);
}

#[test]
fn disconnected_stream_finishes_partial_messages_without_changing_completed_ones() {
    let mut conversation = AiConversation {
        running: true,
        pending_messages: 1,
        steering_queue: vec!["queued".into()],
        ..AiConversation::default()
    };
    for (id, status) in [
        ("done", AiMessageStatus::Complete),
        ("partial", AiMessageStatus::Streaming),
    ] {
        conversation.messages.push(AiMessage {
            id: id.into(),
            status,
            ..AiMessage::default()
        });
    }
    upsert_tool(
        &mut conversation,
        "tool",
        "bash",
        String::new(),
        AiMessageStatus::Running,
        None,
    );
    finish_inflight(&mut conversation, AiMessageStatus::Failed);
    assert!(!conversation.running);
    assert_eq!(conversation.pending_messages, 0);
    assert!(conversation.steering_queue.is_empty());
    assert_eq!(conversation.messages[0].status, AiMessageStatus::Complete);
    assert_eq!(conversation.messages[1].status, AiMessageStatus::Failed);
    assert!(matches!(
        conversation.activity[0],
        AiActivity::Tool {
            status: AiMessageStatus::Failed,
            ..
        }
    ));
}

#[test]
fn streamed_deltas_are_reconciled_with_the_completed_message() {
    let mut conversation = AiConversation::default();
    apply_event_to_conversation(
        &mut conversation,
        &AiEvent::AssistantDelta {
            message_id: "assistant-1".into(),
            text: "hel".into(),
        },
    );
    apply_event_to_conversation(
        &mut conversation,
        &AiEvent::AssistantDelta {
            message_id: "assistant-1".into(),
            text: "lo".into(),
        },
    );
    apply_event_to_conversation(
        &mut conversation,
        &AiEvent::AssistantCompleted(AiMessage {
            id: "assistant-1".into(),
            role: AiRole::Assistant,
            content: "hello!".into(),
            ..AiMessage::default()
        }),
    );

    assert_eq!(conversation.messages.len(), 1);
    assert_eq!(conversation.messages[0].content, "hello!");
}

#[test]
fn repeated_user_events_are_idempotent() {
    let mut conversation = AiConversation::default();
    let event = AiEvent::UserMessage(AiMessage {
        id: "user-1".into(),
        role: AiRole::User,
        content: "question".into(),
        ..AiMessage::default()
    });
    apply_event_to_conversation(&mut conversation, &event);
    apply_event_to_conversation(&mut conversation, &event);
    assert_eq!(conversation.messages.len(), 1);
}

#[test]
fn authoritative_user_event_replaces_the_local_placeholder() {
    let mut conversation = AiConversation::default();
    conversation.messages.push(AiMessage {
        id: "local-user".into(),
        role: AiRole::User,
        content: "question".into(),
        ..AiMessage::default()
    });
    conversation.item_order.push("local-user".into());

    apply_event_to_conversation(
        &mut conversation,
        &AiEvent::UserMessage(AiMessage {
            id: "user-1".into(),
            entry_id: Some("entry-1".into()),
            role: AiRole::User,
            content: "question".into(),
            ..AiMessage::default()
        }),
    );

    assert_eq!(conversation.messages.len(), 1);
    assert_eq!(conversation.messages[0].id, "user-1");
    assert_eq!(
        conversation.messages[0].entry_id.as_deref(),
        Some("entry-1")
    );
    assert_eq!(conversation.item_order, vec!["user-1".to_owned()]);
}

#[test]
fn authoritative_activity_updates_preserve_terminal_status() {
    let mut conversation = AiConversation::default();
    let running = AiActivity::Tool {
        id: "tool-1".into(),
        name: "shell".into(),
        summary: "Running tests".into(),
        output: String::new(),
        args: None,
        details: None,
        args_truncated: false,
        details_truncated: false,
        duration_ms: None,
        status: AiMessageStatus::Running,
    };
    apply_event_to_conversation(&mut conversation, &AiEvent::ActivityUpdated(running));
    let failed = AiActivity::Tool {
        id: "tool-1".into(),
        name: "shell".into(),
        summary: "Running tests".into(),
        output: "failed".into(),
        args: None,
        details: None,
        args_truncated: false,
        details_truncated: false,
        duration_ms: Some(1234),
        status: AiMessageStatus::Failed,
    };
    apply_event_to_conversation(&mut conversation, &AiEvent::ActivityUpdated(failed));

    assert_eq!(conversation.activity.len(), 1);
    assert!(matches!(
        conversation.activity[0],
        AiActivity::Tool {
            status: AiMessageStatus::Failed,
            ..
        }
    ));
}
