//! Pure text operations for browser Pi tools, also exercised by native unit tests.

use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, RetryAdvice};

pub(crate) fn invalid(message: &str) -> AppError {
    AppError::new(
        AppErrorCode::InvalidInput,
        message,
        RetryAdvice::Never,
        ErrorSource::Ai,
    )
}

pub(crate) fn argument<'a>(args: &'a serde_json::Value, name: &str) -> Result<&'a str, AppError> {
    args.get(name)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid("A required tool argument is missing."))
}

fn line_argument(args: &serde_json::Value, name: &str) -> Result<Option<usize>, AppError> {
    match args.get(name) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value > 0)
            .map(Some)
            .ok_or_else(|| invalid("offset and limit must be positive integers.")),
    }
}

pub(crate) fn read_range(content: &str, args: &serde_json::Value) -> Result<String, AppError> {
    let offset = line_argument(args, "offset")?;
    let limit = line_argument(args, "limit")?;
    if offset.is_none() && limit.is_none() {
        return Ok(content.to_owned());
    }
    let lines = content.lines().collect::<Vec<_>>();
    let start = offset.unwrap_or(1) - 1;
    if start >= lines.len() {
        return Err(invalid("offset is past the end of the file."));
    }
    let end = start
        .saturating_add(limit.unwrap_or(lines.len()))
        .min(lines.len());
    let selected = lines[start..end]
        .iter()
        .enumerate()
        .map(|(index, line)| format!("{}: {line}", start + index + 1))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "{selected}\n[Lines {}-{end} of {}]",
        start + 1,
        lines.len()
    ))
}

pub(crate) fn apply_edits(content: &str, args: &serde_json::Value) -> Result<String, AppError> {
    let edits = if let Some(edits) = args.get("edits").filter(|value| !value.is_null()) {
        if ["old_text", "new_text"]
            .iter()
            .any(|name| args.get(*name).is_some_and(|value| !value.is_null()))
        {
            return Err(invalid("Use either edits or old_text/new_text, not both."));
        }
        let edits = edits
            .as_array()
            .ok_or_else(|| invalid("edits must be an array."))?;
        if edits.is_empty() || edits.len() > 100 {
            return Err(invalid(
                "edits must contain between 1 and 100 replacements.",
            ));
        }
        edits.iter().collect::<Vec<_>>()
    } else {
        vec![args]
    };
    let mut updated = content.to_owned();
    for edit in edits {
        let old = argument(edit, "old_text")?;
        let new = argument(edit, "new_text")?;
        if old.is_empty() || updated.matches(old).count() != 1 {
            return Err(invalid(
                "old_text must match exactly once. Read the file and retry.",
            ));
        }
        updated = updated.replacen(old, new, 1);
        if updated.len() > 256 * 1024 {
            return Err(invalid("File exceeds 256 KiB."));
        }
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ranged_reads_number_lines_and_keep_full_reads_unchanged() {
        let content = "one\nสอง\nthree\nfour\n";
        assert_eq!(read_range(content, &json!({})).unwrap(), content);
        assert_eq!(
            read_range(content, &json!({"offset": 2, "limit": 2})).unwrap(),
            "2: สอง\n3: three\n[Lines 2-3 of 4]"
        );
        assert_eq!(
            read_range(content, &json!({"limit": 1})).unwrap(),
            "1: one\n[Lines 1-1 of 4]"
        );
        read_range(content, &json!({"offset": 0})).unwrap_err();
        read_range(content, &json!({"limit": -1})).unwrap_err();
        read_range(content, &json!({"offset": 5})).unwrap_err();
    }

    #[test]
    fn batch_edits_validate_every_replacement_and_reject_conflicts() {
        let original = "alpha beta";
        let edits = json!({"edits": [
            {"old_text": "alpha", "new_text": "gamma"},
            {"old_text": "gamma beta", "new_text": "done"}
        ]});
        assert_eq!(apply_edits(original, &edits).unwrap(), "done");
        let failed = json!({"edits": [
            {"old_text": "alpha", "new_text": "gamma"},
            {"old_text": "missing", "new_text": "done"}
        ]});
        apply_edits(original, &failed).unwrap_err();
        apply_edits(
            "duplicate duplicate",
            &json!({
                "old_text": "duplicate", "new_text": "changed"
            }),
        )
        .unwrap_err();
        apply_edits(original, &json!({"edits": []})).unwrap_err();
        assert_eq!(
            apply_edits(
                original,
                &json!({
                    "old_text": "alpha", "new_text": "gamma", "edits": null
                })
            )
            .unwrap(),
            "gamma beta"
        );
        assert_eq!(
            apply_edits(
                original,
                &json!({
                    "edits": [{"old_text": "alpha", "new_text": "gamma"}],
                    "old_text": null, "new_text": null
                })
            )
            .unwrap(),
            "gamma beta"
        );
        apply_edits(
            original,
            &json!({
                "edits": [{"old_text": "alpha", "new_text": "gamma"}],
                "old_text": "alpha", "new_text": "gamma"
            }),
        )
        .unwrap_err();
    }

    #[test]
    fn edits_enforce_the_file_byte_limit() {
        let replacement = "ก".repeat(100_000);
        apply_edits(
            "small",
            &json!({
                "old_text": "small", "new_text": replacement
            }),
        )
        .unwrap_err();
    }
}
