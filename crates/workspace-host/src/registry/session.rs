use std::collections::HashSet;

use syntaxis_workspace::{RelativePath, WorkspaceSession};

pub(super) fn sanitize_session(session: &mut WorkspaceSession) {
    let mut seen = HashSet::new();
    session.files.tabs.retain(|path| {
        seen.insert(path.clone())
            && RelativePath::try_from(path.clone()).is_ok_and(|path| !path.is_root())
    });
    session.files.tabs.truncate(20);
    if session
        .files
        .active
        .as_ref()
        .is_some_and(|active| !session.files.tabs.contains(active))
    {
        session.files.active = None;
    }
}
