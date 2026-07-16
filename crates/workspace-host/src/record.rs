use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use syntaxis_workspace::{
    ErrorCode, WorkspaceAvailability, WorkspaceError, WorkspaceIcon, WorkspaceIconSymbol,
    WorkspaceId, WorkspaceRecord, WorkspaceResult,
};

pub(crate) fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceRecord> {
    let root: String = row.get(3)?;
    let icon: String = row.get(4)?;
    Ok(WorkspaceRecord {
        id: WorkspaceId::new(row.get::<_, String>(0)?),
        slug: row.get(1)?,
        name: row.get(2)?,
        availability: if Path::new(&root).is_dir() {
            WorkspaceAvailability::Available
        } else {
            WorkspaceAvailability::Missing
        },
        root,
        icon: serde_json::from_str(&icon).unwrap_or(WorkspaceIcon::Symbol {
            name: WorkspaceIconSymbol::Folder,
        }),
        profile: row
            .get::<_, String>(7)
            .ok()
            .and_then(|profile| serde_json::from_str(&profile).ok())
            .unwrap_or_default(),
        registered_at_unix_ms: row.get(5)?,
        last_opened_unix_ms: row.get(6)?,
    })
}

pub(crate) fn slugify(name: &str) -> String {
    let slug = name
        .chars()
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "workspace".to_owned()
    } else {
        slug.to_owned()
    }
}

pub(crate) fn unix_milliseconds() -> WorkspaceResult<i64> {
    let milliseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WorkspaceError::internal())?
        .as_millis();
    i64::try_from(milliseconds).map_err(|_| WorkspaceError::internal())
}

pub(crate) fn require_changed(changed: usize) -> WorkspaceResult<()> {
    if changed == 0 {
        Err(WorkspaceError::new(
            ErrorCode::NotFound,
            "The workspace is no longer registered.",
        ))
    } else {
        Ok(())
    }
}
