use syntaxis_git::{
    BranchStatus, ChangeKind, FileChange, GitError, GitErrorCode, GitResult, RepositoryStatus,
};
use syntaxis_workspace::RelativePath;

pub(crate) fn parse_status(output: &[u8]) -> GitResult<RepositoryStatus> {
    let records = output.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut status = RepositoryStatus::default();
    let mut index = 0;

    while index < records.len() {
        let record = records[index];
        index += 1;
        if record.is_empty() {
            continue;
        }

        match record.first().copied() {
            Some(b'#') => parse_branch(record, &mut status.branch)?,
            Some(b'1') => status.changes.push(parse_ordinary(record)?),
            Some(b'2') => {
                let original = records.get(index).ok_or_else(parse_error)?;
                index += 1;
                status.changes.push(parse_renamed(record, original)?);
            }
            Some(b'u') => status.changes.push(parse_unmerged(record)?),
            Some(b'?') => status.changes.push(FileChange {
                path: parse_path(record.get(2..).ok_or_else(parse_error)?)?,
                original_path: None,
                index: None,
                worktree: Some(ChangeKind::Untracked),
                conflicted: false,
                staged_additions: 0,
                staged_deletions: 0,
                unstaged_additions: 0,
                unstaged_deletions: 0,
            }),
            Some(b'!') => {}
            _ => return Err(parse_error()),
        }
    }

    Ok(status)
}

fn parse_branch(record: &[u8], branch: &mut BranchStatus) -> GitResult<()> {
    let text = std::str::from_utf8(record).map_err(|_| parse_error())?;
    if let Some(value) = text.strip_prefix("# branch.oid ") {
        if value != "(initial)" {
            branch.oid = Some(value.to_owned());
        }
    } else if let Some(value) = text.strip_prefix("# branch.head ") {
        if value != "(detached)" {
            branch.head = Some(value.to_owned());
        }
    } else if let Some(value) = text.strip_prefix("# branch.upstream ") {
        branch.upstream = Some(value.to_owned());
    } else if let Some(value) = text.strip_prefix("# branch.ab ") {
        let mut counts = value.split_ascii_whitespace();
        branch.ahead = parse_count(counts.next(), '+')?;
        branch.behind = parse_count(counts.next(), '-')?;
    }
    Ok(())
}

fn parse_count(value: Option<&str>, prefix: char) -> GitResult<u32> {
    value
        .and_then(|value| value.strip_prefix(prefix))
        .and_then(|value| value.parse().ok())
        .ok_or_else(parse_error)
}

fn parse_ordinary(record: &[u8]) -> GitResult<FileChange> {
    let fields = fields(record, 9)?;
    let (index, worktree, conflicted) = parse_xy(fields[1])?;
    Ok(FileChange {
        path: parse_path(fields[8])?,
        original_path: None,
        index,
        worktree,
        conflicted,
        staged_additions: 0,
        staged_deletions: 0,
        unstaged_additions: 0,
        unstaged_deletions: 0,
    })
}

fn parse_renamed(record: &[u8], original: &[u8]) -> GitResult<FileChange> {
    let fields = fields(record, 10)?;
    let (index, worktree, conflicted) = parse_xy(fields[1])?;
    Ok(FileChange {
        path: parse_path(fields[9])?,
        original_path: Some(parse_path(original)?),
        index,
        worktree,
        conflicted,
        staged_additions: 0,
        staged_deletions: 0,
        unstaged_additions: 0,
        unstaged_deletions: 0,
    })
}

fn parse_unmerged(record: &[u8]) -> GitResult<FileChange> {
    let fields = fields(record, 11)?;
    Ok(FileChange {
        path: parse_path(fields[10])?,
        original_path: None,
        index: Some(ChangeKind::Unmerged),
        worktree: Some(ChangeKind::Unmerged),
        conflicted: true,
        staged_additions: 0,
        staged_deletions: 0,
        unstaged_additions: 0,
        unstaged_deletions: 0,
    })
}

fn fields(record: &[u8], expected: usize) -> GitResult<Vec<&[u8]>> {
    let fields = record
        .splitn(expected, |byte| *byte == b' ')
        .collect::<Vec<_>>();
    if fields.len() == expected {
        Ok(fields)
    } else {
        Err(parse_error())
    }
}

fn parse_xy(value: &[u8]) -> GitResult<(Option<ChangeKind>, Option<ChangeKind>, bool)> {
    if value.len() != 2 {
        return Err(parse_error());
    }
    let conflicted = matches!(value, b"DD" | b"AU" | b"UD" | b"UA" | b"DU" | b"AA" | b"UU");
    if conflicted {
        return Ok((Some(ChangeKind::Unmerged), Some(ChangeKind::Unmerged), true));
    }
    Ok((parse_kind(value[0])?, parse_kind(value[1])?, false))
}

fn parse_kind(value: u8) -> GitResult<Option<ChangeKind>> {
    match value {
        b'.' | b' ' => Ok(None),
        b'M' => Ok(Some(ChangeKind::Modified)),
        b'T' => Ok(Some(ChangeKind::TypeChanged)),
        b'A' => Ok(Some(ChangeKind::Added)),
        b'D' => Ok(Some(ChangeKind::Deleted)),
        b'R' => Ok(Some(ChangeKind::Renamed)),
        b'C' => Ok(Some(ChangeKind::Copied)),
        b'U' => Ok(Some(ChangeKind::Unmerged)),
        _ => Err(parse_error()),
    }
}

fn parse_path(value: &[u8]) -> GitResult<RelativePath> {
    let value = std::str::from_utf8(value).map_err(|_| parse_error())?;
    RelativePath::try_from(value).map_err(|_| parse_error())
}

fn parse_error() -> GitError {
    GitError::new(
        GitErrorCode::Parse,
        "Git returned repository status in an unexpected format.",
    )
}

#[cfg(test)]
mod tests {
    use super::parse_status;
    use syntaxis_git::ChangeKind;
    use syntaxis_workspace::RelativePath;

    #[test]
    fn parses_branch_counts_and_nul_delimited_renames() {
        let output = concat!(
            "# branch.oid abc123\0",
            "# branch.head feature/test\0",
            "# branch.upstream origin/feature/test\0",
            "# branch.ab +2 -3\0",
            "2 R. N... 100644 100644 100644 abc123 def456 R100 new name.rs\0",
            "old name.rs\0",
            "? untracked.txt\0",
        );
        let status = parse_status(output.as_bytes()).unwrap();

        assert_eq!(status.branch.head.as_deref(), Some("feature/test"));
        assert_eq!(status.branch.ahead, 2);
        assert_eq!(status.branch.behind, 3);
        assert_eq!(status.changes.len(), 2);
        assert_eq!(status.changes[0].index, Some(ChangeKind::Renamed));
        assert_eq!(status.changes[0].path.as_str(), "new name.rs");
        assert_eq!(
            status.changes[0]
                .original_path
                .as_ref()
                .map(RelativePath::as_str),
            Some("old name.rs")
        );
        assert_eq!(status.changes[1].worktree, Some(ChangeKind::Untracked));
    }
}
