use super::*;
use syntaxis_workspace::{WorkspaceAvailability, WorkspaceIcon, WorkspaceIconSymbol};

#[tokio::test]
async fn shutdown_succeeds_when_pi_has_already_stopped() {
    let (commands, _command_rx) = mpsc::channel(COMMAND_CAPACITY);
    let (shutdown, shutdown_rx) = mpsc::channel(1);
    drop(shutdown_rx);
    let (events, _) = broadcast::channel(EVENT_CAPACITY);
    let (event_input, _event_rx) = mpsc::unbounded_channel();
    let session = HostAgentSession {
        commands,
        shutdown,
        events,
        event_input,
        state: Arc::new(Mutex::new(RuntimeState {
            snapshot: AgentSnapshot::default(),
            session_file: None,
            current_assistant: None,
            accept_initial_history: true,
            fork_messages: Vec::new(),
            extension_requests: VecDeque::new(),
        })),
    };

    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn setting_a_model_updates_the_snapshot_with_its_requested_effort() {
    let (commands, mut command_rx) = mpsc::channel(COMMAND_CAPACITY);
    let (shutdown, _shutdown_rx) = mpsc::channel(1);
    let (events, _) = broadcast::channel(EVENT_CAPACITY);
    let (event_input, mut event_rx) = mpsc::unbounded_channel();
    let model = ModelSummary {
        provider: "openai-codex".into(),
        id: "gpt-5.6-luna".into(),
        name: "GPT 5.6 Luna".into(),
        reasoning: true,
        thinking_levels: ThinkingLevel::STANDARD.to_vec(),
        supports_images: true,
        context_window: 128_000,
        max_tokens: 0x4000, // 16,384 tokens
        cost: ModelCost::default(),
    };
    let snapshot = AgentSnapshot {
        models: vec![model.clone()],
        ..Default::default()
    };
    let session = HostAgentSession {
        commands,
        shutdown,
        events,
        event_input,
        state: Arc::new(Mutex::new(RuntimeState {
            snapshot,
            session_file: None,
            current_assistant: None,
            accept_initial_history: true,
            fork_messages: Vec::new(),
            extension_requests: VecDeque::new(),
        })),
    };

    session
        .handle(ClientMessage::SetModel {
            provider: model.provider.clone(),
            model_id: model.id.clone(),
            thinking_level: ThinkingLevel::High,
        })
        .await
        .unwrap();

    let set_model = command_rx.recv().await.unwrap();
    let set_effort = command_rx.recv().await.unwrap();
    assert_eq!(set_model["type"], "set_model");
    assert_eq!(set_effort["type"], "set_thinking_level");
    assert_eq!(set_effort["level"], "high");
    assert_eq!(session.snapshot().model, Some(model.clone()));
    assert_eq!(session.snapshot().thinking_level, ThinkingLevel::High);
    assert!(matches!(
        event_rx.recv().await,
        Some(ServerMessage::ModelChanged {
            model: Some(changed),
            thinking_level: ThinkingLevel::High,
        }) if changed == model
    ));
}

#[test]
#[allow(
    clippy::panic_in_result_fn,
    reason = "the test uses Result for fallible setup and assertions for behavior"
)]
fn model_parser_preserves_filter_metadata() -> Result<(), String> {
    let model = parse_model(&json!({
        "provider": "example",
        "id": "reasoner",
        "name": "Reasoner",
        "reasoning": true,
        "input": ["text", "image"],
        "contextWindow": 200_000,
        "maxTokens": 32_000,
        "cost": {
            "input": 0.25,
            "output": 1.5,
            "cacheRead": 0.025,
            "cacheWrite": 0,
            "tiers": [{ "inputTokensAbove": 100_000, "input": 0.5 }]
        }
    }))
    .ok_or_else(|| "valid model metadata was rejected".to_owned())?;
    assert!(model.reasoning);
    assert_eq!(model.thinking_levels, ThinkingLevel::STANDARD);
    assert!(model.supports_images);
    assert_eq!(model.context_window, 200_000);
    assert_eq!(model.max_tokens, 32_000);
    assert_eq!(model.cost.input, 250_000);
    assert_eq!(model.cost.output, 1_500_000);
    assert_eq!(model.cost.cache_read, 25_000);
    assert!(model.cost.has_paid_tier);
    assert!(!model.cost.is_free());
    Ok(())
}

#[test]
#[allow(
    clippy::panic_in_result_fn,
    reason = "the test uses Result for fallible setup and assertions for behavior"
)]
fn model_parser_respects_sparse_thinking_level_maps() -> Result<(), String> {
    let model = parse_model(&json!({
        "provider": "example",
        "id": "reasoner",
        "reasoning": true,
        "thinkingLevelMap": {
            "off": null,
            "minimal": null,
            "low": "low",
            "medium": null,
            "high": "high",
            "xhigh": null,
            "max": "max"
        }
    }))
    .ok_or_else(|| "valid model metadata was rejected".to_owned())?;
    assert_eq!(
        model.thinking_levels,
        vec![ThinkingLevel::Low, ThinkingLevel::High, ThinkingLevel::Max],
    );
    assert_eq!(
        model.effective_thinking_level(ThinkingLevel::Medium),
        ThinkingLevel::Low,
    );
    Ok(())
}

#[test]
fn history_maps_pi_messages_and_tool_results() {
    let messages = vec![
        json!({ "role" : "user", "content" : "Inspect src" }),
        json!(
            { "role" : "assistant", "content" : [{ "type" : "thinking", "thinking" :
            "I should inspect." }, { "type" : "text", "text" : "I found it." }, {
            "type" : "toolCall", "id" : "tool-1", "name" : "read", "arguments" : {
            "path" : "src/main.rs" } }], "stopReason": "length" }
        ),
        json!(
            { "role" : "toolResult", "toolCallId" : "tool-1", "toolName" : "read",
            "content" : [{ "type" : "text", "text" : "fn main() {}" }], "isError" :
            false }
        ),
    ];
    let items = map_history(&messages);
    assert_eq!(items.len(), 3);
    assert!(matches!(&items[0], ChatItem::User { text, .. } if text == "Inspect src"),);
    assert!(matches!(
        &items[1],
        ChatItem::Assistant { text, thinking, truncated: true, .. }
        if text == "I found it." && thinking == "I should inspect."
    ),);
    assert!(matches!(
        &items[2],
        ChatItem::Tool { output, .. }
        if output == "fn main() {}"
    ),);
}
#[test]
fn tool_output_is_bounded() {
    let output = extract_result_text(&Value::String("x".repeat(MAX_TOOL_OUTPUT_CHARS + 100)));
    assert!(output.chars().count() <= MAX_TOOL_OUTPUT_CHARS);
    assert!(output.ends_with('…'));
}
#[test]
#[allow(
    clippy::panic_in_result_fn,
    reason = "the test uses Result for fallible setup and assertions for behavior"
)]
fn frames_fragmented_utf8_and_discards_oversized_records() -> Result<(), serde_json::Error> {
    let mut framer = BoundedLfFramer::new(32);
    assert!(framer.push(b"{\"text\":\"a\xE2").is_empty());
    let records = framer.push(b"\x80\xA8b\"}\r\nok\n");
    assert_eq!(records.len(), 2);
    let FramedLine::Line(first) = &records[0] else {
        panic!("expected a JSON record");
    };
    let first: Value = serde_json::from_slice(first)?;
    assert_eq!(first["text"], "a\u{2028}b");
    let mut bounded = BoundedLfFramer::new(4);
    assert_eq!(
        bounded.push(b"12345\nok\n"),
        vec![FramedLine::Oversized, FramedLine::Line(b"ok".to_vec())]
    );
    Ok(())
}
#[test]
fn only_agent_settled_marks_the_session_idle() {
    let state = Arc::new(Mutex::new(RuntimeState {
        snapshot: AgentSnapshot {
            status: AgentStatus::Working,
            status_message: "Working".into(),
            pending_messages: 2,
            ..AgentSnapshot::default()
        },
        session_file: None,
        current_assistant: None,
        accept_initial_history: false,
        fork_messages: Vec::new(),
        extension_requests: VecDeque::new(),
    }));
    let (events, mut receiver) = mpsc::unbounded_channel();
    let (commands, _command_receiver) = mpsc::channel(1);
    handle_pi_record(&json!({"type":"agent_end"}), &state, &events, &commands);
    assert_eq!(lock(&state).snapshot.status, AgentStatus::Working);
    assert_eq!(lock(&state).snapshot.pending_messages, 2);
    receiver.try_recv().unwrap_err();

    handle_pi_record(&json!({"type":"agent_settled"}), &state, &events, &commands);
    assert_eq!(lock(&state).snapshot.status, AgentStatus::Ready);
    assert_eq!(lock(&state).snapshot.pending_messages, 0);
    assert!(matches!(
        receiver.try_recv(),
        Ok(ServerMessage::Status {
            status: AgentStatus::Ready,
            ..
        })
    ));
}
#[test]
fn queue_updates_preserve_steering_and_follow_up_counts() {
    let state = Arc::new(Mutex::new(RuntimeState {
        snapshot: AgentSnapshot {
            status: AgentStatus::Working,
            ..AgentSnapshot::default()
        },
        session_file: None,
        current_assistant: None,
        accept_initial_history: false,
        fork_messages: Vec::new(),
        extension_requests: VecDeque::new(),
    }));
    let (events, _receiver) = mpsc::unbounded_channel();
    let (commands, _command_receiver) = mpsc::channel(1);
    handle_pi_record(
        &json!({
            "type":"queue_update",
            "steering":[{"message":"one"}],
            "followUp":[{"message":"two"},{"message":"three"}]
        }),
        &state,
        &events,
        &commands,
    );
    assert_eq!(lock(&state).snapshot.pending_messages, 3);
    assert_eq!(lock(&state).snapshot.steering_queue, ["one"]);
    assert_eq!(lock(&state).snapshot.follow_up_queue, ["two", "three"]);
}
#[test]
#[allow(
    clippy::panic_in_result_fn,
    reason = "the test uses Result for fallible setup and assertions for behavior"
)]
fn completion_and_attention_notifications_replace_and_clear()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let record = WorkspaceRecord {
        id: WorkspaceId::new("workspace-1"),
        slug: "project-one".into(),
        name: "Project One".into(),
        root: temp.path().to_string_lossy().into_owned(),
        icon: WorkspaceIcon::Symbol {
            name: WorkspaceIconSymbol::Folder,
        },
        profile: syntaxis_workspace::WorkspaceProfile::default(),
        registered_at_unix_ms: 0,
        last_opened_unix_ms: 0,
        last_section: syntaxis_workspace::WorkspaceSection::default(),
        availability: WorkspaceAvailability::Available,
    };
    let notifications = HostNotificationHub::default();
    let workspace = HostAgentWorkspace::new(record, notifications.clone());
    lock(&workspace.sessions).insert(
        "session-1".into(),
        ManagedSession {
            path: None,
            summary: AgentSessionSummary {
                id: "session-1".into(),
                title: "Fix the tests".into(),
                updated_at_ms: 0,
                status: AgentStatus::Working,
                status_message: "Working".into(),
                running: true,
            },
            process: None,
        },
    );

    workspace.update_summary(
        "session-1",
        &ServerMessage::Status {
            status: AgentStatus::Ready,
            message: "Ready".into(),
            pending_messages: 0,
        },
    );
    let snapshot = notifications.snapshot();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].kind, NotificationKind::Completed);

    workspace.update_summary(
        "session-1",
        &ServerMessage::ExtensionUiRequest {
            request: ExtensionUiRequest {
                id: "request-1".into(),
                method: "confirm".into(),
                title: "Confirm change".into(),
                message: "Apply the migration?".into(),
                options: Vec::new(),
                placeholder: None,
                prefill: None,
            },
        },
    );
    let snapshot = notifications.snapshot();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].kind, NotificationKind::Attention);
    assert_eq!(snapshot[0].message, "Apply the migration?");

    notifications.clear(
        "workspace-1",
        &NotificationTarget::Agent {
            session_id: "session-1".into(),
        },
    );
    assert!(notifications.snapshot().is_empty());
    Ok(())
}
