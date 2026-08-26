//! Inbound Pi protocol reduction and ordered UI event emission.

use super::*;

#[allow(
    clippy::too_many_lines,
    reason = "keeping Pi event dispatch in one exhaustive match makes protocol coverage auditable"
)]
pub(super) fn handle_pi_record(
    record: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &mpsc::UnboundedSender<ServerMessage>,
    command_tx: &mpsc::Sender<Value>,
) {
    let Some(kind) = record.get("type").and_then(Value::as_str) else {
        return;
    };
    if kind == "response" {
        handle_pi_response(record, state, events);
        return;
    }
    match kind {
        "agent_start" | "turn_start" => {
            set_status(state, events, AgentStatus::Working, "Pi is working…", None);
        }
        "agent_end" => {
            let mut guard = lock(state);
            let finalized = finalize_current_assistant(&mut guard, ItemStatus::Complete);
            drop(guard);
            if let Some(item) = finalized {
                let _ = events.send(ServerMessage::ItemUpdated { item });
            }
        }
        "agent_settled" => {
            let mut guard = lock(state);
            let finalized = finalize_current_assistant(&mut guard, ItemStatus::Complete);
            guard.snapshot.pending_messages = 0;
            guard.snapshot.steering_queue.clear();
            guard.snapshot.follow_up_queue.clear();
            guard.snapshot.status = AgentStatus::Ready;
            guard.snapshot.status_message = "Ready".into();
            drop(guard);
            if let Some(item) = finalized {
                let _ = events.send(ServerMessage::ItemUpdated { item });
            }
            let _ = events.send(ServerMessage::Status {
                status: AgentStatus::Ready,
                message: "Ready".into(),
                pending_messages: 0,
            });
            for command in ["get_session_stats", "get_fork_messages"] {
                let _ = command_tx.try_send(json!(
                    { "id" : new_id("syntaxis-refresh"), "type" : command }
                ));
            }
        }
        "queue_update" => {
            let steering = queue_messages(record.get("steering"));
            let follow_up = queue_messages(record.get("followUp"));
            let pending_messages = steering.len().saturating_add(follow_up.len());
            let mut guard = lock(state);
            guard.snapshot.pending_messages = pending_messages;
            guard.snapshot.steering_queue.clone_from(&steering);
            guard.snapshot.follow_up_queue.clone_from(&follow_up);
            let status = guard.snapshot.status;
            let message = guard.snapshot.status_message.clone();
            drop(guard);
            let _ = events.send(ServerMessage::QueueChanged {
                steering,
                follow_up,
            });
            let _ = events.send(ServerMessage::Status {
                status,
                message,
                pending_messages,
            });
        }
        "message_start" => handle_message_start(record, state, events),
        "message_update" => handle_message_update(record, state, events),
        "message_end" => handle_message_end(record, state, events),
        "tool_execution_start" => handle_tool_start(record, state, events),
        "tool_execution_update" => handle_tool_update(record, state, events, false),
        "tool_execution_end" => handle_tool_update(record, state, events, true),
        "compaction_start" => set_status(
            state,
            events,
            AgentStatus::Compacting,
            "Pi is compacting the conversation…",
            None,
        ),
        "compaction_end" => set_status(
            state,
            events,
            AgentStatus::Working,
            "Compaction complete",
            None,
        ),
        "auto_retry_start" => {
            let attempt = record.get("attempt").and_then(Value::as_u64).unwrap_or(1);
            let maximum = record
                .get("maxAttempts")
                .and_then(Value::as_u64)
                .unwrap_or(attempt);
            let delay_ms = record.get("delayMs").and_then(Value::as_u64).unwrap_or(0);
            let delay = if delay_ms >= 1_000 {
                format!(" in {}s", delay_ms.div_ceil(1_000))
            } else {
                String::new()
            };
            set_status(
                state,
                events,
                AgentStatus::Working,
                &format!("Retrying{delay} · attempt {attempt} of {maximum}"),
                None,
            );
        }
        "auto_retry_end" => {
            if record.get("success").and_then(Value::as_bool) == Some(false) {
                let message = string_field(record, "finalError")
                    .unwrap_or_else(|| "Pi exhausted its automatic retries".into());
                let _ = events.send(ServerMessage::Error {
                    error: AgentError::new(AgentErrorCode::Unavailable, message),
                });
            }
        }
        "extension_ui_request" => handle_extension_request(record, state, events),
        "extension_error" => {
            let message = string_field(record, "error")
                .or_else(|| string_field(record, "message"))
                .unwrap_or_else(|| "A Pi extension failed".into());
            let _ = events.send(ServerMessage::Error {
                error: AgentError::new(AgentErrorCode::Internal, message),
            });
        }
        _ => {}
    }
}

pub(super) fn queue_messages(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.as_str()
                .map(str::to_owned)
                .or_else(|| string_field(item, "message"))
        })
        .collect()
}
#[allow(
    clippy::too_many_lines,
    reason = "keeping Pi response dispatch in one exhaustive match makes protocol coverage auditable"
)]
pub(super) fn handle_pi_response(
    response: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &mpsc::UnboundedSender<ServerMessage>,
) {
    if response.get("success").and_then(Value::as_bool) == Some(false) {
        let message =
            string_field(response, "error").unwrap_or_else(|| "Pi rejected a request".into());
        let _ = events.send(ServerMessage::Error {
            error: AgentError::new(AgentErrorCode::InvalidRequest, message),
        });
        return;
    }
    let command = response
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let data = response.get("data").unwrap_or(&Value::Null);
    match command {
        "get_state" => {
            let mut guard = lock(state);
            guard.session_file = string_field(data, "sessionFile").map(PathBuf::from);
            apply_session_state(&mut guard.snapshot, data);
            let snapshot = guard.snapshot.clone();
            drop(guard);
            let _ = events.send(ServerMessage::SessionChanged {
                session_id: snapshot.session_id,
                session_name: snapshot.session_name,
            });
            let _ = events.send(ServerMessage::ModelChanged {
                model: snapshot.model,
                thinking_level: snapshot.thinking_level,
            });
            let _ = events.send(ServerMessage::Status {
                status: snapshot.status,
                message: snapshot.status_message,
                pending_messages: snapshot.pending_messages,
            });
        }
        "get_messages" => handle_messages_response(data, state, events),
        "get_fork_messages" => handle_fork_messages_response(data, state, events),
        "fork" => {
            if data.get("cancelled").and_then(Value::as_bool) != Some(true)
                && let Some(text) = string_field(data, "text")
            {
                let _ = events.send(ServerMessage::ComposerText { text });
            }
        }
        "export_html" => {
            const MAX_EXPORT_BYTES: u64 = 8 * 1024 * 1024;
            if let Some(path) = string_field(data, "path").map(PathBuf::from) {
                let is_syntaxis_export = path.parent() == Some(std::env::temp_dir().as_path())
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| {
                            name.starts_with("syntaxis-session-")
                                && Path::new(name)
                                    .extension()
                                    .is_some_and(|extension| extension.eq_ignore_ascii_case("html"))
                        });
                let contents = is_syntaxis_export
                    .then(|| std::fs::metadata(&path).ok())
                    .flatten()
                    .filter(|metadata| metadata.len() <= MAX_EXPORT_BYTES)
                    .and_then(|_| std::fs::read(&path).ok());
                let _ = std::fs::remove_file(&path);
                if let Some(contents) = contents {
                    let _ = events.send(ServerMessage::ExportReady {
                        filename: "pi-session.html".into(),
                        data_base64: BASE64.encode(contents),
                    });
                } else {
                    let _ = events.send(ServerMessage::Error {
                        error: AgentError::new(
                            AgentErrorCode::Unavailable,
                            "The exported session could not be downloaded or exceeded 8 MiB",
                        ),
                    });
                }
            } else {
                let _ = events.send(ServerMessage::Error {
                    error: AgentError::new(
                        AgentErrorCode::Unavailable,
                        "Pi completed the export without returning a download path",
                    ),
                });
            }
        }
        "get_available_models" => {
            let models = data
                .get("models")
                .and_then(Value::as_array)
                .map(|models| models.iter().filter_map(parse_model).collect::<Vec<_>>())
                .unwrap_or_default();
            lock(state).snapshot.models.clone_from(&models);
            let _ = events.send(ServerMessage::Models { models });
        }
        "get_commands" => {
            let commands = data
                .get("commands")
                .and_then(Value::as_array)
                .map(|commands| {
                    commands
                        .iter()
                        .filter_map(parse_command)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            lock(state).snapshot.commands.clone_from(&commands);
            let _ = events.send(ServerMessage::Commands { commands });
        }
        "get_session_stats" => {
            let session_stats = parse_session_stats(data);
            lock(state).snapshot.session_stats = Some(session_stats.clone());
            let _ = events.send(ServerMessage::SessionStats {
                stats: session_stats,
            });
        }
        "set_model" => {
            let model = parse_model(data);
            let mut guard = lock(state);
            guard.snapshot.model.clone_from(&model);
            let thinking_level = guard.snapshot.thinking_level;
            drop(guard);
            let _ = events.send(ServerMessage::ModelChanged {
                model,
                thinking_level,
            });
        }
        "new_session" => {
            let _ = events.send(ServerMessage::Snapshot {
                snapshot: lock(state).snapshot.clone(),
            });
        }
        _ => {}
    }
}
pub(super) fn handle_messages_response(
    data: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &mpsc::UnboundedSender<ServerMessage>,
) {
    let Some(messages) = data.get("messages").and_then(Value::as_array) else {
        return;
    };
    let mut guard = lock(state);
    if !guard.accept_initial_history {
        return;
    }
    guard.snapshot.items = map_history(messages);
    let fork_messages = guard.fork_messages.clone();
    apply_fork_message_ids(&mut guard.snapshot.items, &fork_messages);
    guard.current_assistant = None;
    guard.accept_initial_history = false;
    let snapshot = guard.snapshot.clone();
    drop(guard);
    let _ = events.send(ServerMessage::Snapshot { snapshot });
}

pub(super) fn handle_fork_messages_response(
    data: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &mpsc::UnboundedSender<ServerMessage>,
) {
    let fork_messages = data
        .get("messages")
        .and_then(Value::as_array)
        .map(|messages| {
            messages
                .iter()
                .filter_map(|message| {
                    Some((
                        string_field(message, "entryId")?,
                        string_field(message, "text").unwrap_or_default(),
                    ))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut guard = lock(state);
    guard.fork_messages = fork_messages;
    let messages = guard.fork_messages.clone();
    guard.snapshot.fork_points.clone_from(&messages);
    apply_fork_message_ids(&mut guard.snapshot.items, &messages);
    let snapshot = guard.snapshot.clone();
    drop(guard);
    let _ = events.send(ServerMessage::Snapshot { snapshot });
}

pub(super) fn handle_message_start(
    record: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &mpsc::UnboundedSender<ServerMessage>,
) {
    let message = record.get("message").unwrap_or(&Value::Null);
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return;
    }
    let item = assistant_item_from_message(message, ItemStatus::Streaming, new_id("assistant"));
    let id = item.id().to_owned();
    let mut guard = lock(state);
    guard.accept_initial_history = false;
    guard.current_assistant = Some(id);
    push_item(&mut guard.snapshot.items, item.clone());
    drop(guard);
    let _ = events.send(ServerMessage::ItemAdded { item });
}
pub(super) fn handle_message_update(
    record: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &mpsc::UnboundedSender<ServerMessage>,
) {
    let update = record.get("assistantMessageEvent").unwrap_or(&Value::Null);
    let update_type = update
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(delta) = update.get("delta").and_then(Value::as_str) else {
        return;
    };
    let thinking = update_type == "thinking_delta";
    if update_type != "text_delta" && !thinking {
        return;
    }
    let mut guard = lock(state);
    let item_id = ensure_current_assistant(&mut guard, events);
    if let Some(ChatItem::Assistant {
        text,
        thinking: reasoning,
        ..
    }) = guard
        .snapshot
        .items
        .iter_mut()
        .find(|item| item.id() == item_id)
    {
        if thinking {
            reasoning.push_str(delta);
        } else {
            text.push_str(delta);
        }
    }
    drop(guard);
    let _ = events.send(ServerMessage::ItemDelta {
        item_id,
        text: delta.into(),
        thinking,
    });
}
pub(super) fn handle_message_end(
    record: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &mpsc::UnboundedSender<ServerMessage>,
) {
    let message = record.get("message").unwrap_or(&Value::Null);
    let role = message.get("role").and_then(Value::as_str);
    if !matches!(role, Some("assistant" | "user" | "toolResult")) {
        if let Some(item) = custom_item_from_message(message) {
            let mut guard = lock(state);
            if let Some(existing) = guard
                .snapshot
                .items
                .iter_mut()
                .find(|candidate| candidate.id() == item.id())
            {
                existing.clone_from(&item);
            } else {
                push_item(&mut guard.snapshot.items, item.clone());
            }
            drop(guard);
            let _ = events.send(ServerMessage::ItemUpdated { item });
        }
        return;
    }
    if role != Some("assistant") {
        return;
    }
    let mut guard = lock(state);
    let id = guard
        .current_assistant
        .take()
        .unwrap_or_else(|| new_id("assistant"));
    let status = if message
        .get("errorMessage")
        .is_some_and(|value| !value.is_null())
    {
        ItemStatus::Failed
    } else {
        ItemStatus::Complete
    };
    let item = assistant_item_from_message(message, status, id.clone());
    if let Some(existing) = guard
        .snapshot
        .items
        .iter_mut()
        .find(|candidate| candidate.id() == id)
    {
        *existing = item.clone();
    } else {
        push_item(&mut guard.snapshot.items, item.clone());
    }
    drop(guard);
    let _ = events.send(ServerMessage::ItemUpdated { item });
}
pub(super) fn handle_tool_start(
    record: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &mpsc::UnboundedSender<ServerMessage>,
) {
    let id = string_field(record, "toolCallId").unwrap_or_else(|| new_id("tool"));
    let name = string_field(record, "toolName").unwrap_or_else(|| "tool".into());
    let summary = summarize_tool(&name, record.get("args"));
    let (args, args_truncated) = bounded_json(record.get("args"));
    let item = ChatItem::Tool {
        id,
        name,
        summary,
        output: String::new(),
        args,
        details: None,
        args_truncated,
        details_truncated: false,
        status: ItemStatus::Running,
    };
    push_item(&mut lock(state).snapshot.items, item.clone());
    let _ = events.send(ServerMessage::ItemAdded { item });
}
pub(super) fn handle_tool_update(
    record: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &mpsc::UnboundedSender<ServerMessage>,
    complete: bool,
) {
    let Some(id) = string_field(record, "toolCallId") else {
        return;
    };
    let name = string_field(record, "toolName").unwrap_or_else(|| "tool".into());
    let result = if complete {
        record.get("result")
    } else {
        record.get("partialResult")
    };
    let output = result.map_or_else(String::new, extract_result_text);
    let detail_source = result
        .and_then(|value| value.get("details"))
        .or_else(|| record.get("details"));
    let (next_details, next_details_truncated) = bounded_json(detail_source);
    let status = if complete {
        if record.get("isError").and_then(Value::as_bool) == Some(true) {
            ItemStatus::Failed
        } else {
            ItemStatus::Complete
        }
    } else {
        ItemStatus::Running
    };
    let mut guard = lock(state);
    let existing = guard.snapshot.items.iter_mut().find(|item| item.id() == id);
    let item = if let Some(ChatItem::Tool {
        name: existing_name,
        summary,
        output: existing_output,
        args,
        details,
        args_truncated,
        details_truncated,
        status: existing_status,
        ..
    }) = existing
    {
        if !output.is_empty() {
            existing_output.clone_from(&output);
        }
        existing_name.clone_from(&name);
        *existing_status = status;
        if next_details.is_some() {
            details.clone_from(&next_details);
            *details_truncated = next_details_truncated;
        }
        ChatItem::Tool {
            id: id.clone(),
            name: existing_name.clone(),
            summary: summary.clone(),
            output: existing_output.clone(),
            args: args.clone(),
            details: details.clone(),
            args_truncated: *args_truncated,
            details_truncated: *details_truncated,
            status,
        }
    } else {
        ChatItem::Tool {
            id: id.clone(),
            name,
            summary: String::new(),
            output,
            args: None,
            details: next_details,
            args_truncated: false,
            details_truncated: next_details_truncated,
            status,
        }
    };
    if existing.is_none() {
        push_item(&mut guard.snapshot.items, item.clone());
    }
    drop(guard);
    let _ = events.send(ServerMessage::ItemUpdated { item });
}
#[allow(
    clippy::too_many_lines,
    reason = "extension UI methods share validation and queueing state best kept in one dispatcher"
)]
pub(super) fn handle_extension_request(
    record: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &mpsc::UnboundedSender<ServerMessage>,
) {
    let method = string_field(record, "method").unwrap_or_else(|| "notify".into());
    if method == "set_editor_text" {
        if let Some(text) = string_field(record, "text") {
            let _ = events.send(ServerMessage::ComposerText { text });
        }
        return;
    }
    if method == "notify" {
        let text = string_field(record, "message").unwrap_or_default();
        if !text.is_empty() {
            let status = if record.get("notifyType").and_then(Value::as_str) == Some("error") {
                ItemStatus::Failed
            } else {
                ItemStatus::Complete
            };
            let item = ChatItem::Notice {
                id: new_id("notice"),
                text,
                status,
            };
            push_item(&mut lock(state).snapshot.items, item.clone());
            let _ = events.send(ServerMessage::ItemAdded { item });
        }
        return;
    }
    if matches!(method.as_str(), "setStatus" | "setWidget" | "setTitle") {
        let mut guard = lock(state);
        match method.as_str() {
            "setStatus" => {
                let key = string_field(record, "statusKey").unwrap_or_else(|| "extension".into());
                guard
                    .snapshot
                    .extension_statuses
                    .retain(|(existing, _)| existing != &key);
                if let Some(text) =
                    string_field(record, "statusText").filter(|text| !text.is_empty())
                {
                    guard
                        .snapshot
                        .extension_statuses
                        .push((key, truncate_chars(text, 240)));
                }
            }
            "setWidget" => {
                let key = string_field(record, "widgetKey").unwrap_or_else(|| "extension".into());
                guard
                    .snapshot
                    .extension_widgets
                    .retain(|widget| widget.key != key);
                let lines = record
                    .get("widgetLines")
                    .and_then(Value::as_array)
                    .map(|lines| {
                        lines
                            .iter()
                            .filter_map(Value::as_str)
                            .take(20)
                            .map(|line| truncate_chars(line.to_owned(), 500))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if !lines.is_empty() {
                    let placement = match record.get("widgetPlacement").and_then(Value::as_str) {
                        Some("belowEditor") => "belowEditor",
                        _ => "aboveEditor",
                    };
                    guard.snapshot.extension_widgets.push(ExtensionWidget {
                        key,
                        lines,
                        placement: placement.into(),
                    });
                }
            }
            "setTitle" => {
                guard.snapshot.extension_title = string_field(record, "title")
                    .filter(|title| !title.is_empty())
                    .map(|title| truncate_chars(title, 120));
            }
            _ => {}
        }
        let _ = events.send(ServerMessage::ExtensionSurfaces {
            title: guard.snapshot.extension_title.clone(),
            statuses: guard.snapshot.extension_statuses.clone(),
            widgets: guard.snapshot.extension_widgets.clone(),
        });
        return;
    }
    if !matches!(method.as_str(), "select" | "confirm" | "input" | "editor") {
        return;
    }
    let request = ExtensionUiRequest {
        id: string_field(record, "id").unwrap_or_else(|| new_id("extension")),
        method,
        title: string_field(record, "title").unwrap_or_else(|| "Pi".into()),
        message: string_field(record, "message").unwrap_or_default(),
        options: record
            .get("options")
            .and_then(Value::as_array)
            .map(|options| {
                options
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        placeholder: string_field(record, "placeholder"),
        prefill: string_field(record, "prefill"),
    };
    let should_display = {
        let mut state = lock(state);
        if state.extension_requests.len() >= MAX_EXTENSION_REQUESTS {
            let _ = events.send(ServerMessage::Error {
                error: AgentError::new(
                    AgentErrorCode::Unavailable,
                    "A Pi extension opened too many dialogs; the newest request was discarded",
                ),
            });
            return;
        }
        let should_display = state.extension_requests.is_empty();
        state.extension_requests.push_back(request.clone());
        if should_display {
            state.snapshot.pending_extension_request = Some(request.clone());
        }
        should_display
    };
    if should_display {
        let _ = events.send(ServerMessage::ExtensionUiRequest { request });
    }
}
pub(super) fn ensure_current_assistant(
    state: &mut RuntimeState,
    events: &mpsc::UnboundedSender<ServerMessage>,
) -> String {
    if let Some(id) = state.current_assistant.as_ref() {
        return id.clone();
    }
    let id = new_id("assistant");
    let item = ChatItem::Assistant {
        id: id.clone(),
        text: String::new(),
        thinking: String::new(),
        status: ItemStatus::Streaming,
        truncated: false,
    };
    state.current_assistant = Some(id.clone());
    state.accept_initial_history = false;
    push_item(&mut state.snapshot.items, item.clone());
    let _ = events.send(ServerMessage::ItemAdded { item });
    id
}
pub(super) fn finalize_current_assistant(
    state: &mut RuntimeState,
    status: ItemStatus,
) -> Option<ChatItem> {
    let id = state.current_assistant.take()?;
    let item = state
        .snapshot
        .items
        .iter_mut()
        .find(|item| item.id() == id)?;
    if let ChatItem::Assistant {
        status: item_status,
        ..
    } = item
    {
        *item_status = status;
        return Some(item.clone());
    }
    None
}
pub(super) fn set_status(
    state: &Arc<Mutex<RuntimeState>>,
    events: &mpsc::UnboundedSender<ServerMessage>,
    status: AgentStatus,
    message: &str,
    pending_messages: Option<usize>,
) {
    let pending_messages = {
        let mut guard = lock(state);
        guard.snapshot.status = status;
        guard.snapshot.status_message = message.into();
        if let Some(pending_messages) = pending_messages {
            guard.snapshot.pending_messages = pending_messages;
        }
        guard.snapshot.pending_messages
    };
    let _ = events.send(ServerMessage::Status {
        status,
        message: message.into(),
        pending_messages,
    });
}
