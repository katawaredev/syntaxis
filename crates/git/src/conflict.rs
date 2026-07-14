use serde::{Deserialize, Serialize};
use syntaxis_workspace::RelativePath;

use crate::{GitError, GitErrorCode, GitResult};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConflictFile {
    pub path: RelativePath,
    pub blocks: Vec<ConflictBlock>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConflictBlock {
    pub index: usize,
    pub current_label: String,
    pub incoming_label: String,
    pub current: String,
    pub incoming: String,
    pub fingerprint: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictChoice {
    Current,
    Incoming,
    Both,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConflictRequest {
    pub path: RelativePath,
    pub block_index: usize,
    pub expected_fingerprint: u64,
    pub choice: ConflictChoice,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedConflict {
    pub content: String,
    pub complete: bool,
}

/// Parses standard two-way or diff3 conflict markers for display.
///
/// # Errors
///
/// Returns an unsupported error when markers are absent or malformed.
pub fn parse_conflict_file(path: RelativePath, content: &str) -> GitResult<ConflictFile> {
    let blocks = parse_blocks(content)?;
    if blocks.is_empty() {
        return Err(unsupported_conflict());
    }
    Ok(ConflictFile {
        path,
        blocks: blocks
            .into_iter()
            .enumerate()
            .map(|(index, block)| block.public(index))
            .collect(),
    })
}

/// Resolves one standard conflict block while preserving all other file content.
///
/// # Errors
///
/// Returns a conflict error if the selected block is stale or missing, and an
/// unsupported error if the file's conflict markers are malformed.
pub fn resolve_conflict_block(
    content: &str,
    block_index: usize,
    expected_fingerprint: u64,
    choice: ConflictChoice,
) -> GitResult<ResolvedConflict> {
    let blocks = parse_blocks(content)?;
    let block = blocks.get(block_index).ok_or_else(stale_conflict)?;
    if block.fingerprint() != expected_fingerprint {
        return Err(stale_conflict());
    }
    let replacement = match choice {
        ConflictChoice::Current => block.current.as_str(),
        ConflictChoice::Incoming => block.incoming.as_str(),
        ConflictChoice::Both => return Ok(resolve_both(content, block, &blocks)),
    };
    Ok(resolved_document(content, block, replacement, &blocks))
}

fn resolve_both(content: &str, block: &ParsedBlock, blocks: &[ParsedBlock]) -> ResolvedConflict {
    let replacement = format!("{}{}", block.current, block.incoming);
    resolved_document(content, block, &replacement, blocks)
}

fn resolved_document(
    content: &str,
    block: &ParsedBlock,
    replacement: &str,
    blocks: &[ParsedBlock],
) -> ResolvedConflict {
    let mut resolved = String::with_capacity(content.len());
    resolved.push_str(&content[..block.start]);
    resolved.push_str(replacement);
    resolved.push_str(&content[block.end..]);
    ResolvedConflict {
        content: resolved,
        complete: blocks.len() == 1,
    }
}

#[derive(Debug)]
struct ParsedBlock {
    start: usize,
    end: usize,
    current_label: String,
    incoming_label: String,
    current: String,
    incoming: String,
    source: String,
}

impl ParsedBlock {
    fn fingerprint(&self) -> u64 {
        self.source
            .as_bytes()
            .iter()
            .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
            })
    }

    fn public(&self, index: usize) -> ConflictBlock {
        ConflictBlock {
            index,
            current_label: self.current_label.clone(),
            incoming_label: self.incoming_label.clone(),
            current: self.current.clone(),
            incoming: self.incoming.clone(),
            fingerprint: self.fingerprint(),
        }
    }
}

fn parse_blocks(content: &str) -> GitResult<Vec<ParsedBlock>> {
    let lines = content.split_inclusive('\n').collect::<Vec<_>>();
    let offsets = lines
        .iter()
        .scan(0, |offset, line| {
            let current = *offset;
            *offset += line.len();
            Some(current)
        })
        .collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if !lines[index].starts_with("<<<<<<< ") {
            if is_marker(lines[index]) {
                return Err(unsupported_conflict());
            }
            index += 1;
            continue;
        }
        let start_line = index;
        let current_label = marker_label(lines[index], "<<<<<<< ");
        index += 1;
        let current_start = index;
        while index < lines.len()
            && !lines[index].starts_with("||||||| ")
            && !lines[index].starts_with("=======")
        {
            if is_marker(lines[index]) {
                return Err(unsupported_conflict());
            }
            index += 1;
        }
        let current = lines[current_start..index].concat();
        if index < lines.len() && lines[index].starts_with("||||||| ") {
            index += 1;
            while index < lines.len() && !lines[index].starts_with("=======") {
                if is_marker(lines[index]) {
                    return Err(unsupported_conflict());
                }
                index += 1;
            }
        }
        if index >= lines.len() || !lines[index].starts_with("=======") {
            return Err(unsupported_conflict());
        }
        index += 1;
        let incoming_start = index;
        while index < lines.len() && !lines[index].starts_with(">>>>>>> ") {
            if is_marker(lines[index]) {
                return Err(unsupported_conflict());
            }
            index += 1;
        }
        if index >= lines.len() {
            return Err(unsupported_conflict());
        }
        let incoming = lines[incoming_start..index].concat();
        let incoming_label = marker_label(lines[index], ">>>>>>> ");
        index += 1;
        let start = offsets[start_line];
        let end = if index == lines.len() {
            content.len()
        } else {
            offsets[index]
        };
        blocks.push(ParsedBlock {
            start,
            end,
            current_label,
            incoming_label,
            current,
            incoming,
            source: content[start..end].to_owned(),
        });
    }
    Ok(blocks)
}

fn marker_label(line: &str, prefix: &str) -> String {
    line.strip_prefix(prefix)
        .unwrap_or_default()
        .trim_end_matches(['\r', '\n'])
        .to_owned()
}

fn is_marker(line: &str) -> bool {
    line.starts_with("<<<<<<<")
        || line.starts_with("|||||||")
        || line.starts_with("=======")
        || line.starts_with(">>>>>>>")
}

fn unsupported_conflict() -> GitError {
    GitError::new(
        GitErrorCode::Unsupported,
        "This conflict does not use supported standard text markers. Resolve it with an external Git tool.",
    )
}

fn stale_conflict() -> GitError {
    GitError::new(
        GitErrorCode::Conflict,
        "The selected conflict block changed. Refresh it before resolving.",
    )
}

#[cfg(test)]
mod tests {
    use syntaxis_workspace::RelativePath;

    use super::{parse_conflict_file, resolve_conflict_block, ConflictChoice};

    #[test]
    fn resolves_blocks_independently_and_preserves_surrounding_text() {
        let source = concat!(
            "before\n<<<<<<< HEAD\ncurrent one\n=======\nincoming one\n>>>>>>> feature\n",
            "middle\n<<<<<<< HEAD\ncurrent two\n=======\nincoming two\n>>>>>>> feature\nafter\n",
        );
        let parsed =
            parse_conflict_file(RelativePath::try_from("file.txt").unwrap(), source).unwrap();
        assert_eq!(parsed.blocks.len(), 2);
        let resolved = resolve_conflict_block(
            source,
            0,
            parsed.blocks[0].fingerprint,
            ConflictChoice::Incoming,
        )
        .unwrap();
        assert!(!resolved.complete);
        assert!(resolved.content.contains("incoming one\nmiddle"));
        assert!(resolved.content.contains("<<<<<<< HEAD\ncurrent two"));
    }

    #[test]
    fn rejects_malformed_and_stale_conflict_blocks() {
        let malformed = "<<<<<<< HEAD\ncurrent\n>>>>>>> feature\n";
        assert!(
            parse_conflict_file(RelativePath::try_from("file.txt").unwrap(), malformed).is_err()
        );

        let source = "<<<<<<< HEAD\ncurrent\n=======\nincoming\n>>>>>>> feature\n";
        let parsed =
            parse_conflict_file(RelativePath::try_from("file.txt").unwrap(), source).unwrap();
        assert!(resolve_conflict_block(
            source,
            0,
            parsed.blocks[0].fingerprint ^ 1,
            ConflictChoice::Current,
        )
        .is_err());
    }
}
