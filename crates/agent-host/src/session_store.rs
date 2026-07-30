use std::{
    env, fs,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use regex::RegexBuilder;
use serde_json::Value;
use syntaxis_agent::{ConversationMatchRole, ConversationSearchResult};

const HEAD_BYTES: usize = 64 * 1024;
const TAIL_BYTES: usize = 256 * 1024;
const MAX_SEARCH_RESULTS: usize = 30;
const SNIPPET_CONTEXT_CHARS: usize = 60;

#[derive(Clone, Debug)]
pub(crate) struct PersistedSession {
    pub(crate) id: String,
    pub(crate) path: PathBuf,
    pub(crate) title: String,
    pub(crate) updated_at_ms: u64,
}

pub(crate) fn discover(workspace_root: &Path) -> Vec<PersistedSession> {
    let root = session_root(workspace_root);
    let expected_cwd = canonical_or_owned(workspace_root);
    let mut files = Vec::new();
    walk_jsonl(&root, &mut files);
    let mut sessions = files
        .into_iter()
        .filter_map(|path| read_descriptor(&path, &expected_cwd))
        .collect::<Vec<_>>();
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at_ms));
    sessions
}

pub(crate) fn search(workspace_root: &Path, query: &str) -> Vec<ConversationSearchResult> {
    let query = query.trim();
    if query.chars().count() < 2 {
        return Vec::new();
    }
    let Ok(pattern) = RegexBuilder::new(&regex::escape(query))
        .case_insensitive(true)
        .build()
    else {
        return Vec::new();
    };
    discover(workspace_root)
        .into_iter()
        .filter_map(|session| search_session(&session, &pattern))
        .take(MAX_SEARCH_RESULTS)
        .collect()
}

fn search_session(
    session: &PersistedSession,
    pattern: &regex::Regex,
) -> Option<ConversationSearchResult> {
    let file = fs::File::open(&session.path).ok()?;
    let mut first_match = None;
    let mut match_count = 0;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if entry.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let Some(message) = entry.get("message") else {
            continue;
        };
        let role = match message.get("role").and_then(Value::as_str) {
            Some("user") => ConversationMatchRole::User,
            Some("assistant") => ConversationMatchRole::Assistant,
            _ => continue,
        };
        let Some(content) = message.get("content") else {
            continue;
        };
        let Some(text) = message_text(content) else {
            continue;
        };
        let Some(found) = pattern.find(&text) else {
            continue;
        };
        match_count += 1;
        first_match.get_or_insert_with(|| (role, snippet(&text, found.start(), found.end())));
    }
    let (role, snippet) = first_match?;
    Some(ConversationSearchResult {
        session_id: session.id.clone(),
        title: session.title.clone(),
        updated_at_ms: session.updated_at_ms,
        role,
        snippet,
        match_count,
    })
}

fn snippet(text: &str, match_start: usize, match_end: usize) -> String {
    let start = text
        .get(..match_start)
        .expect("regex match starts are UTF-8 boundaries")
        .char_indices()
        .rev()
        .nth(SNIPPET_CONTEXT_CHARS)
        .map_or(0, |(index, _)| index);
    let end = text
        .get(match_end..)
        .expect("regex match ends are UTF-8 boundaries")
        .char_indices()
        .nth(SNIPPET_CONTEXT_CHARS)
        .map_or(text.len(), |(index, _)| match_end + index);
    let flattened = text
        .get(start..end)
        .expect("snippet offsets are derived from character boundaries")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let prefix = if start > 0 { "…" } else { "" };
    let suffix = if end < text.len() { "…" } else { "" };
    format!("{prefix}{flattened}{suffix}")
}

pub(crate) fn delete(workspace_root: &Path, session_id: &str, path: &Path) -> std::io::Result<()> {
    let expected_cwd = canonical_or_owned(workspace_root);
    let descriptor = read_descriptor(path, &expected_cwd).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "The Pi session file does not belong to this workspace",
        )
    })?;
    if descriptor.id != session_id {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "The Pi session id does not match its session file",
        ));
    }
    fs::remove_file(path)
}

fn session_root(workspace_root: &Path) -> PathBuf {
    let home = env::var_os("HOME").map(PathBuf::from);
    let agent_dir = env::var_os("PI_CODING_AGENT_DIR")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".pi/agent")))
        .unwrap_or_else(|| PathBuf::from(".pi/agent"));
    if let Some(path) = env::var_os("PI_CODING_AGENT_SESSION_DIR") {
        return resolve_path(Path::new(&path), workspace_root, home.as_deref());
    }
    for settings in [
        workspace_root.join(".pi/settings.json"),
        agent_dir.join("settings.json"),
    ] {
        if let Some(configured) = configured_session_dir(&settings) {
            return resolve_path(Path::new(&configured), workspace_root, home.as_deref());
        }
    }
    agent_dir.join("sessions")
}

fn configured_session_dir(path: &Path) -> Option<String> {
    let value: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    value
        .get("sessionDir")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
}

fn resolve_path(path: &Path, base: &Path, home: Option<&Path>) -> PathBuf {
    let text = path.to_string_lossy();
    if text == "~" {
        return home.unwrap_or(base).to_owned();
    }
    if let Some(suffix) = text.strip_prefix("~/") {
        return home.unwrap_or(base).join(suffix);
    }
    if path.is_absolute() {
        path.to_owned()
    } else {
        base.join(path)
    }
}

fn walk_jsonl(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            walk_jsonl(&path, files);
        } else if kind.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "jsonl")
        {
            files.push(path);
        }
    }
}

fn read_descriptor(path: &Path, expected_cwd: &Path) -> Option<PersistedSession> {
    let head = read_head(path)?;
    let first_line = head.lines().next()?;
    let header: Value = serde_json::from_str(first_line).ok()?;
    if header.get("type").and_then(Value::as_str) != Some("session") {
        return None;
    }
    let id = string(&header, "id")?;
    let cwd = canonical_or_owned(Path::new(&string(&header, "cwd")?));
    if cwd != expected_cwd {
        return None;
    }
    let tail = read_tail(path).unwrap_or_default();
    let title = explicit_title(&tail)
        .or_else(|| explicit_title(&head))
        .or_else(|| first_user_message(&head))
        .unwrap_or_else(|| "New chat".into());
    let updated_at_ms = fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or_default();
    Some(PersistedSession {
        id,
        path: path.to_owned(),
        title: preview(&title),
        updated_at_ms,
    })
}

fn read_head(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut bytes = vec![0; HEAD_BYTES];
    let count = file.read(&mut bytes).ok()?;
    bytes.truncate(count);
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn read_tail(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let tail = u64::try_from(TAIL_BYTES).ok()?;
    file.seek(SeekFrom::Start(size.saturating_sub(tail))).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn explicit_title(content: &str) -> Option<String> {
    content.lines().rev().find_map(|line| {
        let value: Value = serde_json::from_str(line).ok()?;
        (value.get("type").and_then(Value::as_str) == Some("session_info"))
            .then(|| string(&value, "name"))
            .flatten()
    })
}

fn first_user_message(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let value: Value = serde_json::from_str(line).ok()?;
        if value.get("type").and_then(Value::as_str) != Some("message") {
            return None;
        }
        let message = value.get("message")?;
        if message.get("role").and_then(Value::as_str) != Some("user") {
            return None;
        }
        message_text(message.get("content")?)
    })
}

fn message_text(content: &Value) -> Option<String> {
    if let Some(text) = content.as_str() {
        return non_empty(text);
    }
    let text = content
        .as_array()?
        .iter()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    non_empty(&text)
}

fn string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).and_then(non_empty)
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    (!value.is_empty()).then_some(value)
}

fn preview(value: &str) -> String {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = value.chars();
    let title = chars.by_ref().take(80).collect::<String>();
    if chars.next().is_some() {
        format!("{title}…")
    } else {
        title
    }
}

fn canonical_or_owned(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn reads_pi_session_identity_and_title() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        let path = temp.path().join("session.jsonl");
        let mut file = fs::File::create(&path).unwrap();
        writeln!(
            file,
            "{}",
            serde_json::json!({"type":"session","id":"session-1","cwd":workspace})
        )
        .unwrap();
        writeln!(file, "{}", serde_json::json!({"type":"message","message":{"role":"user","content":"Inspect the project"}})).unwrap();
        writeln!(
            file,
            "{}",
            serde_json::json!({"type":"session_info","name":"Project review"})
        )
        .unwrap();

        let session = read_descriptor(&path, &workspace.canonicalize().unwrap()).unwrap();
        assert_eq!(session.id, "session-1");
        assert_eq!(session.title, "Project review");
    }

    #[test]
    fn only_deletes_a_matching_workspace_session() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let other_workspace = temp.path().join("other");
        fs::create_dir(&workspace).unwrap();
        fs::create_dir(&other_workspace).unwrap();
        let path = temp.path().join("session.jsonl");
        let mut file = fs::File::create(&path).unwrap();
        writeln!(
            file,
            "{}",
            serde_json::json!({"type":"session","id":"session-1","cwd":workspace})
        )
        .unwrap();

        assert!(delete(&other_workspace, "session-1", &path).is_err());
        assert!(path.exists());
        assert!(delete(&workspace, "different-session", &path).is_err());
        assert!(path.exists());
        delete(&workspace, "session-1", &path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn searches_only_user_and_assistant_text() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.jsonl");
        let mut file = fs::File::create(&path).unwrap();
        writeln!(file, "{}", serde_json::json!({"type":"message","message":{"role":"toolResult","content":"Needle in tool output"}})).unwrap();
        writeln!(file, "{}", serde_json::json!({"type":"message","message":{"role":"user","content":"Find the Needle here"}})).unwrap();
        writeln!(file, "{}", serde_json::json!({"type":"message","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Needle in reasoning"},{"type":"text","text":"Another needle"}]}})).unwrap();
        let session = PersistedSession {
            id: "session-1".into(),
            path,
            title: "Search test".into(),
            updated_at_ms: 42,
        };
        let pattern = RegexBuilder::new("needle")
            .case_insensitive(true)
            .build()
            .unwrap();

        let result = search_session(&session, &pattern).unwrap();

        assert_eq!(result.role, ConversationMatchRole::User);
        assert_eq!(result.snippet, "Find the Needle here");
        assert_eq!(result.match_count, 2);
    }
}
