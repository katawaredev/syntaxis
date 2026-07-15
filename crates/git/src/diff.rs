use serde::{Deserialize, Serialize};
use syntaxis_workspace::RelativePath;

use crate::{GitError, GitErrorCode, GitResult};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    Staged,
    Worktree,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UnifiedDiff {
    pub path: RelativePath,
    pub kind: DiffKind,
    pub patch: String,
    pub binary: bool,
    /// Text on the original side of the comparison, when it can be displayed safely.
    #[serde(default)]
    pub original: Option<String>,
    /// Text on the changed side of the comparison, when it can be displayed safely.
    #[serde(default)]
    pub current: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiffHunk {
    pub index: usize,
    pub header: String,
    pub body: String,
    pub old_start: usize,
    pub new_start: usize,
    /// Complete single-file patch containing only this hunk.
    pub patch: String,
    pub fingerprint: u64,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HunkAction {
    Stage,
    Unstage,
    Discard,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HunkRequest {
    pub path: RelativePath,
    pub kind: DiffKind,
    pub hunk_index: usize,
    pub expected_fingerprint: u64,
    pub action: HunkAction,
}

/// Splits a single-file unified Git patch into independently applicable hunks.
///
/// # Errors
///
/// Returns an unsupported error for binary, rename, or copy patches, and a parse
/// error when the input does not have the expected unified-diff structure.
pub fn parse_diff_hunks(patch: &str) -> GitResult<Vec<DiffHunk>> {
    if patch.contains("GIT binary patch") || patch.contains("Binary files ") {
        return Err(unsupported_hunks());
    }
    if patch.lines().any(|line| {
        line.starts_with("rename from ")
            || line.starts_with("rename to ")
            || line.starts_with("copy from ")
            || line.starts_with("copy to ")
    }) {
        return Err(unsupported_hunks());
    }

    let lines = patch.split_inclusive('\n').collect::<Vec<_>>();
    let Some(first_hunk) = lines.iter().position(|line| line.starts_with("@@ ")) else {
        return Ok(Vec::new());
    };
    let preamble = lines[..first_hunk].concat();
    if preamble.is_empty() || !preamble.starts_with("diff --git ") {
        return Err(parse_error());
    }

    let mut starts = lines
        .iter()
        .enumerate()
        .skip(first_hunk)
        .filter_map(|(index, line)| line.starts_with("@@ ").then_some(index))
        .collect::<Vec<_>>();
    starts.push(lines.len());
    let mut hunks = Vec::with_capacity(starts.len().saturating_sub(1));
    for window in starts.windows(2) {
        let start = window[0];
        let end = window[1];
        if lines[start + 1..end]
            .iter()
            .any(|line| line.starts_with("diff --git "))
        {
            return Err(parse_error());
        }
        let header = lines[start].trim_end_matches(['\r', '\n']).to_owned();
        let (old_start, new_start) = hunk_starts(&header).ok_or_else(parse_error)?;
        let body = lines[start..end].concat();
        let additions = lines[start + 1..end]
            .iter()
            .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
            .count()
            .try_into()
            .unwrap_or(u32::MAX);
        let deletions = lines[start + 1..end]
            .iter()
            .filter(|line| line.starts_with('-') && !line.starts_with("---"))
            .count()
            .try_into()
            .unwrap_or(u32::MAX);
        let patch = format!("{preamble}{body}");
        hunks.push(DiffHunk {
            index: hunks.len(),
            header,
            fingerprint: patch_fingerprint(&patch),
            patch,
            body,
            old_start,
            new_start,
            additions,
            deletions,
        });
    }
    Ok(hunks)
}

fn patch_fingerprint(patch: &str) -> u64 {
    patch
        .as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

fn hunk_starts(header: &str) -> Option<(usize, usize)> {
    let ranges = header.strip_prefix("@@ -")?.split_once(" @@")?.0;
    let (old, new) = ranges.split_once(" +")?;
    let start = |range: &str| {
        range
            .split_once(',')
            .map_or(range, |(start, _)| start)
            .parse()
            .ok()
    };
    Some((start(old)?, start(new)?))
}

fn parse_error() -> GitError {
    GitError::new(
        GitErrorCode::Parse,
        "Git returned a unified diff in an unexpected format.",
    )
}

fn unsupported_hunks() -> GitError {
    GitError::new(
        GitErrorCode::Unsupported,
        "Hunk actions are unavailable for this type of Git change.",
    )
}

#[cfg(test)]
mod tests {
    use super::parse_diff_hunks;

    #[test]
    fn preserves_file_headers_and_splits_multiple_hunks() {
        let patch = concat!(
            "diff --git a/file.txt b/file.txt\n",
            "index 1111111..2222222 100644\n",
            "--- a/file.txt\n",
            "+++ b/file.txt\n",
            "@@ -1,2 +1,2 @@\n",
            "-old one\n",
            "+new one\n",
            " context\n",
            "@@ -20 +20 @@\n",
            "-old two\n",
            "+new two\n",
        );
        let hunks = parse_diff_hunks(patch).unwrap();
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].additions, 1);
        assert_eq!(hunks[0].deletions, 1);
        assert_eq!((hunks[0].old_start, hunks[0].new_start), (1, 1));
        assert_eq!((hunks[1].old_start, hunks[1].new_start), (20, 20));
        assert!(hunks[0].patch.contains("diff --git a/file.txt b/file.txt"));
        assert!(!hunks[0].patch.contains("@@ -20 +20 @@"));
        assert!(hunks[1].patch.contains("@@ -20 +20 @@"));
    }
}
