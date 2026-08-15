//! Bounded JSON and string helpers used by protocol mapping.

use serde_json::Value;

const MAX_STRUCTURED_DEPTH: usize = 6;
const MAX_STRUCTURED_ITEMS: usize = 64;
const MAX_STRUCTURED_STRING_CHARS: usize = 4 * 1024;

pub(super) fn bounded_json(value: Option<&Value>) -> (Option<Value>, bool) {
    let Some(value) = value else {
        return (None, false);
    };
    let mut truncated = false;
    (Some(bound_json_value(value, 0, &mut truncated)), truncated)
}

fn bound_json_value(value: &Value, depth: usize, truncated: &mut bool) -> Value {
    if depth >= MAX_STRUCTURED_DEPTH {
        *truncated = true;
        return Value::String("… nested data truncated …".into());
    }
    match value {
        Value::String(text) => {
            if text.chars().count() > MAX_STRUCTURED_STRING_CHARS {
                *truncated = true;
                Value::String(truncate_chars(text.clone(), MAX_STRUCTURED_STRING_CHARS))
            } else {
                value.clone()
            }
        }
        Value::Array(items) => {
            if items.len() > MAX_STRUCTURED_ITEMS {
                *truncated = true;
            }
            Value::Array(
                items
                    .iter()
                    .take(MAX_STRUCTURED_ITEMS)
                    .map(|item| bound_json_value(item, depth + 1, truncated))
                    .collect(),
            )
        }
        Value::Object(object) => {
            if object.len() > MAX_STRUCTURED_ITEMS {
                *truncated = true;
            }
            Value::Object(
                object
                    .iter()
                    .take(MAX_STRUCTURED_ITEMS)
                    .map(|(key, item)| {
                        (
                            truncate_chars(key.clone(), 256),
                            bound_json_value(item, depth + 1, truncated),
                        )
                    })
                    .collect(),
            )
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => value.clone(),
    }
}

pub(super) fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

pub(super) fn truncate_chars(mut text: String, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text;
    }
    let boundary = text
        .char_indices()
        .nth(max_chars.saturating_sub(1))
        .map_or(text.len(), |(index, _)| index);
    text.truncate(boundary);
    text.push('…');
    text
}

pub(super) fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}
