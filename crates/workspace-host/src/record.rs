use std::time::{SystemTime, UNIX_EPOCH};

use syntaxis_workspace::{WorkspaceError, WorkspaceResult};

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
