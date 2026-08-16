//! Mapping between Pi protocol values and typed agent state.

use super::*;

pub(super) fn apply_session_state(snapshot: &mut AgentSnapshot, value: &Value) {
    snapshot.session_id = string_field(value, "sessionId");
    snapshot.session_name = string_field(value, "sessionName");
    snapshot.model = value.get("model").and_then(parse_model);
    snapshot.thinking_level = value
        .get("thinkingLevel")
        .and_then(Value::as_str)
        .and_then(parse_thinking_level)
        .unwrap_or(snapshot.thinking_level);
    snapshot.pending_messages = value
        .get("pendingMessageCount")
        .and_then(Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
        .unwrap_or_default();
    let streaming = value
        .get("isStreaming")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let compacting = value
        .get("isCompacting")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    (snapshot.status, snapshot.status_message) = if compacting {
        (
            AgentStatus::Compacting,
            "Pi is compacting the conversation…".into(),
        )
    } else if streaming {
        (AgentStatus::Working, "Pi is working…".into())
    } else {
        (AgentStatus::Ready, "Ready".into())
    };
}
pub(super) fn parse_model(value: &Value) -> Option<ModelSummary> {
    let provider = string_field(value, "provider")?;
    let id = string_field(value, "id")?;
    let name = string_field(value, "name").unwrap_or_else(|| id.clone());
    let reasoning = value
        .get("reasoning")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let supports_images = value
        .get("input")
        .and_then(Value::as_array)
        .is_some_and(|inputs| inputs.iter().any(|input| input.as_str() == Some("image")));
    Some(ModelSummary {
        provider,
        id,
        name,
        reasoning,
        thinking_levels: parse_thinking_levels(value, reasoning),
        supports_images,
        context_window: value
            .get("contextWindow")
            .and_then(Value::as_u64)
            .unwrap_or(128_000),
        max_tokens: value
            .get("maxTokens")
            .and_then(Value::as_u64)
            .unwrap_or(0x4000),
        cost: parse_model_cost(value.get("cost")),
    })
}
fn parse_thinking_levels(model: &Value, reasoning: bool) -> Vec<ThinkingLevel> {
    if !reasoning {
        return ThinkingLevel::OFF_ONLY.to_vec();
    }
    let mapping = model.get("thinkingLevelMap").and_then(Value::as_object);
    ThinkingLevel::ALL
        .into_iter()
        .filter(
            |level| match mapping.and_then(|mapping| mapping.get(level.as_str())) {
                Some(Value::Null) => false,
                Some(Value::String(_)) => true,
                _ => ThinkingLevel::STANDARD.contains(level),
            },
        )
        .collect()
}

pub(super) fn parse_model_cost(cost: Option<&Value>) -> ModelCost {
    let rate = |value: Option<&Value>, field| {
        value
            .and_then(|value| value.get(field))
            .and_then(Value::as_f64)
            .map_or(0, |rate| rounded_u64(rate * 1_000_000.0, 1.0e15))
    };
    let has_paid_tier = cost
        .and_then(|cost| cost.get("tiers"))
        .and_then(Value::as_array)
        .is_some_and(|tiers| {
            tiers.iter().any(|tier| {
                ["input", "output", "cacheRead", "cacheWrite"]
                    .into_iter()
                    .any(|field| rate(Some(tier), field) > 0)
            })
        });
    ModelCost {
        input: rate(cost, "input"),
        output: rate(cost, "output"),
        cache_read: rate(cost, "cacheRead"),
        cache_write: rate(cost, "cacheWrite"),
        has_paid_tier,
    }
}
pub(super) fn parse_command(value: &Value) -> Option<PiCommand> {
    Some(PiCommand {
        name: string_field(value, "name")?,
        description: string_field(value, "description").unwrap_or_default(),
        source: string_field(value, "source").unwrap_or_else(|| "command".into()),
        location: string_field(value, "location"),
        argument_hint: string_field(value, "argumentHint"),
        invocation: string_field(value, "invocation"),
    })
}
pub(super) fn parse_session_stats(value: &Value) -> SessionStats {
    let tokens = value.get("tokens").unwrap_or(&Value::Null);
    let context = value.get("contextUsage").unwrap_or(&Value::Null);
    SessionStats {
        user_messages: u64_field(value, "userMessages"),
        assistant_messages: u64_field(value, "assistantMessages"),
        tool_calls: u64_field(value, "toolCalls"),
        total_messages: u64_field(value, "totalMessages"),
        tokens: TokenUsage {
            input: u64_field(tokens, "input"),
            output: u64_field(tokens, "output"),
            cache_read: u64_field(tokens, "cacheRead"),
            cache_write: u64_field(tokens, "cacheWrite"),
            total: u64_field(tokens, "total"),
        },
        cost_microusd: value
            .get("cost")
            .and_then(Value::as_f64)
            .map_or(0, |cost| rounded_u64(cost * 1_000_000.0, 1.0e15)),
        context_tokens: context.get("tokens").and_then(Value::as_u64),
        context_window: context.get("contextWindow").and_then(Value::as_u64),
        context_percent: context
            .get("percent")
            .and_then(Value::as_f64)
            .and_then(|percent| u8::try_from(rounded_u64(percent, 100.0)).ok()),
    }
}
pub(super) fn pi_image(image: &ImageAttachment) -> Value {
    json!({ "type" : "image", "data" : image.data, "mimeType" : image.mime_type, })
}
pub(super) fn u64_field(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or_default()
}
pub(super) fn rounded_u64(value: f64, maximum: f64) -> u64 {
    format!("{:.0}", value.clamp(0.0, maximum))
        .parse()
        .unwrap_or_default()
}
pub(super) fn parse_thinking_level(value: &str) -> Option<ThinkingLevel> {
    match value {
        "off" => Some(ThinkingLevel::Off),
        "minimal" => Some(ThinkingLevel::Minimal),
        "low" => Some(ThinkingLevel::Low),
        "medium" => Some(ThinkingLevel::Medium),
        "high" => Some(ThinkingLevel::High),
        "xhigh" => Some(ThinkingLevel::Xhigh),
        "max" => Some(ThinkingLevel::Max),
        _ => None,
    }
}
pub(super) fn map_history(messages: &[Value]) -> Vec<ChatItem> {
    let mut items = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        match message.get("role").and_then(Value::as_str) {
            Some("user") => {
                let text = extract_message_text(message);
                if !text.trim().is_empty() {
                    push_item(
                        &mut items,
                        ChatItem::User {
                            id: format!("history-user-{index}"),
                            entry_id: None,
                            text,
                            images: extract_message_images(message),
                        },
                    );
                }
            }
            Some("custom") => {
                if let Some(mut item) = custom_item_from_message(message) {
                    if let ChatItem::Custom { id, .. } = &mut item {
                        *id = string_field(message, "id")
                            .unwrap_or_else(|| format!("history-custom-{index}"));
                    }
                    push_item(&mut items, item);
                }
            }
            Some("assistant") => {
                let item = assistant_item_from_message(
                    message,
                    if message
                        .get("errorMessage")
                        .is_some_and(|value| !value.is_null())
                    {
                        ItemStatus::Failed
                    } else {
                        ItemStatus::Complete
                    },
                    format!("history-assistant-{index}"),
                );
                if let ChatItem::Assistant { text, thinking, .. } = &item
                    && (!text.is_empty() || !thinking.is_empty())
                {
                    push_item(&mut items, item);
                }
                if let Some(content) = message.get("content").and_then(Value::as_array) {
                    for part in content {
                        if part.get("type").and_then(Value::as_str) == Some("toolCall") {
                            let id = string_field(part, "id")
                                .unwrap_or_else(|| format!("history-tool-{index}"));
                            let name = string_field(part, "name").unwrap_or_else(|| "tool".into());
                            let (args, args_truncated) = bounded_json(part.get("arguments"));
                            push_item(
                                &mut items,
                                ChatItem::Tool {
                                    id,
                                    summary: summarize_tool(&name, part.get("arguments")),
                                    name,
                                    output: String::new(),
                                    args,
                                    details: None,
                                    args_truncated,
                                    details_truncated: false,
                                    status: ItemStatus::Complete,
                                },
                            );
                        }
                    }
                }
            }
            Some("toolResult") => {
                if let Some(id) = string_field(message, "toolCallId") {
                    let output = message
                        .get("content")
                        .map_or_else(String::new, extract_result_text);
                    if let Some(ChatItem::Tool {
                        output: existing,
                        details,
                        details_truncated,
                        status,
                        ..
                    }) = items.iter_mut().find(|item| item.id() == id)
                    {
                        existing.clone_from(&output);
                        let (next_details, next_truncated) = bounded_json(message.get("details"));
                        if next_details.is_some() {
                            details.clone_from(&next_details);
                            *details_truncated = next_truncated;
                        }
                        *status = if message.get("isError").and_then(Value::as_bool) == Some(true) {
                            ItemStatus::Failed
                        } else {
                            ItemStatus::Complete
                        };
                    }
                }
            }
            _ => {}
        }
    }
    items
}
pub(super) fn apply_fork_message_ids(items: &mut [ChatItem], messages: &[(String, String)]) {
    let mut message_index = 0;
    for item in items {
        let ChatItem::User { entry_id, text, .. } = item else {
            continue;
        };
        *entry_id = None;
        if let Some(offset) = messages[message_index..]
            .iter()
            .position(|(_, candidate)| candidate == text)
        {
            message_index += offset;
            *entry_id = Some(messages[message_index].0.clone());
            message_index += 1;
        }
    }
}

pub(super) fn assistant_item_from_message(
    message: &Value,
    status: ItemStatus,
    id: String,
) -> ChatItem {
    let mut text = String::new();
    let mut thinking = String::new();
    if let Some(content) = message.get("content").and_then(Value::as_array) {
        for part in content {
            match part.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(value) = part.get("text").and_then(Value::as_str) {
                        text.push_str(value);
                    }
                }
                Some("thinking") => {
                    if let Some(value) = part.get("thinking").and_then(Value::as_str) {
                        thinking.push_str(value);
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(error) = message.get("errorMessage").and_then(Value::as_str) {
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(error);
    }
    ChatItem::Assistant {
        id,
        text,
        thinking,
        status,
    }
}
pub(super) fn extract_message_text(message: &Value) -> String {
    match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}
pub(super) fn extract_message_images(message: &Value) -> Vec<ImageAttachment> {
    message
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("image"))
        .filter_map(|part| {
            let data = string_field(part, "data").or_else(|| string_field(part, "content"))?;
            let mime_type = string_field(part, "mimeType")?;
            Some(ImageAttachment {
                name: string_field(part, "fileName").unwrap_or_else(|| "image".into()),
                size: u64::try_from(data.len().saturating_mul(3) / 4).unwrap_or(u64::MAX),
                mime_type,
                data,
            })
        })
        .collect()
}
pub(super) fn custom_item_from_message(message: &Value) -> Option<ChatItem> {
    if message.get("display").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let label = string_field(message, "customType")
        .or_else(|| string_field(message, "role"))
        .unwrap_or_else(|| "extension".into());
    let text = string_field(message, "summary").unwrap_or_else(|| extract_message_text(message));
    let (details, details_truncated) = bounded_json(message.get("details"));
    if text.trim().is_empty() && details.is_none() {
        return None;
    }
    Some(ChatItem::Custom {
        id: string_field(message, "id").unwrap_or_else(|| new_id("custom")),
        label,
        text,
        details,
        details_truncated,
    })
}
pub(super) fn summarize_tool(name: &str, arguments: Option<&Value>) -> String {
    let arguments = arguments.unwrap_or(&Value::Null);
    let keys: &[&str] = match name {
        "bash" => &["command"],
        "read" | "write" | "edit" | "ls" => &["path", "file_path"],
        "grep" | "find" => &["pattern", "path"],
        _ => &[],
    };
    for key in keys {
        if let Some(value) = string_field(arguments, key) {
            return truncate_chars(value, 240);
        }
    }
    if arguments.is_null() {
        String::new()
    } else {
        truncate_chars(compact_json(arguments), 240)
    }
}
pub(super) fn extract_result_text(result: &Value) -> String {
    let text = if let Some(text) = result.as_str() {
        text.to_owned()
    } else if let Some(content) = result.get("content") {
        extract_result_text(content)
    } else if let Some(parts) = result.as_array() {
        parts
            .iter()
            .map(extract_result_text)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    } else if let Some(text) = result.get("text").and_then(Value::as_str) {
        text.to_owned()
    } else {
        compact_json(result)
    };
    truncate_chars(text, MAX_TOOL_OUTPUT_CHARS)
}
