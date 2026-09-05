use dioxus::prelude::*;

pub use syntaxis_module_files::FilesQuery;
pub(crate) use syntaxis_module_files::{
    SearchOptions as WorkspaceSearchOptions, SearchScope, search_workspace_files,
};

#[component]
pub fn Files(slug: String, query: FilesQuery) -> Element {
    let active = use_context::<crate::workspace::ActiveWorkspace>();
    let navigator = use_navigator();
    let route_slug = slug.clone();
    let on_navigate = EventHandler::new(move |query| {
        navigator.replace(crate::app::Route::Files {
            slug: route_slug.clone(),
            query,
        });
    });
    rsx! {
        syntaxis_module_files::FilesView {
            workspace: active.current(),
            query,
            on_navigate,
        }
    }
}
