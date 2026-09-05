use std::collections::BTreeSet;

use crate::{FilesPorts, SearchRequest};
use syntaxis_workspace::{RelativePath, WorkspaceRecord};

pub(super) use crate::{
    SearchOptions as WorkspaceSearchOptions, SearchResult as WorkspaceSearchResult,
    SearchResults as WorkspaceSearchResults, SearchScope,
};

const MAX_SEARCH_RESULTS: usize = 500;

pub async fn search_workspace_files(
    files: &FilesPorts,
    workspace: WorkspaceRecord,
    query: String,
    options: WorkspaceSearchOptions,
    ignored_paths: BTreeSet<String>,
    show_ignored: bool,
) -> Result<WorkspaceSearchResults, String> {
    let search = files.search();
    let ignored_paths = ignored_paths
        .into_iter()
        .map(RelativePath::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.message)?;
    search
        .search(
            &workspace,
            SearchRequest {
                query,
                options,
                ignored_paths,
                show_ignored,
                max_results: MAX_SEARCH_RESULTS,
            },
        )
        .await
        .map_err(|error| error.message)
}
