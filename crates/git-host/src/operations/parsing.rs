use std::path::Path;

use syntaxis_git::{
    BranchInfo, CommitInfo, GitError, GitErrorCode, GitResult, RepositoryStatus, TagInfo,
};
use syntaxis_workspace::RelativePath;

pub(super) fn parse_branches(output: &[u8]) -> GitResult<Vec<BranchInfo>> {
    let branches = output
        .split(|byte| *byte == b'\n')
        .filter(|record| !record.is_empty())
        .map(|record| {
            let fields = record.split(|byte| *byte == 0).collect::<Vec<_>>();
            if fields.len() != 4 {
                return Err(parse_error());
            }
            Ok(BranchInfo {
                name: parse_utf8(fields[0])?.to_owned(),
                current: fields[1] == b"*",
                upstream: match parse_utf8(fields[2])? {
                    "" => None,
                    upstream => Some(upstream.to_owned()),
                },
                remote: fields[3].starts_with(b"refs/remotes/"),
            })
        })
        .collect::<GitResult<Vec<_>>>()?;
    Ok(branches
        .into_iter()
        .filter(|branch| !branch.remote || branch.name.contains('/'))
        .collect())
}

pub(super) fn parse_path_numstat(output: &[u8]) -> GitResult<Vec<(RelativePath, u64, u64)>> {
    let records = output.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut stats = Vec::new();
    let mut index = 0;
    while index < records.len() {
        let record = records[index];
        index += 1;
        if record.is_empty() {
            continue;
        }
        let mut fields = record.splitn(3, |byte| *byte == b'\t');
        let additions = fields.next().ok_or_else(parse_error)?;
        let deletions = fields.next().ok_or_else(parse_error)?;
        let inline_path = fields.next().ok_or_else(parse_error)?;
        let path = if inline_path.is_empty() {
            index += 1;
            let renamed_path = records.get(index).ok_or_else(parse_error)?;
            index += 1;
            *renamed_path
        } else {
            inline_path
        };
        let additions = if additions == b"-" {
            0
        } else {
            parse_utf8(additions)?.parse().map_err(|_| parse_error())?
        };
        let deletions = if deletions == b"-" {
            0
        } else {
            parse_utf8(deletions)?.parse().map_err(|_| parse_error())?
        };
        stats.push((
            RelativePath::try_from(parse_utf8(path)?).map_err(|_| parse_error())?,
            additions,
            deletions,
        ));
    }
    Ok(stats)
}

pub(super) fn apply_path_stats(
    status: &mut RepositoryStatus,
    path_stats: &[(RelativePath, u64, u64)],
    staged: bool,
) {
    for (path, additions, deletions) in path_stats {
        let Some(change) = status
            .changes
            .iter_mut()
            .find(|change| change.path == *path || change.original_path.as_ref() == Some(path))
        else {
            continue;
        };
        if staged {
            change.staged_additions = change.staged_additions.saturating_add(*additions);
            change.staged_deletions = change.staged_deletions.saturating_add(*deletions);
        } else {
            change.unstaged_additions = change.unstaged_additions.saturating_add(*additions);
            change.unstaged_deletions = change.unstaged_deletions.saturating_add(*deletions);
        }
    }
}

pub(super) fn apply_untracked_stats(root: &Path, status: &mut RepositoryStatus, max_bytes: usize) {
    for change in &mut status.changes {
        if change.worktree != Some(syntaxis_git::ChangeKind::Untracked) {
            continue;
        }
        let path = root.join(change.path.as_str());
        let Ok(metadata) = path.symlink_metadata() else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > u64::try_from(max_bytes).unwrap_or(u64::MAX) {
            continue;
        }
        let Ok(contents) = std::fs::read(path) else {
            continue;
        };
        if contents.contains(&0) {
            continue;
        }
        let lines = contents
            .split(|byte| *byte == b'\n')
            .count()
            .saturating_sub(usize::from(contents.last() == Some(&b'\n')));
        change.unstaged_additions = lines.try_into().unwrap_or(u64::MAX);
    }
}

pub(super) fn parse_tags(output: &[u8]) -> GitResult<Vec<TagInfo>> {
    output
        .split(|byte| *byte == b'\n')
        .filter(|record| !record.is_empty())
        .map(|record| {
            let fields = record.split(|byte| *byte == 0).collect::<Vec<_>>();
            if fields.len() != 4 {
                return Err(parse_error());
            }
            let annotated = fields[1] == b"tag";
            let target = if annotated { fields[3] } else { fields[2] };
            if target.is_empty() {
                return Err(parse_error());
            }
            Ok(TagInfo {
                name: parse_utf8(fields[0])?.to_owned(),
                target_oid: parse_utf8(target)?.to_owned(),
                annotated,
            })
        })
        .collect()
}

pub(super) fn parse_history(output: &[u8]) -> GitResult<Vec<CommitInfo>> {
    output
        .split(|byte| *byte == 0)
        .map(trim_ascii_end)
        .filter(|record| !record.is_empty())
        .map(parse_commit_record)
        .collect()
}

pub(super) fn parse_commit_record(record: &[u8]) -> GitResult<CommitInfo> {
    let fields = record.split(|byte| *byte == 0x1f).collect::<Vec<_>>();
    if fields.len() != 7 {
        return Err(parse_error());
    }
    Ok(CommitInfo {
        oid: parse_utf8(fields[0])?.to_owned(),
        short_oid: parse_utf8(fields[1])?.to_owned(),
        parents: parse_utf8(fields[2])?
            .split_ascii_whitespace()
            .map(ToOwned::to_owned)
            .collect(),
        author_name: parse_utf8(fields[3])?.to_owned(),
        author_email: parse_utf8(fields[4])?.to_owned(),
        authored_unix_seconds: parse_utf8(fields[5])?.parse().map_err(|_| parse_error())?,
        subject: parse_utf8(fields[6])?.to_owned(),
    })
}

pub(super) fn parse_numstat(output: &[u8]) -> GitResult<(u32, u64, u64)> {
    let text = parse_utf8(output)?;
    let mut files = 0_u32;
    let mut additions = 0_u64;
    let mut deletions = 0_u64;
    for line in text.lines().filter(|line| !line.is_empty()) {
        let mut fields = line.splitn(3, '\t');
        let added = fields.next().ok_or_else(parse_error)?;
        let deleted = fields.next().ok_or_else(parse_error)?;
        fields.next().ok_or_else(parse_error)?;
        files = files.saturating_add(1);
        if added != "-" {
            additions = additions.saturating_add(added.parse().map_err(|_| parse_error())?);
        }
        if deleted != "-" {
            deletions = deletions.saturating_add(deleted.parse().map_err(|_| parse_error())?);
        }
    }
    Ok((files, additions, deletions))
}

pub(super) fn parse_comparison_counts(output: &[u8]) -> GitResult<(u32, u32)> {
    let mut fields = parse_utf8(output)?.split_ascii_whitespace();
    let base_only = fields
        .next()
        .ok_or_else(parse_error)?
        .parse()
        .map_err(|_| parse_error())?;
    let head_only = fields
        .next()
        .ok_or_else(parse_error)?
        .parse()
        .map_err(|_| parse_error())?;
    if fields.next().is_some() {
        return Err(parse_error());
    }
    Ok((base_only, head_only))
}

pub(super) fn trim_ascii_end(mut value: &[u8]) -> &[u8] {
    while value.last().is_some_and(u8::is_ascii_whitespace) {
        value = &value[..value.len() - 1];
    }
    value
}

pub(super) fn parse_utf8(value: &[u8]) -> GitResult<&str> {
    std::str::from_utf8(value).map_err(|_| parse_error())
}

pub(super) fn parse_error() -> GitError {
    GitError::new(
        GitErrorCode::Parse,
        "Git returned data in an unexpected format.",
    )
}
