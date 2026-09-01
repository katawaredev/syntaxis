mod ai;
mod git;

use std::{
    collections::{BTreeSet, HashSet},
    fmt,
    rc::Rc,
};

use self::{
    ai::GuestAi,
    git::{GuestGit, HISTORY_PATH as GUEST_HISTORY_PATH},
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use dioxus::html::{ScrollBehavior, geometry::PixelsVector2D};
use dioxus::prelude::*;
use dioxus_code_editor::{CODE_EDITOR_CSS, CodeEditor, EditorEdit};
use serde::{Deserialize, Serialize};
use syntaxis_editor::{
    EditorBuffer, EditorConfig, ExplorerNode, ExplorerTree, ExternalChange, language_slug_for_path,
};
use syntaxis_terminal_browser::{
    WorkspaceChange, WorkspaceChangeKind, cancel as cancel_browser_command,
    execute as execute_browser_command, wait_for_bridge,
};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, ControlSize, DialogActions, DialogForm, Field, FileIcon, Icon,
    IconButton, Modal, PanelHeader, PanelTab, PanelTabIndicator, PanelTabList, PanelTabWidth,
    StatusBadge, TextInput, Tone,
};
use syntaxis_workspace::{
    BULKY_GENERATED_DIRECTORY_NAMES, EntryKind, ErrorCode, FileEntry, RelativePath,
    WorkspaceAvailability, WorkspaceError, WorkspaceFiles, WorkspaceIcon, WorkspaceIconSymbol,
    WorkspaceId, WorkspaceProfile, WorkspaceRecord, WorkspaceSection,
    is_bulky_generated_directory_name,
};
use syntaxis_workspace_browser::{
    BrowserSearchHit, OpfsWorkspaceFiles, SavedDirectory, local_directory_picker_supported,
    restore_local_directory, search as search_workspace, select_local_directory,
    set_private_workspace,
};

const GUEST_CSS: Asset = asset!("/assets/guest.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const GEIST_FONT: Asset = asset!("/assets/geist-latin-wght-normal.woff2");
const FAVICON: Asset = asset!("/assets/favicon.ico");
const FAVICON_SVG: Asset = asset!("/assets/favicon.svg");
const FAVICON_96: Asset = asset!("/assets/favicon-96x96.png");
const APPLE_TOUCH_ICON: Asset = asset!("/assets/apple-touch-icon.png");
const SITE_MANIFEST: Asset = asset!("/assets/site.webmanifest");
#[used]
static WEB_APP_ICON_192: Asset = asset!(
    "/assets/web-app-manifest-192x192.png",
    AssetOptions::builder().with_hash_suffix(false)
);
#[used]
static WEB_APP_ICON_512: Asset = asset!(
    "/assets/web-app-manifest-512x512.png",
    AssetOptions::builder().with_hash_suffix(false)
);
const GUEST_ARCHIVE_SCRIPT: Asset = asset!("/assets/guest-archive.bundle.js");
const GUEST_TERMINAL_SCRIPT: Asset = asset!("/assets/guest-terminal.bundle.js");
const MAX_TEXT_BYTES: u64 = 4 * 1024 * 1024;
const UNSAVED_NAVIGATION_MESSAGE: &str =
    "Save all modified files before leaving or changing workspace storage.";
const MAX_ARCHIVE_FILES: usize = 10_000;
const MAX_ARCHIVE_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ARCHIVE_WORKSPACE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct GuestFilesQuery {
    path: Option<String>,
}

impl From<&str> for GuestFilesQuery {
    fn from(query: &str) -> Self {
        Self {
            path: url::form_urlencoded::parse(query.as_bytes()).find_map(|(key, value)| {
                (key == "path")
                    .then(|| value.into_owned())
                    .filter(|value| !value.trim().is_empty())
            }),
        }
    }
}

impl fmt::Display for GuestFilesQuery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        if let Some(path) = self.path.as_deref() {
            serializer.append_pair("path", path);
        }
        formatter.write_str(&serializer.finish())
    }
}

#[derive(Clone, Debug, Routable, PartialEq)]
#[rustfmt::skip]
enum GuestRoute {
    #[route("/")]
    Home {},
    #[route("/workspaces/:slug/files?:..query")]
    Files { slug: String, query: GuestFilesQuery },
    #[route("/workspaces/:slug/terminal?:..query")]
    Terminal { slug: String, query: GuestFilesQuery },
    #[route("/workspaces/:slug/preview")]
    Preview { slug: String },
    #[route("/workspaces/:slug/git")]
    Git { slug: String },
    #[route("/workspaces/:slug/ai")]
    Ai { slug: String },
}

#[component]
pub fn App() -> Element {
    let geist_font_face = format!(
        "@font-face {{ font-family: 'Geist Variable'; src: url('{GEIST_FONT}') format('woff2'); font-style: normal; font-weight: 100 900; font-display: swap; }}",
    );
    rsx! {
        document::Link { rel: "icon", r#type: "image/svg+xml", href: FAVICON_SVG }
        document::Link {
            rel: "icon",
            r#type: "image/png",
            sizes: "96x96",
            href: FAVICON_96,
        }
        document::Link { rel: "shortcut icon", href: FAVICON }
        document::Link {
            rel: "apple-touch-icon",
            sizes: "180x180",
            href: APPLE_TOUCH_ICON,
        }
        document::Link { rel: "manifest", href: SITE_MANIFEST }
        document::Meta { name: "theme-color", content: "#1f2021" }
        document::Link {
            rel: "preload",
            href: GEIST_FONT,
            r#as: "font",
            r#type: "font/woff2",
            crossorigin: "anonymous",
        }
        document::Style { {geist_font_face} }
        document::Stylesheet { href: TAILWIND_CSS }
        document::Stylesheet { href: GUEST_CSS }
        document::Stylesheet { href: CODE_EDITOR_CSS }
        Router::<GuestRoute> {}
    }
}

#[component]
fn Home() -> Element {
    let mut notice = use_signal(|| None::<Notice>);
    let folder_picker_supported = use_hook(local_directory_picker_supported);
    let navigator = use_navigator();
    let open_browser_workspace: EventHandler<MouseEvent> = {
        let navigator = navigator.clone();
        EventHandler::new(move |_: MouseEvent| {
            set_private_workspace();
            navigator.push(GuestRoute::Files {
                slug: "browser".to_owned(),
                query: GuestFilesQuery::default(),
            });
        })
    };
    let open_local_folder: EventHandler<MouseEvent> = EventHandler::new(move |_: MouseEvent| {
        if !folder_picker_supported {
            notice.set(Some(Notice::error(
                "This browser does not support opening a local folder.",
            )));
            return;
        }
        let navigator = navigator.clone();
        spawn(async move {
            match select_local_directory().await {
                Ok(selected) => {
                    if let Some(warning) = selected.persistence_warning {
                        notice.set(Some(Notice::error(warning)));
                    }
                    navigator.push(GuestRoute::Files {
                        slug: slug_for_project(&selected.name),
                        query: GuestFilesQuery::default(),
                    });
                }
                Err(error) => notice.set(Some(Notice::error(error.message))),
            }
        });
    });

    rsx! {
        document::Title { "Home · Syntaxis" }
        main { class: "app-viewport relative w-full overflow-x-hidden overflow-y-auto overscroll-contain bg-background",
            section { class: "mx-auto flex min-h-full w-[calc(100%-2.5rem)] max-w-205 flex-col pt-[max(9vh,env(safe-area-inset-top))] pb-[max(1.5rem,env(safe-area-inset-bottom))] max-md:w-[calc(100%-1.5rem)] max-md:max-w-155 max-md:pt-[max(2.125rem,env(safe-area-inset-top))]",
                header { class: "mb-9.5 flex items-start justify-between gap-4 max-md:mb-6.5",
                    div { class: "min-w-0",
                        p { class: "text-[10px] font-bold tracking-[0.14em] text-primary max-[420px]:hidden",
                            "BROWSER WORKSPACES"
                        }
                        h1 { class: "mt-1 text-4xl font-semibold tracking-tight text-foreground max-md:text-3xl max-[420px]:mt-0 max-[420px]:text-2xl",
                            "Welcome back!"
                        }
                        p { class: "mt-1 text-[15px] text-muted-foreground max-[420px]:text-[13px]",
                            "Pick up where you left off or open another project."
                        }
                    }
                    div { class: "flex items-center gap-1",
                        IconButton {
                            label: "Notifications",
                            icon: AppIcon::Bell,
                            size: ControlSize::Small,
                            onclick: move |_| notice.set(Some(Notice::success("Browser workspace notifications."))),
                        }
                        IconButton {
                            label: "Sign out",
                            icon: AppIcon::Logout,
                            size: ControlSize::Small,
                            onclick: move |_| {
                                notice.set(Some(Notice::error("The browser workspace has no account session.")))
                            },
                        }
                    }
                }
                div { class: "mb-10.5 grid grid-cols-2 gap-3 max-md:mb-8 max-md:grid-cols-1",
                    GuestSourceAction {
                        icon: AppIcon::Folder,
                        title: "Open folder".to_owned(),
                        description: if folder_picker_supported { "Use a folder from this device".to_owned() } else { "Unavailable here; use private browser storage".to_owned() },
                        disabled: !folder_picker_supported,
                        onclick: open_local_folder,
                    }
                    GuestSourceAction {
                        icon: AppIcon::FolderPlus,
                        title: "Empty Project".to_owned(),
                        description: "Start a pure browser project".to_owned(),
                        disabled: false,
                        onclick: open_browser_workspace,
                    }
                }
                section { "aria-labelledby": "recent-title",
                    div { class: "mb-3 flex items-center justify-between gap-3",
                        h2 {
                            class: "text-[17px] font-semibold text-muted-foreground",
                            id: "recent-title",
                            "Recent projects"
                        }
                        IconButton {
                            label: "Recent project actions",
                            icon: AppIcon::MoreVertical,
                            size: ControlSize::Small,
                            onclick: move |_| {
                                notice
                                    .set(
                                        Some(
                                            Notice::success(
                                                "Browser projects are stored locally in this browser.",
                                            ),
                                        ),
                                    )
                            },
                        }
                    }
                    button {
                        class: "flex w-full min-h-22 items-center gap-3 rounded-xl border border-border bg-card px-4 py-3 text-left shadow-sm transition-colors hover:border-primary/60 hover:bg-accent/80 max-md:min-h-16 max-md:py-2.5",
                        r#type: "button",
                        onclick: open_browser_workspace,
                        GuestProjectIcon { name: "Browser workspace".to_owned() }
                        span { class: "min-w-0 flex-1",
                            strong { class: "block truncate text-foreground", "Browser workspace" }
                            small { class: "text-muted-foreground", "Private storage on this device" }
                        }
                        span { class: "text-muted-foreground", "Open →" }
                    }
                }
                footer { class: "mt-auto pt-10 text-center text-[11px] text-muted-foreground",
                    "Browser storage"
                }
            }
        }
        if let Some(message) = notice() {
            div {
                class: if message.error { "guest-notice guest-notice-error" } else { "guest-notice" },
                role: "status",
                span { "{message.message}" }
                button {
                    aria_label: "Dismiss notification",
                    onclick: move |_| notice.set(None),
                    "×"
                }
            }
        }
    }
}

#[component]
fn Files(slug: String, query: GuestFilesQuery) -> Element {
    rsx! {
        GuestWorkspace {
            slug,
            initial_view: GuestView::Editor,
            initial_path: query.path,
        }
    }
}

#[component]
fn Terminal(slug: String, query: GuestFilesQuery) -> Element {
    rsx! {
        GuestWorkspace {
            slug,
            initial_view: GuestView::Terminal,
            initial_path: query.path,
        }
    }
}

#[component]
fn Preview(slug: String) -> Element {
    rsx! {
        GuestWorkspace { slug, initial_view: GuestView::Preview, initial_path: None }
    }
}

#[component]
fn Git(slug: String) -> Element {
    rsx! {
        GuestWorkspace { slug, initial_view: GuestView::Git, initial_path: None }
    }
}

#[component]
fn Ai(slug: String) -> Element {
    rsx! {
        GuestWorkspace { slug, initial_view: GuestView::Ai, initial_path: None }
    }
}

#[component]
fn GuestSourceAction(
    icon: AppIcon,
    title: String,
    description: String,
    disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            class: "grid min-w-0 grid-cols-[auto_minmax(0,1fr)] items-center gap-3 overflow-hidden rounded-xl border border-border bg-card p-4 text-left shadow-sm transition-colors hover:border-primary/60 hover:bg-accent/80 max-[420px]:p-3.5",
            disabled,
            aria_label: title.clone(),
            onclick: move |event| onclick.call(event),
            span { class: "grid size-9 place-items-center rounded-lg bg-primary/10 text-primary",
                Icon { icon, size: 22 }
            }
            span { class: "min-w-0",
                strong { class: "mb-1 block text-foreground", {title.clone()} }
                small { class: "block leading-snug text-muted-foreground", {description} }
            }
        }
    }
}

#[component]
fn GuestWorkspace(slug: String, initial_view: GuestView, initial_path: Option<String>) -> Element {
    let workspace = guest_workspace(slug);
    let files = OpfsWorkspaceFiles;
    let requested_path = initial_path
        .and_then(|path| RelativePath::try_from(path).ok())
        .filter(|path| !path.is_root());
    let mut current_directory = use_signal(RelativePath::root);
    let mut explorer_search_open = use_signal(|| false);
    let mut explorer_tree = use_signal(ExplorerTree::default);
    let mut selected_tree_entry = use_signal(|| None::<FileEntry>);
    let mut show_generated = use_signal(|| false);
    let mut mobile_explorer_open = use_signal(|| false);
    let mut workspace_menu_open = use_signal(|| false);
    let mut new_entry_open = use_signal(|| false);
    let mut revision = use_signal(|| 0_u64);
    let mut buffer = use_signal(|| None::<EditorBuffer>);
    let mut tab_buffers = use_signal(Vec::<EditorBuffer>::new);
    let mut open_tabs = use_signal(Vec::<String>::new);
    let mut binary_preview = use_signal(|| None::<BinaryPreviewState>);
    let mut new_file_name = use_signal(String::new);
    let mut new_entry_kind = use_signal(|| EntryKind::File);
    let mut busy = use_signal(|| false);
    let mut notice = use_signal(|| None::<Notice>);
    let mut storage_location = use_signal(|| StorageLocation::Private);
    let mut saved_directory_name = use_signal(|| None::<String>);
    let mut startup_complete = use_signal(|| false);
    let mut pending_delete = use_signal(|| None::<RelativePath>);
    let mut search_query = use_signal(String::new);
    let mut file_operation = use_signal(|| None::<FileOperation>);
    let mut operation_destination = use_signal(String::new);
    let mut initial_location_loaded = use_signal(|| false);
    let mut active_view = use_signal(move || initial_view);
    let preview_source = use_signal(|| None::<String>);
    let preview_loading = use_signal(|| false);
    let folder_picker_supported = use_hook(local_directory_picker_supported);
    let navigator = use_navigator();
    let route_slug = workspace.slug.clone();
    let initial_route_path = requested_path.as_ref().map(|path| path.as_str().to_owned());
    let route_navigator = navigator.clone();
    use_effect(use_reactive(
        (
            &current_directory,
            &active_view,
            &buffer,
            &binary_preview,
            &initial_location_loaded,
        ),
        move |(path, view, buffer, binary_preview, location_loaded)| {
            if view() == GuestView::Editor {
                let file_path = if !location_loaded() {
                    initial_route_path.clone()
                } else {
                    buffer().as_ref().map(|open| open.path.clone()).or_else(|| {
                        binary_preview()
                            .as_ref()
                            .map(|preview| preview.path.clone())
                    })
                };
                route_navigator.replace(GuestRoute::Files {
                    slug: route_slug.clone(),
                    query: GuestFilesQuery {
                        path: file_path
                            .or_else(|| (!path().is_root()).then(|| path().as_str().to_owned())),
                    },
                });
            }
        },
    ));

    let restore_saved_directory = workspace.slug != "browser";
    let _restore_directory = use_resource(move || async move {
        if folder_picker_supported && restore_saved_directory {
            match restore_local_directory(false).await {
                Ok(SavedDirectory::Active(name)) => {
                    storage_location.set(StorageLocation::Local(name));
                }
                Ok(SavedDirectory::NeedsPermission(name)) => {
                    saved_directory_name.set(Some(name));
                }
                Ok(SavedDirectory::Missing) => {}
                Err(error) => notice.set(Some(Notice::error(error.message))),
            }
        }
        startup_complete.set(true);
    });

    let directory_workspace = workspace.clone();
    let entries: Resource<Result<(RelativePath, Vec<FileEntry>), WorkspaceError>> =
        use_resource(move || {
            let path = current_directory();
            let _revision = revision();
            let ready = startup_complete();
            let workspace = directory_workspace.clone();
            async move {
                let mut entries = if ready {
                    files.list(&workspace, &path).await?
                } else {
                    Vec::new()
                };
                entries.retain(|entry| entry.path.as_str() != GUEST_HISTORY_PATH);
                Ok((path, entries))
            }
        });
    use_effect(move || {
        let Some(Ok((path, items))) = entries() else {
            return;
        };
        explorer_tree
            .write()
            .replace_directory(path.as_str(), items);
    });
    let search_workspace_record = workspace.clone();
    let search_results: Resource<Result<Vec<BrowserSearchHit>, String>> = use_resource(move || {
        let query = search_query();
        let ready = startup_complete();
        let _revision = revision();
        let include_generated = show_generated();
        let workspace = search_workspace_record.clone();
        async move {
            if !ready || query.trim().is_empty() {
                Ok(Vec::new())
            } else {
                let mut hits = search_workspace(&files, &workspace, &query)
                    .await
                    .map_err(|error| error.message)?;
                hits.retain(|hit| {
                    hit.entry.path.as_str() != GUEST_HISTORY_PATH
                        && (include_generated
                            || !path_contains_bulky_generated_directory(hit.entry.path.as_str()))
                });
                Ok(hits)
            }
        }
    });

    let initial_file_workspace = workspace.clone();
    let initial_file_path = requested_path.clone();
    use_effect(move || {
        if !startup_complete() || initial_location_loaded() {
            return;
        }
        let Some(path) = initial_file_path.clone() else {
            initial_location_loaded.set(true);
            return;
        };
        active_view.set(GuestView::Editor);
        busy.set(true);
        notice.set(None);
        let workspace = initial_file_workspace.clone();
        spawn(async move {
            populate_tree_to_file(&files, &workspace, &path, explorer_tree).await;
            match files.read_text(&workspace, &path, MAX_TEXT_BYTES).await {
                Ok(text) => {
                    remember_tab(open_tabs, path.as_str());
                    buffer.set(Some(EditorBuffer::open(
                        path.as_str(),
                        text.content,
                        text.version,
                        EditorConfig::default(),
                    )));
                    initial_location_loaded.set(true);
                }
                Err(text_error) => {
                    match files.read_binary(&workspace, &path, MAX_BINARY_BYTES).await {
                        Ok(file) => {
                            binary_preview.set(Some(BinaryPreviewState {
                                path: path.as_str().to_owned(),
                                size: file.content.len(),
                                data_url: image_mime(path.as_str()).map(|mime| {
                                    format!("data:{mime};base64:{}", BASE64.encode(file.content))
                                }),
                                hex: String::new(),
                            }));
                            initial_location_loaded.set(true);
                        }
                        Err(_) => {
                            notice.set(Some(Notice::error(text_error.message)));
                            initial_location_loaded.set(true);
                        }
                    }
                }
            }
            busy.set(false);
        });
    });

    let active_buffer = buffer();
    let active_binary = binary_preview();
    let active_path = active_buffer
        .as_ref()
        .map(|open| open.path.clone())
        .or_else(|| active_binary.as_ref().map(|preview| preview.path.clone()));
    let active_name = active_path
        .as_deref()
        .and_then(|path| path.rsplit('/').next())
        .unwrap_or("No file selected");
    let preview_path = active_path
        .clone()
        .unwrap_or_else(|| "index.html".to_owned());
    let dirty = active_buffer.as_ref().is_some_and(EditorBuffer::is_dirty);
    let any_dirty = dirty || tab_buffers.read().iter().any(EditorBuffer::is_dirty);
    let has_html_preview = active_buffer
        .as_ref()
        .is_some_and(|open| is_html_path(&open.path));
    let generated_paths = BULKY_GENERATED_DIRECTORY_NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<BTreeSet<_>>();
    let explorer_nodes =
        explorer_tree
            .read()
            .flattened("", None, &generated_paths, show_generated());
    let section_title = match active_view() {
        GuestView::Editor => "Files",
        GuestView::Terminal => "Terminal",
        GuestView::Git => "Source control",
        GuestView::Preview => "Preview",
        GuestView::Ai => "AI",
    };
    let page_title = format!("{} · {section_title}", workspace.name);
    let save_workspace = workspace.clone();
    rsx! {
        document::Title { "{page_title}" }
        document::Script { src: GUEST_ARCHIVE_SCRIPT }
        document::Script { src: GUEST_TERMINAL_SCRIPT }

        main { class: "app-viewport flex w-full flex-col overflow-hidden",
            header { class: "flex h-[calc(2.875rem+env(safe-area-inset-top))] min-h-[calc(2.875rem+env(safe-area-inset-top))] items-center gap-2 border-b border-border bg-background px-[max(0.625rem,env(safe-area-inset-left))] pt-[env(safe-area-inset-top)] max-md:h-[calc(3rem+env(safe-area-inset-top))] max-md:min-h-[calc(3rem+env(safe-area-inset-top))]",
                button {
                    class: "inline-flex size-8.5 items-center justify-center rounded-lg text-muted-foreground hover:bg-accent hover:text-foreground",
                    r#type: "button",
                    title: "Back to browser workspaces",
                    aria_label: "Back to browser workspaces",
                    onclick: {
                        let navigator = navigator.clone();
                        move |_| {
                            if any_dirty {
                                notice.set(Some(Notice::error(UNSAVED_NAVIGATION_MESSAGE)));
                            } else {
                                navigator.push(GuestRoute::Home {});
                            }
                        }
                    },
                    "←"
                }
                if active_view() == GuestView::Editor {
                    button {
                        class: "hidden size-8.5 items-center justify-center rounded-lg text-muted-foreground hover:bg-accent hover:text-foreground max-md:inline-flex",
                        r#type: "button",
                        title: "Open file explorer",
                        aria_label: "Open file explorer",
                        onclick: move |_| mobile_explorer_open.set(true),
                        Icon { icon: AppIcon::Folder, size: 17 }
                    }
                }
                GuestProjectIcon { name: workspace.name.clone() }
                div { class: "flex min-w-0 items-center gap-2",
                    strong { class: "truncate text-[13px]", {workspace.name.clone()} }
                    StatusBadge {
                        label: storage_location().badge_label(),
                        tone: Tone::Neutral,
                    }
                }
                div { class: "ml-auto flex items-center gap-1 pr-2 text-[11px] text-muted-foreground",
                    div {
                        class: "grid size-8 place-items-center rounded-lg",
                        title: "Browser-only workspace",
                        span {
                            class: "size-2 rounded-full bg-success shadow-[0_0_0.5rem_color-mix(in_oklch,var(--success),transparent_20%)]",
                            aria_hidden: "true",
                        }
                    }
                    IconButton {
                        label: "Notifications",
                        icon: AppIcon::Bell,
                        size: ControlSize::Small,
                        onclick: move |_| notice.set(Some(Notice::success("Browser workspace notifications."))),
                    }
                    IconButton {
                        label: "Sign out",
                        icon: AppIcon::Logout,
                        size: ControlSize::Small,
                        onclick: move |_| {
                            notice.set(Some(Notice::error("The browser workspace has no account session.")))
                        },
                    }
                }
            }

            div { class: "min-h-0 flex-1 overflow-hidden",
                section { class: if active_view() == GuestView::Editor { "grid size-full min-h-0 min-w-0 grid-cols-[248px_minmax(0,1fr)] overflow-hidden max-md:block" } else { "flex size-full min-h-0 min-w-0 flex-col overflow-hidden bg-background" },
                    if active_view() == GuestView::Editor {
                        aside { class: if mobile_explorer_open() { "min-h-0 min-w-0 border-r border-border bg-sidebar max-md:fixed max-md:inset-x-0 max-md:top-[calc(3rem+env(safe-area-inset-top))] max-md:bottom-[calc(3.875rem+env(safe-area-inset-bottom))] max-md:z-30" } else { "min-h-0 min-w-0 border-r border-border bg-sidebar max-md:hidden" },
                            div { class: "grid h-12 min-h-12 grid-cols-2 items-center gap-1 border-b border-border p-1.25",
                                button {
                                    class: if !explorer_search_open() { "guest-explorer-tab guest-explorer-tab-active" } else { "guest-explorer-tab" },
                                    onclick: move |_| {
                                        explorer_search_open.set(false);
                                        search_query.set(String::new());
                                    },
                                    "Files"
                                }
                                button {
                                    class: if explorer_search_open() { "guest-explorer-tab guest-explorer-tab-active" } else { "guest-explorer-tab" },
                                    onclick: move |_| explorer_search_open.set(true),
                                    "Search"
                                }
                            }
                            if !explorer_search_open() {
                                div { class: "explorer-toolbar relative flex h-10.5 min-h-10.5 items-center gap-1 border-b border-border px-1.25",
                                    IconButton {
                                        label: "New file",
                                        icon: AppIcon::FilePlus,
                                        size: ControlSize::Small,
                                        disabled: busy(),
                                        onclick: move |_| {
                                            new_entry_kind.set(EntryKind::File);
                                            new_entry_open.set(true);
                                        },
                                    }
                                    IconButton {
                                        label: "New folder",
                                        icon: AppIcon::FolderPlus,
                                        size: ControlSize::Small,
                                        disabled: busy(),
                                        onclick: move |_| {
                                            new_entry_kind.set(EntryKind::Directory);
                                            new_entry_open.set(true);
                                        },
                                    }
                                    label {
                                        class: if busy() { "touch-target inline-flex size-7.25 min-w-7.25 cursor-not-allowed items-center justify-center rounded-md bg-transparent text-muted-foreground opacity-50" } else { "touch-target inline-flex size-7.25 min-w-7.25 cursor-pointer items-center justify-center rounded-md bg-transparent text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" },
                                        aria_label: "Upload files",
                                        title: "Upload files",
                                        input {
                                            class: "guest-upload-input",
                                            r#type: "file",
                                            multiple: true,
                                            disabled: busy(),
                                            onchange: {
                                                let workspace = workspace.clone();
                                                move |event: FormEvent| {
                                                    let selected = event.files();
                                                    let workspace = workspace.clone();
                                                    spawn(
                                                        upload_files(
                                                            selected,
                                                            workspace,
                                                            current_directory(),
                                                            files,
                                                            busy,
                                                            revision,
                                                            notice,
                                                        ),
                                                    );
                                                }
                                            },
                                        }
                                        Icon { icon: AppIcon::Upload, size: 14 }
                                    }
                                    span { class: "flex-1" }
                                    button {
                                        class: "hidden size-7.25 items-center justify-center rounded-md text-muted-foreground hover:bg-accent max-md:inline-flex",
                                        r#type: "button",
                                        title: "Close file explorer",
                                        aria_label: "Close file explorer",
                                        onclick: move |_| mobile_explorer_open.set(false),
                                        "×"
                                    }
                                    IconButton {
                                        label: "Refresh files",
                                        icon: AppIcon::Refresh,
                                        size: ControlSize::Small,
                                        disabled: busy(),
                                        onclick: move |_| {
                                            explorer_tree.set(ExplorerTree::default());
                                            current_directory.set(RelativePath::root());
                                            revision += 1;
                                        },
                                    }
                                    IconButton {
                                        label: "Workspace actions",
                                        icon: AppIcon::Menu,
                                        size: ControlSize::Small,
                                        pressed: workspace_menu_open(),
                                        onclick: move |_| workspace_menu_open.toggle(),
                                    }
                                    if workspace_menu_open() {
                                        div { class: "guest-workspace-menu absolute top-10 right-1 z-10 w-56 rounded-md border border-border bg-popover p-1.5 text-xs text-foreground shadow-xl",
                                            div { class: "border-b border-border px-2 py-1.5 text-[10px] text-muted-foreground",
                                                "Workspace actions · {storage_location().badge_label()}"
                                                div { class: "truncate",
                                                    "{display_path(&current_directory())}"
                                                }
                                            }
                                            if let Some(selected) = selected_tree_entry() {
                                                button {
                                                    r#type: "button",
                                                    disabled: busy(),
                                                    onclick: {
                                                        let selected = selected.clone();
                                                        move |_| {
                                                            operation_destination
                                                                .set(selected.path.as_str().to_owned());
                                                            file_operation
                                                                .set(
                                                                    Some(FileOperation {
                                                                        source: selected.path.clone(),
                                                                        kind: FileOperationKind::Move,
                                                                    }),
                                                                );
                                                            workspace_menu_open.set(false);
                                                        }
                                                    },
                                                    Icon {
                                                        icon: AppIcon::FileMove,
                                                        size: 14,
                                                    }
                                                    span { "Rename or move selected" }
                                                }
                                                button {
                                                    r#type: "button",
                                                    disabled: busy(),
                                                    onclick: {
                                                        let selected = selected.clone();
                                                        move |_| {
                                                            operation_destination
                                                                .set(duplicate_path(&selected.path));
                                                            file_operation
                                                                .set(
                                                                    Some(FileOperation {
                                                                        source: selected.path.clone(),
                                                                        kind: FileOperationKind::Duplicate,
                                                                    }),
                                                                );
                                                            workspace_menu_open.set(false);
                                                        }
                                                    },
                                                    Icon {
                                                        icon: AppIcon::Copy,
                                                        size: 14,
                                                    }
                                                    span { "Duplicate selected" }
                                                }
                                                button {
                                                    r#type: "button",
                                                    class: "!text-destructive",
                                                    disabled: busy(),
                                                    onclick: {
                                                        let selected = selected.clone();
                                                        move |_| {
                                                            pending_delete.set(Some(selected.path.clone()));
                                                            workspace_menu_open.set(false);
                                                        }
                                                    },
                                                    Icon {
                                                        icon: AppIcon::Delete,
                                                        size: 14,
                                                    }
                                                    span { "Delete selected" }
                                                }
                                                hr {}
                                            }
                                            IconButton {
                                                label: if show_generated() { "Hide generated folders" } else { "Show generated folders" },
                                                icon: AppIcon::Eye,
                                                size: ControlSize::Small,
                                                pressed: show_generated(),
                                                onclick: move |_| show_generated.toggle(),
                                            }
                                            IconButton {
                                                label: "Open local folder",
                                                icon: AppIcon::Folder,
                                                size: ControlSize::Small,
                                                disabled: busy() || any_dirty || !folder_picker_supported,
                                                onclick: move |_| {
                                                    spawn(async move {
                                                        match select_local_directory().await {
                                                            Ok(selected) => {
                                                                storage_location.set(StorageLocation::Local(selected.name));
                                                                saved_directory_name.set(None);
                                                                explorer_tree.set(ExplorerTree::default());
                                                                current_directory.set(RelativePath::root());
                                                                revision += 1;
                                                                if let Some(warning) = selected.persistence_warning {
                                                                    notice.set(Some(Notice::error(warning)));
                                                                } else {
                                                                    notice
                                                                        .set(
                                                                            Some(Notice::success("Using the selected local folder.")),
                                                                        );
                                                                }
                                                            }
                                                            Err(error) => notice.set(Some(Notice::error(error.message))),
                                                        }
                                                    });
                                                },
                                            }
                                            if saved_directory_name().is_some() {
                                                IconButton {
                                                    label: "Reconnect local folder",
                                                    icon: AppIcon::Refresh,
                                                    size: ControlSize::Small,
                                                    disabled: busy() || any_dirty,
                                                    onclick: move |_| {
                                                        spawn(async move {
                                                            match restore_local_directory(true).await {
                                                                Ok(SavedDirectory::Active(name)) => {
                                                                    saved_directory_name.set(None);
                                                                    storage_location.set(StorageLocation::Local(name));
                                                                    explorer_tree.set(ExplorerTree::default());
                                                                    current_directory.set(RelativePath::root());
                                                                    revision += 1;
                                                                    notice.set(Some(Notice::success("Local folder reconnected.")));
                                                                }
                                                                Ok(SavedDirectory::NeedsPermission(name)) => {
                                                                    saved_directory_name.set(Some(name));
                                                                    notice
                                                                        .set(
                                                                            Some(
                                                                                Notice::error("The browser did not grant folder access."),
                                                                            ),
                                                                        );
                                                                }
                                                                Ok(SavedDirectory::Missing) => {
                                                                    notice.set(Some(Notice::error("No saved local folder was found.")));
                                                                }
                                                                Err(error) => notice.set(Some(Notice::error(error.message))),
                                                            }
                                                        });
                                                    },
                                                }
                                            }
                                            if storage_location() != StorageLocation::Private {
                                                IconButton {
                                                    label: "Use private workspace",
                                                    icon: AppIcon::ShieldAlert,
                                                    size: ControlSize::Small,
                                                    disabled: busy() || any_dirty,
                                                    onclick: move |_| {
                                                        set_private_workspace();
                                                        storage_location.set(StorageLocation::Private);
                                                        explorer_tree.set(ExplorerTree::default());
                                                        current_directory.set(RelativePath::root());
                                                        revision += 1;
                                                        notice.set(Some(Notice::success("Using private browser storage.")));
                                                    },
                                                }
                                            }
                                            IconButton {
                                                label: "Export workspace ZIP",
                                                icon: AppIcon::Share,
                                                size: ControlSize::Small,
                                                disabled: busy(),
                                                onclick: {
                                                    let workspace = workspace.clone();
                                                    move |_| {
                                                        spawn(export_workspace(files, workspace.clone(), busy, notice));
                                                    }
                                                },
                                            }
                                            label {
                                                class: if busy() { "touch-target inline-flex size-7.25 min-w-7.25 cursor-not-allowed items-center justify-center rounded-md bg-transparent text-muted-foreground opacity-50" } else { "touch-target inline-flex size-7.25 min-w-7.25 cursor-pointer items-center justify-center rounded-md bg-transparent text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" },
                                                aria_label: "Import workspace ZIP",
                                                title: "Import workspace ZIP",
                                                input {
                                                    class: "guest-upload-input",
                                                    r#type: "file",
                                                    accept: ".zip,application/zip",
                                                    disabled: busy(),
                                                    onchange: {
                                                        let workspace = workspace.clone();
                                                        move |event: FormEvent| {
                                                            let Some(selected) = event.files().into_iter().next() else {
                                                                return;
                                                            };
                                                            spawn(
                                                                import_workspace(
                                                                    selected,
                                                                    workspace.clone(),
                                                                    files,
                                                                    busy,
                                                                    revision,
                                                                    notice,
                                                                ),
                                                            );
                                                        }
                                                    },
                                                }
                                                Icon {
                                                    icon: AppIcon::FilePlus,
                                                    size: 14,
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                div { class: "flex items-center gap-1 border-b border-border p-1.75",
                                    form {
                                        class: "relative min-w-0 flex-1",
                                        onsubmit: move |event: FormEvent| event.prevent_default(),
                                        input {
                                            class: "h-7.25 w-full rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none placeholder:text-muted-foreground focus:border-ring",
                                            value: search_query,
                                            r#type: "search",
                                            placeholder: "Search workspace…",
                                            aria_label: "Search workspace",
                                            oninput: move |event| search_query.set(event.value()),
                                        }
                                    }
                                    if !search_query().is_empty() {
                                        IconButton {
                                            label: "Clear workspace search",
                                            icon: AppIcon::Close,
                                            size: ControlSize::Small,
                                            onclick: move |_| search_query.set(String::new()),
                                        }
                                    }
                                }
                            }
                            if new_entry_open() {
                                Modal {
                                    title: if new_entry_kind() == EntryKind::Directory { "New folder" } else { "New file" },
                                    description: format!("Create inside {}.", display_path(&current_directory())),
                                    on_close: move |()| {
                                        new_entry_open.set(false);
                                        new_file_name.set(String::new());
                                    },
                                    form {
                                        onsubmit: {
                                            let workspace = workspace.clone();
                                            move |event| {
                                                event.prevent_default();
                                                let name = new_file_name().trim().to_owned();
                                                let Ok(path) = child_path(&current_directory(), &name) else {
                                                    notice.set(Some(Notice::error("Enter a valid file name.")));
                                                    return;
                                                };
                                                let workspace = workspace.clone();
                                                let kind = new_entry_kind();
                                                busy.set(true);
                                                notice.set(None);
                                                spawn(async move {
                                                    let result = if kind == EntryKind::Directory {
                                                        files.create_directory(&workspace, &path).await
                                                    } else {
                                                        files.create_file(&workspace, &path).await
                                                    };
                                                    match result {
                                                        Ok(_) => {
                                                            if kind == EntryKind::File {
                                                                match files
                                                                    .read_text(&workspace, &path, MAX_TEXT_BYTES)
                                                                    .await
                                                                {
                                                                    Ok(text) => {
                                                                        remember_tab(open_tabs, path.as_str());
                                                                        buffer
                                                                            .set(
                                                                                Some(
                                                                                    EditorBuffer::open(
                                                                                        path.as_str(),
                                                                                        text.content,
                                                                                        text.version,
                                                                                        EditorConfig::default(),
                                                                                    ),
                                                                                ),
                                                                            );
                                                                    }
                                                                    Err(error) => notice.set(Some(Notice::error(error.message))),
                                                                }
                                                            }
                                                            new_file_name.set(String::new());
                                                            new_entry_open.set(false);
                                                            revision += 1;
                                                        }
                                                        Err(error) => notice.set(Some(Notice::error(error.message))),
                                                    }
                                                    busy.set(false);
                                                });
                                            }
                                        },
                                        DialogForm {
                                            Field {
                                                control_id: "guest-new-entry-name",
                                                label: "Name",
                                                TextInput {
                                                    value: new_file_name(),
                                                    autofocus: true,
                                                    placeholder: if new_entry_kind() == EntryKind::Directory { "new_folder" } else { "new_file.txt" },
                                                    oninput: move |event: FormEvent| new_file_name.set(event.value()),
                                                }
                                            }
                                            DialogActions {
                                                Button {
                                                    label: "Cancel",
                                                    kind: ButtonKind::Ghost,
                                                    onclick: move |event: MouseEvent| {
                                                        event.prevent_default();
                                                        new_entry_open.set(false);
                                                        new_file_name.set(String::new());
                                                    },
                                                }
                                                button {
                                                    class: "inline-flex h-9 items-center justify-center rounded-lg bg-primary px-3.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50",
                                                    disabled: busy() || new_file_name().trim().is_empty(),
                                                    if new_entry_kind() == EntryKind::Directory {
                                                        "Create folder"
                                                    } else {
                                                        "Create file"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            div {
                                class: "touch-scroll-region min-h-0 flex-1 touch-pan-y overflow-y-auto overscroll-contain px-1.25 pt-1 guest-file-list",
                                role: if explorer_search_open() { "list" } else { "tree" },
                                "aria-label": if explorer_search_open() { "Workspace search results" } else { "Workspace files" },
                                if explorer_search_open() && search_query().trim().is_empty() {
                                    p { class: "guest-muted", "Search files by name or content." }
                                }
                                if explorer_search_open() && !search_query().trim().is_empty() {
                                    match search_results() {
                                        None => rsx! {
                                            p { class: "guest-muted", "Searching workspace…" }
                                        },
                                        Some(Err(error)) => rsx! {
                                            p { class: "guest-error", "{error}" }
                                        },
                                        Some(Ok(items)) if items.is_empty() => rsx! {
                                            p { class: "guest-muted", "No matching files." }
                                        },
                                        Some(Ok(items)) => rsx! {
                                            for hit in items {
                                                SearchRow {
                                                    key: "{hit.entry.path.as_str()}",
                                                    hit: hit.clone(),
                                                    active: active_path.as_deref() == Some(hit.entry.path.as_str()),
                                                    on_open: {
                                                        let workspace = workspace.clone();
                                                        move |entry: FileEntry| {
                                                            search_query.set(String::new());
                                                            if entry.kind == EntryKind::Directory {
                                                                active_view.set(GuestView::Editor);
                                                                expand_tree_path(explorer_tree, entry.path.as_str());
                                                                current_directory.set(entry.path);
                                                                explorer_search_open.set(false);
                                                                return;
                                                            }
                                                            expand_tree_path(explorer_tree, entry.path.as_str());
                                                            let entry_path = entry.path.clone();
                                                            active_view.set(GuestView::Editor);
                                                            buffer.set(None);
                                                            binary_preview.set(None);
                                                            busy.set(true);
                                                            notice.set(None);
                                                            let workspace = workspace.clone();
                                                            spawn(async move {
                                                                populate_tree_to_file(&files, &workspace, &entry_path, explorer_tree)
                                                                    .await;
                                                                match files.read_text(&workspace, &entry_path, MAX_TEXT_BYTES).await {
                                                                    Ok(text) => {
                                                                        remember_tab(open_tabs, entry_path.as_str());
                                                                        buffer
                                                                            .set(
                                                                                Some(
                                                                                    EditorBuffer::open(
                                                                                        entry_path.as_str(),
                                                                                        text.content,
                                                                                        text.version,
                                                                                        EditorConfig::default(),
                                                                                    ),
                                                                                ),
                                                                            );
                                                                    }
                                                                    Err(text_error) => {
                                                                        match files
                                                                            .read_binary(&workspace, &entry_path, MAX_BINARY_BYTES)
                                                                            .await
                                                                        {
                                                                            Ok(file) => {
                                                                                binary_preview
                                                                                    .set(
                                                                                        Some(BinaryPreviewState {
                                                                                            path: entry_path.as_str().to_owned(),
                                                                                            size: file.content.len(),
                                                                                            data_url: image_mime(entry_path.as_str())
                                                                                                .map(|mime| {
                                                                                                    format!(
                                                                                                        "data:{mime};base64:{}",
                                                                                                        BASE64.encode(file.content),
                                                                                                    )
                                                                                                }),
                                                                                            hex: String::new(),
                                                                                        }),
                                                                                    )
                                                                            }
                                                                            Err(_) => notice.set(Some(Notice::error(text_error.message))),
                                                                        }
                                                                    }
                                                                }
                                                                busy.set(false);
                                                            });
                                                        }
                                                    },
                                                }
                                            }
                                        },
                                    }
                                } else if !explorer_search_open() {
                                    match entries() {
                                        None => rsx! {
                                            p { class: "guest-muted", "Opening browser storage…" }
                                        },
                                        Some(Err(error)) => rsx! {
                                            p { class: "guest-error", "{error}" }
                                        },
                                        Some(Ok(_)) if explorer_nodes.is_empty() => rsx! {
                                            p { class: "guest-muted", "This workspace is empty." }
                                        },
                                        Some(Ok(_)) => rsx! {
                                            for node in explorer_nodes.clone() {
                                                FileRow {
                                                    key: "{node.entry.path.as_str()}",
                                                    active: active_path.as_deref() == Some(node.entry.path.as_str())
                                                        || selected_tree_entry()
                                                            .as_ref()
                                                            .is_some_and(|selected| selected.path == node.entry.path),
                                                    node: node.clone(),
                                                    on_open: {
                                                        let workspace = workspace.clone();
                                                        move |entry: FileEntry| {
                                                            selected_tree_entry.set(Some(entry.clone()));
                                                            if entry.kind == EntryKind::Directory {
                                                                let path = entry.path.clone();
                                                                let expanded = explorer_tree.write().toggle(path.as_str());
                                                                if expanded {
                                                                    current_directory.set(path);
                                                                }
                                                                return;
                                                            }
                                                            let entry_path = entry.path.clone();
                                                            active_view.set(GuestView::Editor);
                                                            buffer.set(None);
                                                            binary_preview.set(None);
                                                            let workspace = workspace.clone();
                                                            busy.set(true);
                                                            notice.set(None);
                                                            spawn(async move {
                                                                if let Some(mime) = image_mime(entry_path.as_str()) {
                                                                    match files
                                                                        .read_binary(&workspace, &entry_path, MAX_BINARY_BYTES)
                                                                        .await
                                                                    {
                                                                        Ok(file) => {
                                                                            binary_preview
                                                                                .set(
                                                                                    Some(BinaryPreviewState {
                                                                                        path: entry_path.as_str().to_owned(),
                                                                                        size: file.content.len(),
                                                                                        data_url: Some(
                                                                                            format!(
                                                                                                "data:{mime};base64:{}",
                                                                                                BASE64.encode(file.content),
                                                                                            ),
                                                                                        ),
                                                                                        hex: String::new(),
                                                                                    }),
                                                                                )
                                                                        }
                                                                        Err(error) => notice.set(Some(Notice::error(error.message))),
                                                                    }
                                                                } else {
                                                                    match files.read_text(&workspace, &entry_path, MAX_TEXT_BYTES).await
                                                                    {
                                                                        Ok(text) => {
                                                                            remember_tab(open_tabs, entry_path.as_str());
                                                                            buffer
                                                                                .set(
                                                                                    Some(
                                                                                        EditorBuffer::open(
                                                                                            entry_path.as_str(),
                                                                                            text.content,
                                                                                            text.version,
                                                                                            EditorConfig::default(),
                                                                                        ),
                                                                                    ),
                                                                                );
                                                                        }
                                                                        Err(text_error) => {
                                                                            match files
                                                                                .read_binary(&workspace, &entry_path, MAX_BINARY_BYTES)
                                                                                .await
                                                                            {
                                                                                Ok(file) => {
                                                                                    binary_preview
                                                                                        .set(
                                                                                            Some(BinaryPreviewState {
                                                                                                path: entry_path.as_str().to_owned(),
                                                                                                size: file.content.len(),
                                                                                                data_url: None,
                                                                                                hex: hex_preview(&file.content),
                                                                                            }),
                                                                                        )
                                                                                }
                                                                                Err(_) => {
                                                                                    notice.set(Some(Notice::error(text_error.message)))
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                                busy.set(false);
                                                            });
                                                        }
                                                    },
                                                    confirm_delete: pending_delete().as_ref() == Some(&node.entry.path),
                                                    on_delete: {
                                                        let workspace = workspace.clone();
                                                        let active_path = active_path.clone();
                                                        move |entry: FileEntry| {
                                                            if pending_delete().as_ref() != Some(&entry.path) {
                                                                pending_delete.set(Some(entry.path));
                                                                return;
                                                            }
                                                            if tab_buffers
                                                                .read()
                                                                .iter()
                                                                .any(|open| {
                                                                    open.is_dirty() && path_is_within(&open.path, entry.path.as_str())
                                                                })
                                                            {
                                                                notice.set(Some(Notice::error(UNSAVED_NAVIGATION_MESSAGE)));
                                                                return;
                                                            }
                                                            let workspace = workspace.clone();
                                                            let deleting_active = active_path.as_deref() == Some(entry.path.as_str());
                                                            let refresh_directory = parent_path(&entry.path);
                                                            busy.set(true);
                                                            notice.set(None);
                                                            spawn(async move {
                                                                match files.delete(&workspace, &entry.path).await {
                                                                    Ok(()) => {
                                                                        if deleting_active {
                                                                            buffer.set(None);
                                                                            binary_preview.set(None);
                                                                        }
                                                                        open_tabs
                                                                            .write()
                                                                            .retain(|path| {
                                                                                !path_is_within(path, entry.path.as_str())
                                                                            });
                                                                        tab_buffers
                                                                            .write()
                                                                            .retain(|open| {
                                                                                !path_is_within(&open.path, entry.path.as_str())
                                                                            });
                                                                        pending_delete.set(None);
                                                                        current_directory.set(refresh_directory);
                                                                        revision += 1;
                                                                        notice.set(Some(Notice::success("Deleted from this device.")));
                                                                    }
                                                                    Err(error) => notice.set(Some(Notice::error(error.message))),
                                                                }
                                                                busy.set(false);
                                                            });
                                                        }
                                                    },
                                                    on_operation: {
                                                        move |(entry, kind): (FileEntry, FileOperationKind)| {
                                                            if tab_buffers
                                                                .read()
                                                                .iter()
                                                                .any(|open| {
                                                                    open.is_dirty() && path_is_within(&open.path, entry.path.as_str())
                                                                })
                                                            {
                                                                notice.set(Some(Notice::error(UNSAVED_NAVIGATION_MESSAGE)));
                                                                return;
                                                            }
                                                            let destination = match kind {
                                                                FileOperationKind::Move => entry.path.as_str().to_owned(),
                                                                FileOperationKind::Duplicate => duplicate_path(&entry.path),
                                                            };
                                                            operation_destination.set(destination);
                                                            file_operation
                                                                .set(
                                                                    Some(FileOperation {
                                                                        source: entry.path,
                                                                        kind,
                                                                    }),
                                                                );
                                                        }
                                                    },
                                                }
                                            }
                                        },
                                    }
                                }
                            }

                            if let Some(operation) = file_operation() {
                                Modal {
                                    title: if operation.kind == FileOperationKind::Move { "Rename or move item" } else { "Duplicate item" },
                                    description: format!("Selected path: {}", operation.source.as_str()),
                                    on_close: move |()| file_operation.set(None),
                                    form {
                                        onsubmit: {
                                            let source = operation.source.clone();
                                            let kind = operation.kind;
                                            let workspace = workspace.clone();
                                            move |event| {
                                                event.prevent_default();
                                                if tab_buffers
                                                    .read()
                                                    .iter()
                                                    .any(|open| {
                                                        open.is_dirty() && path_is_within(&open.path, source.as_str())
                                                    })
                                                {
                                                    notice.set(Some(Notice::error(UNSAVED_NAVIGATION_MESSAGE)));
                                                    return;
                                                }
                                                let Ok(destination) = RelativePath::try_from(
                                                    operation_destination().trim().to_owned(),
                                                ) else {
                                                    notice
                                                        .set(
                                                            Some(
                                                                Notice::error(
                                                                    "Enter a valid workspace-relative destination.",
                                                                ),
                                                            ),
                                                        );
                                                    return;
                                                };
                                                if destination.is_root() || destination == source
                                                    || (kind == FileOperationKind::Move
                                                        && path_is_within(destination.as_str(), source.as_str()))
                                                {
                                                    notice
                                                        .set(
                                                            Some(
                                                                Notice::error(
                                                                    "Choose a different destination outside the source.",
                                                                ),
                                                            ),
                                                        );
                                                    return;
                                                }
                                                busy.set(true);
                                                notice.set(None);
                                                let source_for_task = source.clone();
                                                let workspace_for_task = workspace.clone();
                                                spawn(async move {
                                                    let result = if kind == FileOperationKind::Move {
                                                        files
                                                            .move_entry(&workspace_for_task, &source_for_task, &destination)
                                                            .await
                                                    } else {
                                                        files.copy(&workspace_for_task, &source_for_task, &destination).await
                                                    };
                                                    match result {
                                                        Ok(()) => {
                                                            if kind == FileOperationKind::Move {
                                                                for tab in open_tabs.write().iter_mut() {
                                                                    if let Some(path) = remap_path(
                                                                        tab,
                                                                        &source_for_task,
                                                                        &destination,
                                                                    ) {
                                                                        *tab = path;
                                                                    }
                                                                }
                                                                for open in tab_buffers.write().iter_mut() {
                                                                    if let Some(path) = remap_path(
                                                                        &open.path,
                                                                        &source_for_task,
                                                                        &destination,
                                                                    ) {
                                                                        open.rename(path);
                                                                    }
                                                                }
                                                                if let Some(open) = buffer.write().as_mut()
                                                                    && let Some(path) = remap_path(
                                                                        &open.path,
                                                                        &source_for_task,
                                                                        &destination,
                                                                    )
                                                                {
                                                                    open.rename(path);
                                                                }
                                                                if let Some(preview) = binary_preview.write().as_mut()
                                                                    && let Some(path) = remap_path(
                                                                        &preview.path,
                                                                        &source_for_task,
                                                                        &destination,
                                                                    )
                                                                {
                                                                    preview.path = path;
                                                                }
                                                                if path_is_within(
                                                                    current_directory().as_str(),
                                                                    source_for_task.as_str(),
                                                                ) {
                                                                    current_directory
                                                                        .set(
                                                                            destination
                                                                                .as_str()
                                                                                .rsplit_once('/')
                                                                                .map_or_else(
                                                                                    RelativePath::root,
                                                                                    |(parent, _)| {
                                                                                        RelativePath::try_from(parent.to_owned())
                                                                                            .unwrap_or_else(|_| RelativePath::root())
                                                                                    },
                                                                                ),
                                                                        );
                                                                }
                                                            }
                                                            file_operation.set(None);
                                                            revision += 1;
                                                            notice
                                                                .set(
                                                                    Some(
                                                                        Notice::success(
                                                                            if kind == FileOperationKind::Move {
                                                                                "Moved locally."
                                                                            } else {
                                                                                "Duplicated locally."
                                                                            },
                                                                        ),
                                                                    ),
                                                                );
                                                        }
                                                        Err(error) => notice.set(Some(Notice::error(error.message))),
                                                    }
                                                    busy.set(false);
                                                });
                                            }
                                        },
                                        DialogForm {
                                            Field {
                                                control_id: "guest-operation-destination",
                                                label: "Workspace-relative destination",
                                                TextInput {
                                                    value: operation_destination(),
                                                    autofocus: true,
                                                    oninput: move |event: FormEvent| operation_destination.set(event.value()),
                                                }
                                            }
                                            DialogActions {
                                                Button {
                                                    label: "Cancel",
                                                    kind: ButtonKind::Ghost,
                                                    onclick: move |event: MouseEvent| {
                                                        event.prevent_default();
                                                        file_operation.set(None);
                                                    },
                                                }
                                                button {
                                                    class: "inline-flex h-9 items-center justify-center rounded-lg bg-primary px-3.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50",
                                                    disabled: busy() || operation_destination().trim().is_empty(),
                                                    if operation.kind == FileOperationKind::Move {
                                                        "Move"
                                                    } else {
                                                        "Duplicate"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if let Some(delete_path) = pending_delete() {
                                Modal {
                                    title: "Delete item?",
                                    description: format!(
                                        "This permanently removes {} and all of its children.",
                                        delete_path.as_str(),
                                    ),
                                    on_close: move |()| pending_delete.set(None),
                                    DialogForm {
                                        p { class: "rounded-md border border-destructive/35 bg-destructive/10 px-2.5 py-2.25 text-xs text-destructive",
                                            "{delete_path.as_str()}"
                                        }
                                        DialogActions {
                                            Button {
                                                label: "Cancel",
                                                kind: ButtonKind::Ghost,
                                                onclick: move |_| pending_delete.set(None),
                                            }
                                            Button {
                                                label: "Delete",
                                                kind: ButtonKind::Danger,
                                                disabled: busy(),
                                                onclick: {
                                                    let workspace = workspace.clone();
                                                    let active_path = active_path.clone();
                                                    let delete_path = delete_path.clone();
                                                    move |_| {
                                                        if tab_buffers
                                                            .read()
                                                            .iter()
                                                            .any(|open| {
                                                                open.is_dirty() && path_is_within(&open.path, delete_path.as_str())
                                                            })
                                                        {
                                                            notice.set(Some(Notice::error(UNSAVED_NAVIGATION_MESSAGE)));
                                                            return;
                                                        }
                                                        busy.set(true);
                                                        notice.set(None);
                                                        let workspace = workspace.clone();
                                                        let delete_path = delete_path.clone();
                                                        let deleting_active = active_path
                                                            .as_deref()
                                                            .is_some_and(|path| { path_is_within(path, delete_path.as_str()) });
                                                        spawn(async move {
                                                            match files.delete(&workspace, &delete_path).await {
                                                                Ok(()) => {
                                                                    if deleting_active {
                                                                        buffer.set(None);
                                                                        binary_preview.set(None);
                                                                    }
                                                                    open_tabs
                                                                        .write()
                                                                        .retain(|path| {
                                                                            !path_is_within(path, delete_path.as_str())
                                                                        });
                                                                    tab_buffers
                                                                        .write()
                                                                        .retain(|open| {
                                                                            !path_is_within(&open.path, delete_path.as_str())
                                                                        });
                                                                    selected_tree_entry.set(None);
                                                                    pending_delete.set(None);
                                                                    explorer_tree.set(ExplorerTree::default());
                                                                    current_directory.set(RelativePath::root());
                                                                    revision += 1;
                                                                    notice.set(Some(Notice::success("Deleted from this device.")));
                                                                }
                                                                Err(error) => notice.set(Some(Notice::error(error.message))),
                                                            }
                                                            busy.set(false);
                                                        });
                                                    }
                                                },
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    section { class: "flex min-h-0 min-w-0 flex-col overflow-hidden max-md:h-full guest-editor-panel",
                        if matches!(active_view(), GuestView::Editor | GuestView::Preview) {
                            if active_view() == GuestView::Editor && !open_tabs().is_empty() {
                                nav {
                                    class: "guest-tab-strip",
                                    "aria-label": "Open files",
                                    for tab_path in open_tabs() {
                                        div { class: if active_path.as_deref() == Some(tab_path.as_str()) { "guest-tab guest-tab-active" } else { "guest-tab" },
                                            button {
                                                class: "guest-tab-open",
                                                title: "Open {tab_path}",
                                                onclick: {
                                                    let tab_path = tab_path.clone();
                                                    let workspace = workspace.clone();
                                                    let active_path = active_path.clone();
                                                    move |_| {
                                                        if active_path.as_deref() == Some(tab_path.as_str()) {
                                                            return;
                                                        }
                                                        activate_tab(
                                                            tab_path.clone(),
                                                            files,
                                                            workspace.clone(),
                                                            buffer,
                                                            tab_buffers,
                                                            binary_preview,
                                                            active_view,
                                                            busy,
                                                            notice,
                                                        );
                                                    }
                                                },
                                                "{tab_path}"
                                            }
                                            button {
                                                class: "guest-tab-close",
                                                title: "Close {tab_path}",
                                                aria_label: "Close {tab_path}",
                                                onclick: {
                                                    let tab_path = tab_path.clone();
                                                    let active_path = active_path.clone();
                                                    move |_| {
                                                        let tab_dirty = tab_buffers
                                                            .read()
                                                            .iter()
                                                            .find(|open| open.path == tab_path)
                                                            .is_some_and(EditorBuffer::is_dirty);
                                                        if tab_dirty {
                                                            notice.set(Some(Notice::error("Save this file before closing it.")));
                                                            return;
                                                        }
                                                        open_tabs.write().retain(|path| path != &tab_path);
                                                        tab_buffers.write().retain(|open| open.path != tab_path);
                                                        if active_path.as_deref() == Some(tab_path.as_str()) {
                                                            buffer.set(None);
                                                            binary_preview.set(None);
                                                        }
                                                    }
                                                },
                                                "×"
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "guest-editor-toolbar",
                                div { class: "guest-panel-heading",
                                    if active_view() == GuestView::Editor {
                                        strong { "{active_name}" }
                                        if dirty {
                                            span {
                                                class: "guest-dirty",
                                                title: "Unsaved changes",
                                                "●"
                                            }
                                        }
                                    } else if active_view() == GuestView::Terminal {
                                        strong { "Terminal" }
                                    } else if active_view() == GuestView::Preview {
                                        strong { "Preview" }
                                    } else if active_view() == GuestView::Git {
                                        strong { "Git" }
                                    } else {
                                        strong { "AI" }
                                    }
                                }
                                if active_view() == GuestView::Preview {
                                    IconButton {
                                        label: "Reload preview",
                                        icon: AppIcon::Refresh,
                                        size: ControlSize::Small,
                                        disabled: preview_loading(),
                                        onclick: {
                                            let workspace = workspace.clone();
                                            move |_| {
                                                if let Some(open) = buffer() {
                                                    load_preview(
                                                        files,
                                                        workspace.clone(),
                                                        open.path.clone(),
                                                        open.contents,
                                                        preview_source,
                                                        preview_loading,
                                                    );
                                                }
                                            }
                                        },
                                    }
                                }
                                if active_view() == GuestView::Editor {
                                    IconButton {
                                        label: "Save file",
                                        icon: AppIcon::Save,
                                        size: ControlSize::Small,
                                        disabled: busy() || !dirty,
                                        onclick: move |_| {
                                            let Some(snapshot) = buffer() else {
                                                return;
                                            };
                                            let Ok(path) = RelativePath::try_from(snapshot.path.clone()) else {
                                                notice.set(Some(Notice::error("The active file path is invalid.")));
                                                return;
                                            };
                                            let contents = snapshot.contents.clone();
                                            let preview_contents = contents.clone();
                                            let expected = snapshot.version.clone();
                                            if let Some(open) = buffer.write().as_mut() {
                                                open.begin_save(contents.clone());
                                            }
                                            let workspace = save_workspace.clone();
                                            busy.set(true);
                                            notice.set(None);
                                            spawn(async move {
                                                match files
                                                    .write_text(
                                                        &workspace,
                                                        &path,
                                                        &contents,
                                                        Some(&expected),
                                                        MAX_TEXT_BYTES,
                                                    )
                                                    .await
                                                {
                                                    Ok(version) => {
                                                        if let Some(open) = buffer.write().as_mut()
                                                            && open.path == path.as_str()
                                                        {
                                                            open.finish_save(contents, version);
                                                            cache_buffer(tab_buffers, open);
                                                        }
                                                        if is_html_path(path.as_str()) {
                                                            load_preview(
                                                                files,
                                                                workspace.clone(),
                                                                path.as_str().to_owned(),
                                                                preview_contents,
                                                                preview_source,
                                                                preview_loading,
                                                            );
                                                        }
                                                        revision += 1;
                                                        notice.set(Some(Notice::success("Saved locally.")));
                                                    }
                                                    Err(error) => {
                                                        if let Some(open) = buffer.write().as_mut() {
                                                            open.cancel_save();
                                                        }
                                                        notice.set(Some(Notice::error(error.message)));
                                                    }
                                                }
                                                busy.set(false);
                                            });
                                        },
                                    }
                                    if let Some(path) = active_path.clone() {
                                        IconButton {
                                            label: "Download file",
                                            icon: AppIcon::Share,
                                            size: ControlSize::Small,
                                            disabled: busy(),
                                            onclick: {
                                                let workspace = workspace.clone();
                                                move |_| {
                                                    let Ok(path) = RelativePath::try_from(path.clone()) else {
                                                        return;
                                                    };
                                                    let filename = path.as_str().to_owned();
                                                    let workspace = workspace.clone();
                                                    spawn(async move {
                                                        match files.read_binary(&workspace, &path, MAX_BINARY_BYTES).await {
                                                            Ok(file) => {
                                                                download_bytes(
                                                                    filename,
                                                                    file.content,
                                                                    mime_for_path(path.as_str()),
                                                                )
                                                            }
                                                            Err(error) => notice.set(Some(Notice::error(error.message))),
                                                        }
                                                    });
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }

                        div { class: "relative min-h-0 min-w-0 flex-1 overflow-auto bg-card guest-editor-surface",
                            if active_view() == GuestView::Terminal {
                                GuestTerminal {
                                    workspace: workspace.clone(),
                                    notice,
                                    on_workspace_changed: {
                                        let workspace = workspace.clone();
                                        let active_path = active_path.clone();
                                        move |changes: Vec<WorkspaceChange>| {
                                            revision += 1;
                                            open_tabs
                                                .write()
                                                .retain(|path| {
                                                    !changes
                                                        .iter()
                                                        .any(|change| {
                                                            change.path == *path
                                                                && change.kind == WorkspaceChangeKind::Deleted
                                                        })
                                                });
                                            tab_buffers
                                                .write()
                                                .retain(|open| {
                                                    !changes
                                                        .iter()
                                                        .any(|change| {
                                                            change.path == open.path
                                                                && (change.kind == WorkspaceChangeKind::Deleted
                                                                    || (change.kind == WorkspaceChangeKind::Modified
                                                                        && !open.is_dirty()))
                                                        })
                                                });
                                            let Some(path) = active_path.clone() else {
                                                return;
                                            };
                                            let affected = changes
                                                .iter()
                                                .any(|change| {
                                                    change.path == path
                                                        && matches!(
                                                            change.kind,
                                                            WorkspaceChangeKind::Modified | WorkspaceChangeKind::Deleted
                                                        )
                                                });
                                            if !affected {
                                                return;
                                            }
                                            if changes
                                                .iter()
                                                .any(|change| {
                                                    change.path == path && change.kind == WorkspaceChangeKind::Deleted
                                                })
                                            {
                                                open_tabs.write().retain(|open_path| open_path != &path);
                                                buffer.set(None);
                                                notice
                                                    .set(
                                                        Some(Notice::error("The open file was removed by the terminal.")),
                                                    );
                                                return;
                                            }
                                            let Ok(relative) = RelativePath::try_from(path.clone()) else {
                                                return;
                                            };
                                            let workspace = workspace.clone();
                                            spawn(async move {
                                                match files.read_text(&workspace, &relative, MAX_TEXT_BYTES).await {
                                                    Ok(file) => {
                                                        let file_content = file.content.clone();
                                                        if let Some(open) = buffer.write().as_mut() {
                                                            match open.reconcile_external(file.content, file.version) {
                                                                ExternalChange::Reload => {
                                                                    notice
                                                                        .set(
                                                                            Some(
                                                                                Notice::success(
                                                                                    "The terminal changed the open file; it was reloaded.",
                                                                                ),
                                                                            ),
                                                                        )
                                                                }
                                                                ExternalChange::Conflict => {
                                                                    notice
                                                                        .set(
                                                                            Some(
                                                                                Notice::error(
                                                                                    "The terminal changed the open file. Save was blocked until you resolve the conflict.",
                                                                                ),
                                                                            ),
                                                                        )
                                                                }
                                                                ExternalChange::Unchanged => {}
                                                            }
                                                            cache_buffer(tab_buffers, open);
                                                        }
                                                        if is_html_path(relative.as_str()) {
                                                            load_preview(
                                                                files,
                                                                workspace.clone(),
                                                                relative.as_str().to_owned(),
                                                                file_content,
                                                                preview_source,
                                                                preview_loading,
                                                            );
                                                        }
                                                    }
                                                    Err(error) => notice.set(Some(Notice::error(error.message))),
                                                }
                                            });
                                        }
                                    },
                                }
                            } else if active_view() == GuestView::Preview {
                                if let Some(source) = preview_source() {
                                    GuestPreview {
                                        path: preview_path.clone(),
                                        source,
                                    }
                                } else {
                                    div { class: "flex size-full items-center justify-center p-7 text-center text-sm text-muted-foreground",
                                        "Choose Preview again to reload the HTML."
                                    }
                                }
                            } else if active_view() == GuestView::Git {
                                GuestGit {
                                    workspace: workspace.clone(),
                                    revision,
                                    dirty: any_dirty,
                                    notice,
                                    on_workspace_changed: move |()| {
                                        buffer.set(None);
                                        binary_preview.set(None);
                                        open_tabs.write().clear();
                                        tab_buffers.write().clear();
                                        revision += 1;
                                    },
                                }
                            } else if active_view() == GuestView::Ai {
                                GuestAi {
                                    active_path: active_buffer.as_ref().map(|open| open.path.clone()),
                                    active_contents: active_buffer.as_ref().map(|open| open.contents.clone()),
                                }
                            } else if let Some(open) = active_buffer.as_ref() {
                                CodeEditor {
                                    key: "{open.path}",
                                    value: open.contents.clone(),
                                    filename: open.path.clone(),
                                    language_name: language_slug_for_path(&open.path),
                                    autocomplete: true,
                                    class: "guest-code-editor",
                                    aria_label: format!("Editing {}", open.path),
                                    oninput: move |edits: Vec<EditorEdit>| {
                                        let edits = edits
                                            .into_iter()
                                            .map(|edit| (edit.start, edit.end, edit.text))
                                            .collect::<Vec<_>>();
                                        if let Some(open) = buffer.write().as_mut() {
                                            if open.apply_edits(&edits) {
                                                cache_buffer(tab_buffers, open);
                                            } else {
                                                notice
                                                    .set(
                                                        Some(
                                                            Notice::error("The editor returned an invalid text change."),
                                                        ),
                                                    );
                                            }
                                        }
                                    },
                                }
                            } else if let Some(preview) = active_binary {
                                BinaryFilePreview { preview }
                            } else {
                                div { class: "flex size-full flex-col items-center justify-center p-7 text-center",
                                    h2 { class: "text-lg text-foreground", "No open files" }
                                    p { class: "mt-1.75 max-w-97.5 text-muted-foreground",
                                        "Choose a file from the explorer to open it."
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(message) = notice() {
                div {
                    class: if message.error { "guest-notice guest-notice-error" } else { "guest-notice" },
                    role: "status",
                    span { "{message.message}" }
                    button {
                        "aria-label": "Dismiss notification",
                        onclick: move |_| notice.set(None),
                        "×"
                    }
                }
            }
            nav {
                class: "flex h-[calc(3.625rem+env(safe-area-inset-bottom))] min-h-[calc(3.625rem+env(safe-area-inset-bottom))] items-stretch justify-center border-t border-border bg-background pb-[env(safe-area-inset-bottom)] max-md:h-[calc(3.875rem+env(safe-area-inset-bottom))] max-md:min-h-[calc(3.875rem+env(safe-area-inset-bottom))]",
                "aria-label": "Workspace modules",
                GuestNavItem {
                    label: "Files",
                    icon: AppIcon::Folder,
                    active: active_view() == GuestView::Editor,
                    onclick: {
                        let navigator = navigator.clone();
                        let slug = workspace.slug.clone();
                        let route_path = active_path.clone();
                        move |_| {
                            active_view.set(GuestView::Editor);
                            navigator
                                .push(GuestRoute::Files {
                                    slug: slug.clone(),
                                    query: GuestFilesQuery {
                                        path: route_path.clone(),
                                    },
                                });
                        }
                    },
                }
                GuestNavItem {
                    label: "Terminal",
                    icon: AppIcon::Terminal,
                    active: active_view() == GuestView::Terminal,
                    onclick: {
                        let navigator = navigator.clone();
                        let slug = workspace.slug.clone();
                        let route_path = active_path.clone();
                        move |_| {
                            active_view.set(GuestView::Terminal);
                            navigator
                                .push(GuestRoute::Terminal {
                                    slug: slug.clone(),
                                    query: GuestFilesQuery {
                                        path: route_path.clone(),
                                    },
                                });
                        }
                    },
                }
                GuestNavItem {
                    label: "Git",
                    icon: AppIcon::GitBranch,
                    active: active_view() == GuestView::Git,
                    onclick: {
                        let navigator = navigator.clone();
                        let slug = workspace.slug.clone();
                        move |_| {
                            active_view.set(GuestView::Git);
                            navigator
                                .push(GuestRoute::Git {
                                    slug: slug.clone(),
                                });
                        }
                    },
                }
                GuestNavItem {
                    label: "Preview",
                    icon: AppIcon::Eye,
                    active: active_view() == GuestView::Preview,
                    onclick: {
                        let navigator = navigator.clone();
                        let slug = workspace.slug.clone();
                        move |_| {
                            if has_html_preview {
                                active_view.set(GuestView::Preview);
                                navigator
                                    .push(GuestRoute::Preview {
                                        slug: slug.clone(),
                                    });
                            } else {
                                notice.set(Some(Notice::error("Open an HTML file to use Preview.")));
                            }
                        }
                    },
                }
                GuestNavItem {
                    label: "AI",
                    icon: AppIcon::Bot,
                    active: active_view() == GuestView::Ai,
                    onclick: {
                        let navigator = navigator.clone();
                        let slug = workspace.slug.clone();
                        move |_| {
                            active_view.set(GuestView::Ai);
                            navigator
                                .push(GuestRoute::Ai {
                                    slug: slug.clone(),
                                });
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn GuestTerminal(
    workspace: WorkspaceRecord,
    mut notice: Signal<Option<Notice>>,
    on_workspace_changed: EventHandler<Vec<WorkspaceChange>>,
) -> Element {
    let files = OpfsWorkspaceFiles;
    let mut command = use_signal(String::new);
    let mut shell_number = use_signal(|| 1_u32);
    let mut history = use_signal(Vec::<CommandRecord>::new);
    let mut history_cursor = use_signal(|| None::<usize>);
    let running = use_signal(|| false);
    let mut output = use_signal(|| None::<Rc<MountedData>>);
    let mut command_input = use_signal(|| None::<Rc<MountedData>>);
    let bridge_status = use_resource(|| async { wait_for_bridge().await });
    let bridge_ready = bridge_status().is_some_and(|result| result.is_ok());
    let bridge_message = match bridge_status() {
        None => "Loading the browser shell…".to_owned(),
        Some(Ok(())) => "just-bash · local sandbox".to_owned(),
        Some(Err(error)) => error,
    };
    use_effect(move || {
        let _history_length = history().len();
        let Some(output) = output() else {
            return;
        };
        spawn(async move {
            let _ = output
                .scroll(PixelsVector2D::new(0.0, f64::MAX), ScrollBehavior::Instant)
                .await;
        });
    });
    use_effect(move || {
        if !bridge_ready || command_input().is_none() {
            return;
        }
        focus_guest_terminal_input(command_input);
    });

    let command_workspace = workspace.clone();
    let submit_workspace = workspace.clone();
    rsx! {
        section { class: "guest-terminal", "aria-label": "Browser terminal",
            PanelHeader {
                PanelTabList {
                    PanelTab {
                        label: format!("shell {}", shell_number()),
                        active: true,
                        width: PanelTabWidth::Session,
                        indicator: PanelTabIndicator::Dot(Tone::Success),
                        on_select: move |_| {},
                        on_close: move |()| {
                            notice
                                .set(
                                    Some(
                                        Notice::success(
                                            "The browser shell stays available for this workspace.",
                                        ),
                                    ),
                                )
                        },
                    }
                }
                div { class: "flex items-center gap-1",
                    IconButton {
                        label: "Reset terminal",
                        icon: AppIcon::Plus,
                        size: ControlSize::Small,
                        disabled: running(),
                        onclick: move |_| {
                            shell_number += 1;
                            history.write().clear();
                            history_cursor.set(None);
                            command.set(String::new());
                            notice.set(Some(Notice::success("Started a fresh browser shell.")));
                            focus_guest_terminal_input(command_input);
                        },
                    }
                    IconButton {
                        label: "Clear terminal output",
                        icon: AppIcon::Delete,
                        size: ControlSize::Small,
                        disabled: history().is_empty(),
                        onclick: move |_| {
                            history.write().clear();
                            history_cursor.set(None);
                        },
                    }
                    IconButton {
                        label: if running() { "Stop command" } else { "Run command" },
                        icon: if running() { AppIcon::Stop } else { AppIcon::Play },
                        size: ControlSize::Small,
                        disabled: !bridge_ready,
                        onclick: move |_| {
                            if running() {
                                if let Err(error) = cancel_browser_command() {
                                    notice.set(Some(Notice::error(error)));
                                }
                            } else if command().trim().is_empty() {
                                focus_guest_terminal_input(command_input);
                            } else {
                                run_guest_command(
                                    files,
                                    command_workspace.clone(),
                                    command,
                                    history,
                                    history_cursor,
                                    running,
                                    command_input,
                                    on_workspace_changed,
                                );
                            }
                        },
                    }
                }
            }
            div {
                class: "guest-terminal-output",
                onmounted: move |event| output.set(Some(event.data())),
                role: "log",
                "aria-live": "polite",
                for record in history() {
                    article { class: "guest-command-record",
                        div { class: "guest-command-line",
                            span { "dev@browser:/workspace$" }
                            code { "{record.command}" }
                        }
                        if !record.stdout.is_empty() {
                            pre { "{record.stdout}" }
                        }
                        if !record.stderr.is_empty() {
                            pre { class: "guest-command-error", "{record.stderr}" }
                        }
                        if record.exit_code != 0 {
                            small { "Exited with {record.exit_code}" }
                        }
                        if record.reconciliation_succeeded && !record.changes.is_empty() {
                            small { class: "guest-command-changes",
                                "Workspace updated: {change_summary(&record.changes)}"
                            }
                        }
                    }
                }
                if running() {
                    p { class: "guest-terminal-running", "Running locally…" }
                }
                form {
                    class: "guest-terminal-prompt",
                    onsubmit: move |event| {
                        event.prevent_default();
                        run_guest_command(
                            files,
                            submit_workspace.clone(),
                            command,
                            history,
                            history_cursor,
                            running,
                            command_input,
                            on_workspace_changed,
                        );
                    },
                    span { "dev@browser:/workspace$" }
                    input {
                        id: "guest-terminal-command",
                        autofocus: true,
                        value: command,
                        disabled: running() || !bridge_ready,
                        autocomplete: "off",
                        autocapitalize: "off",
                        spellcheck: false,
                        title: bridge_message.clone(),
                        placeholder: "",
                        aria_label: "Browser shell command",
                        onmounted: move |event| {
                            let input = event.data();
                            command_input.set(Some(input.clone()));
                            focus_guest_terminal_input(command_input);
                        },
                        oninput: move |event| {
                            command.set(event.value());
                            history_cursor.set(None);
                        },
                        onkeydown: move |event: KeyboardEvent| {
                            let records = history();
                            if records.is_empty() || running() || !bridge_ready {
                                return;
                            }
                            match event.key() {
                                Key::ArrowUp => {
                                    event.prevent_default();
                                    let index = match history_cursor() {
                                        Some(index) => index.saturating_sub(1),
                                        None => records.len() - 1,
                                    };
                                    command.set(records[index].command.clone());
                                    history_cursor.set(Some(index));
                                }
                                Key::ArrowDown => {
                                    event.prevent_default();
                                    match history_cursor() {
                                        Some(index) if index + 1 < records.len() => {
                                            let next = index + 1;
                                            command.set(records[next].command.clone());
                                            history_cursor.set(Some(next));
                                        }
                                        Some(_) => {
                                            command.set(String::new());
                                            history_cursor.set(None);
                                        }
                                        None => {}
                                    }
                                }
                                _ => {}
                            }
                        },
                    }
                    button {
                        class: "sr-only",
                        disabled: running() || !bridge_ready || command().trim().is_empty(),
                        "Run"
                    }
                }
            }
            div {
                class: "guest-terminal-status",
                title: "Generated directories such as node_modules, target, dist, and .git are visible but their contents are excluded from the bounded shell snapshot.",
                "Browser command console · local just-bash · generated folders protected"
            }
        }
    }
}

fn run_guest_command(
    files: OpfsWorkspaceFiles,
    workspace: WorkspaceRecord,
    mut command: Signal<String>,
    mut history: Signal<Vec<CommandRecord>>,
    mut history_cursor: Signal<Option<usize>>,
    mut running: Signal<bool>,
    command_input: Signal<Option<Rc<MountedData>>>,
    on_workspace_changed: EventHandler<Vec<WorkspaceChange>>,
) {
    let value = command().trim().to_owned();
    if value.is_empty() || running() {
        return;
    }
    command.set(String::new());
    history_cursor.set(None);
    running.set(true);
    spawn(async move {
        let record = match execute_browser_command(&files, &workspace, &value).await {
            Ok(result) => {
                if result.workspace_changed {
                    on_workspace_changed.call(result.changes.clone());
                }
                CommandRecord {
                    command: value,
                    stdout: result.stdout,
                    stderr: result.stderr,
                    exit_code: result.exit_code,
                    changes: result.changes,
                    reconciliation_succeeded: result.reconciliation_succeeded,
                }
            }
            Err(error) => CommandRecord {
                command: value,
                stdout: String::new(),
                stderr: format!("{error}\n"),
                exit_code: 1,
                changes: Vec::new(),
                reconciliation_succeeded: false,
            },
        };
        let mut records = history.write();
        if records.len() >= 50 {
            records.remove(0);
        }
        records.push(record);
        drop(records);
        running.set(false);
        focus_guest_terminal_input(command_input);
    });
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GuestView {
    Editor,
    Preview,
    Terminal,
    Git,
    Ai,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CommandRecord {
    command: String,
    stdout: String,
    stderr: String,
    exit_code: i32,
    changes: Vec<WorkspaceChange>,
    reconciliation_succeeded: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveEntry {
    path: String,
    #[serde(default)]
    directory: bool,
    content: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
struct BinaryPreviewState {
    path: String,
    size: usize,
    data_url: Option<String>,
    hex: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FileOperationKind {
    Move,
    Duplicate,
}

#[derive(Clone, Debug, PartialEq)]
struct FileOperation {
    source: RelativePath,
    kind: FileOperationKind,
}

#[component]
fn BinaryFilePreview(preview: BinaryPreviewState) -> Element {
    rsx! {
        div { class: "guest-binary-preview",
            if let Some(data_url) = preview.data_url.as_deref() {
                p { "Image preview · {preview.path} · {preview.size} bytes" }
                img { src: data_url, alt: "Preview of {preview.path}" }
            } else {
                h2 { "Binary file" }
                p { "{preview.path} · {preview.size} bytes" }
                code { "{preview.hex}" }
                small { "This file is not displayed as text." }
            }
        }
    }
}

#[component]
fn GuestPreview(path: String, source: String) -> Element {
    rsx! {
        section { class: "guest-preview", "aria-label": "HTML preview",
            div { class: "guest-preview-note",
                "Sandboxed preview of {path}. Scripts and network access are disabled."
            }
            iframe {
                class: "guest-preview-frame",
                title: "Preview of {path}",
                "sandbox": "",
                srcdoc: source,
            }
        }
    }
}

#[component]
fn SearchRow(hit: BrowserSearchHit, active: bool, on_open: EventHandler<FileEntry>) -> Element {
    let entry = hit.entry.clone();
    let label = entry.path.as_str().to_owned();
    rsx! {
        button {
            class: if active { "guest-search-row guest-file-row-active" } else { "guest-search-row" },
            title: "Open {label}",
            onclick: move |_| on_open.call(entry.clone()),
            span { class: "guest-file-icon",
                if entry.kind == EntryKind::Directory {
                    "▸"
                } else {
                    "·"
                }
            }
            span { class: "guest-file-name", "{label}" }
            if hit.content_match {
                small { "content" }
            }
        }
    }
}

fn change_summary(changes: &[WorkspaceChange]) -> String {
    changes
        .iter()
        .map(|change| {
            let verb = match change.kind {
                WorkspaceChangeKind::Added => "added",
                WorkspaceChangeKind::Modified => "modified",
                WorkspaceChangeKind::Deleted => "deleted",
            };
            format!("{verb} {}", change.path)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn remember_tab(mut open_tabs: Signal<Vec<String>>, path: &str) {
    if !open_tabs.read().iter().any(|open_path| open_path == path) {
        open_tabs.write().push(path.to_owned());
    }
}

fn cache_buffer(mut tab_buffers: Signal<Vec<EditorBuffer>>, buffer: &EditorBuffer) {
    let mut buffers = tab_buffers.write();
    if let Some(cached) = buffers.iter_mut().find(|cached| cached.path == buffer.path) {
        *cached = buffer.clone();
    } else {
        buffers.push(buffer.clone());
    }
}

fn activate_tab(
    path: String,
    files: OpfsWorkspaceFiles,
    workspace: WorkspaceRecord,
    mut buffer: Signal<Option<EditorBuffer>>,
    tab_buffers: Signal<Vec<EditorBuffer>>,
    mut binary_preview: Signal<Option<BinaryPreviewState>>,
    mut active_view: Signal<GuestView>,
    mut busy: Signal<bool>,
    mut notice: Signal<Option<Notice>>,
) {
    active_view.set(GuestView::Editor);
    if let Some(cached) = tab_buffers
        .read()
        .iter()
        .find(|cached| cached.path == path)
        .cloned()
    {
        buffer.set(Some(cached));
        binary_preview.set(None);
        return;
    }
    buffer.set(None);
    binary_preview.set(None);
    busy.set(true);
    notice.set(None);
    let Ok(relative) = RelativePath::try_from(path.clone()) else {
        busy.set(false);
        notice.set(Some(Notice::error("The tab path is invalid.")));
        return;
    };
    spawn(async move {
        match files.read_text(&workspace, &relative, MAX_TEXT_BYTES).await {
            Ok(text) => buffer.set(Some(EditorBuffer::open(
                path,
                text.content,
                text.version,
                EditorConfig::default(),
            ))),
            Err(error) => notice.set(Some(Notice::error(error.message))),
        }
        busy.set(false);
    });
}

const MAX_BINARY_BYTES: u64 = 8 * 1024 * 1024;
const MAX_UPLOAD_BYTES: u64 = 8 * 1024 * 1024;

async fn export_workspace(
    files: OpfsWorkspaceFiles,
    workspace: WorkspaceRecord,
    mut busy: Signal<bool>,
    mut notice: Signal<Option<Notice>>,
) {
    busy.set(true);
    let result = async {
        let entries = collect_archive_entries(&files, &workspace).await?;
        let mut eval = document::eval(
            r#"
            const entries = await dioxus.recv();
            const bridge = globalThis.SyntaxisGuestArchive;
            if (!bridge) throw new Error("The ZIP archive bridge is unavailable.");
            await dioxus.send(Array.from(bridge.exportZip(entries)));
            "#,
        );
        eval.send(entries)
            .map_err(|error| format!("Could not prepare the workspace export: {error}"))?;
        eval.recv::<Vec<u8>>()
            .await
            .map_err(|error| format!("Could not create the workspace ZIP: {error}"))
    }
    .await;
    match result {
        Ok(bytes) => {
            download_bytes("syntaxis-workspace.zip".into(), bytes, "application/zip");
            notice.set(Some(Notice::success("Workspace ZIP downloaded.")));
        }
        Err(error) => notice.set(Some(Notice::error(error))),
    }
    busy.set(false);
}

async fn collect_archive_entries(
    files: &OpfsWorkspaceFiles,
    workspace: &WorkspaceRecord,
) -> Result<Vec<ArchiveEntry>, String> {
    let mut pending = vec![RelativePath::root()];
    let mut entries = Vec::new();
    let mut total_bytes = 0_u64;
    while let Some(directory) = pending.pop() {
        let listed = files
            .list(workspace, &directory)
            .await
            .map_err(|error| error.message)?;
        for entry in listed {
            if entry.path.as_str() == GUEST_HISTORY_PATH {
                continue;
            }
            if entries.len() >= MAX_ARCHIVE_FILES {
                return Err("The workspace contains too many entries for a ZIP export.".into());
            }
            match entry.kind {
                EntryKind::Directory => {
                    entries.push(ArchiveEntry {
                        path: format!("{}/", entry.path.as_str()),
                        directory: true,
                        content: Vec::new(),
                    });
                    pending.push(entry.path);
                }
                EntryKind::File => {
                    if entry.size > MAX_ARCHIVE_FILE_BYTES {
                        return Err(format!(
                            "{} exceeds the 8 MiB ZIP export file limit.",
                            entry.path.as_str()
                        ));
                    }
                    let file = files
                        .read_binary(workspace, &entry.path, MAX_ARCHIVE_FILE_BYTES)
                        .await
                        .map_err(|error| error.message)?;
                    total_bytes = total_bytes
                        .saturating_add(u64::try_from(file.content.len()).unwrap_or(u64::MAX));
                    if total_bytes > MAX_ARCHIVE_WORKSPACE_BYTES {
                        return Err("The workspace exceeds the 32 MiB ZIP export limit.".into());
                    }
                    entries.push(ArchiveEntry {
                        path: entry.path.as_str().to_owned(),
                        directory: false,
                        content: file.content,
                    });
                }
                EntryKind::Symlink => {}
            }
        }
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

async fn import_workspace(
    selected: dioxus::html::FileData,
    workspace: WorkspaceRecord,
    files: OpfsWorkspaceFiles,
    mut busy: Signal<bool>,
    mut revision: Signal<u64>,
    mut notice: Signal<Option<Notice>>,
) {
    if selected.size() > MAX_ARCHIVE_WORKSPACE_BYTES {
        notice.set(Some(Notice::error(
            "The ZIP file exceeds the 32 MiB limit.",
        )));
        return;
    }
    busy.set(true);
    let result = async {
        let bytes = selected
            .read_bytes()
            .await
            .map_err(|_| "Could not read the selected ZIP file.".to_owned())?;
        let mut eval = document::eval(
            r#"
            const bytes = await dioxus.recv();
            const bridge = globalThis.SyntaxisGuestArchive;
            if (!bridge) throw new Error("The ZIP archive bridge is unavailable.");
            await dioxus.send(bridge.importZip(bytes));
            "#,
        );
        eval.send(bytes.to_vec())
            .map_err(|error| format!("Could not prepare the ZIP import: {error}"))?;
        let entries = eval
            .recv::<Vec<ArchiveEntry>>()
            .await
            .map_err(|error| format!("Could not read the ZIP archive: {error}"))?;
        apply_archive_entries(&files, &workspace, entries).await
    }
    .await;
    match result {
        Ok(imported) => {
            revision += 1;
            notice.set(Some(Notice::success(format!(
                "Imported {imported} workspace entr{}.",
                if imported == 1 { "y" } else { "ies" }
            ))));
        }
        Err(error) => notice.set(Some(Notice::error(error))),
    }
    busy.set(false);
}

async fn apply_archive_entries(
    files: &OpfsWorkspaceFiles,
    workspace: &WorkspaceRecord,
    entries: Vec<ArchiveEntry>,
) -> Result<usize, String> {
    if entries.len() > MAX_ARCHIVE_FILES {
        return Err("The ZIP contains too many entries.".into());
    }
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(entries.len());
    let mut total_bytes = 0_u64;
    for entry in entries {
        let raw_path = entry.path.trim_end_matches('/');
        let path = RelativePath::try_from(raw_path.to_owned())
            .map_err(|_| format!("The ZIP contains an invalid path: {}", entry.path))?;
        if path.is_root() || !seen.insert(path.as_str().to_owned()) {
            return Err(format!(
                "The ZIP contains a duplicate or root path: {}",
                entry.path
            ));
        }
        let size = u64::try_from(entry.content.len()).unwrap_or(u64::MAX);
        if size > MAX_ARCHIVE_FILE_BYTES
            || (total_bytes.saturating_add(size) > MAX_ARCHIVE_WORKSPACE_BYTES)
        {
            return Err("The ZIP exceeds the 8 MiB file or 32 MiB workspace limit.".into());
        }
        total_bytes = total_bytes.saturating_add(size);
        normalized.push((path, entry.directory, entry.content));
    }
    normalized.sort_by_key(|(path, directory, _)| {
        (
            if *directory { 0 } else { 1 },
            path.as_str().matches('/').count(),
        )
    });
    for (path, directory, content) in &normalized {
        ensure_archive_parent(files, workspace, path).await?;
        if files.stat(workspace, path).await.is_ok() {
            return Err(format!("The destination already exists: {}", path.as_str()));
        }
        if *directory {
            files
                .create_directory(workspace, path)
                .await
                .map_err(|error| error.message)?;
        } else {
            files
                .write_binary(workspace, path, content, MAX_ARCHIVE_FILE_BYTES)
                .await
                .map_err(|error| error.message)?;
        }
    }
    Ok(normalized.len())
}

async fn ensure_archive_parent(
    files: &OpfsWorkspaceFiles,
    workspace: &WorkspaceRecord,
    path: &RelativePath,
) -> Result<(), String> {
    let mut segments = path.as_str().split('/').collect::<Vec<_>>();
    segments.pop();
    let mut current = String::new();
    for segment in segments {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(segment);
        let parent = RelativePath::try_from(current.clone())
            .map_err(|_| "The ZIP contains an invalid parent path.".to_owned())?;
        match files.stat(workspace, &parent).await {
            Ok(entry) if entry.kind == EntryKind::Directory => {}
            Ok(_) => return Err(format!("A ZIP parent is not a directory: {current}")),
            Err(error) if error.code == ErrorCode::NotFound => {
                files
                    .create_directory(workspace, &parent)
                    .await
                    .map_err(|error| error.message)?;
            }
            Err(error) => return Err(error.message),
        }
    }
    Ok(())
}

async fn upload_files(
    selected: Vec<dioxus::html::FileData>,
    workspace: WorkspaceRecord,
    directory: RelativePath,
    files: OpfsWorkspaceFiles,
    mut busy: Signal<bool>,
    mut revision: Signal<u64>,
    mut notice: Signal<Option<Notice>>,
) {
    if selected.is_empty() {
        return;
    }
    busy.set(true);
    let total = selected.len();
    let mut uploaded = 0_usize;
    let mut first_error = None;
    for file in selected {
        let name = file
            .name()
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default()
            .to_owned();
        let Ok(path) = child_path(&directory, &name) else {
            first_error.get_or_insert_with(|| format!("{name} has an invalid name."));
            continue;
        };
        if file.size() > MAX_UPLOAD_BYTES {
            first_error.get_or_insert_with(|| format!("{name} exceeds the 8 MiB limit."));
            continue;
        }
        if files.stat(&workspace, &path).await.is_ok() {
            first_error.get_or_insert_with(|| format!("{name} already exists."));
            continue;
        }
        let Ok(content) = file.read_bytes().await else {
            first_error.get_or_insert_with(|| format!("Could not read {name}."));
            continue;
        };
        match files
            .write_binary(&workspace, &path, &content, MAX_UPLOAD_BYTES)
            .await
        {
            Ok(_) => uploaded += 1,
            Err(error) => {
                first_error.get_or_insert(error.message);
            }
        }
    }
    busy.set(false);
    if uploaded > 0 {
        revision += 1;
    }
    let message = if let Some(error) = first_error {
        format!("Uploaded {uploaded} of {total} files. {error}")
    } else {
        format!("Uploaded {uploaded} file(s).")
    };
    notice.set(Some(if uploaded == total {
        Notice::success(message)
    } else {
        Notice::error(message)
    }));
}

fn download_bytes(filename: String, bytes: Vec<u8>, mime: &'static str) {
    use js_sys::{Array, Uint8Array};
    use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, Url};

    let parts = Array::new();
    parts.push(&Uint8Array::from(bytes.as_slice()));
    let options = BlobPropertyBag::new();
    options.set_type(mime);
    let Ok(blob) = Blob::new_with_u8_array_sequence_and_options(&parts, &options) else {
        return;
    };
    let Ok(url) = Url::create_object_url_with_blob(&blob) else {
        return;
    };
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        let _ = Url::revoke_object_url(&url);
        return;
    };
    let Ok(element) = document.create_element("a") else {
        let _ = Url::revoke_object_url(&url);
        return;
    };
    let Ok(link) = element.dyn_into::<HtmlAnchorElement>() else {
        let _ = Url::revoke_object_url(&url);
        return;
    };
    link.set_href(&url);
    link.set_download(filename.rsplit('/').next().unwrap_or(&filename));
    link.click();
    let _ = Url::revoke_object_url(&url);
}

fn image_mime(path: &str) -> Option<&'static str> {
    let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
    match extension.as_str() {
        "apng" => Some("image/apng"),
        "avif" => Some("image/avif"),
        "gif" => Some("image/gif"),
        "jpeg" | "jpg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "svg" => Some("image/svg+xml"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn mime_for_path(path: &str) -> &'static str {
    preview_asset_mime(path)
}

const MAX_PREVIEW_ASSET_BYTES: u64 = 1024 * 1024;
const MAX_PREVIEW_TOTAL_ASSET_BYTES: u64 = 4 * 1024 * 1024;

fn load_preview(
    files: OpfsWorkspaceFiles,
    workspace: WorkspaceRecord,
    html_path: String,
    source: String,
    mut preview_source: Signal<Option<String>>,
    mut preview_loading: Signal<bool>,
) {
    preview_loading.set(true);
    preview_source.set(None);
    spawn(async move {
        let source = prepare_html_preview(&files, &workspace, &html_path, source).await;
        preview_source.set(Some(source));
        preview_loading.set(false);
    });
}

async fn prepare_html_preview<F>(
    files: &F,
    workspace: &WorkspaceRecord,
    html_path: &str,
    mut source: String,
) -> String
where
    F: WorkspaceFiles,
{
    let mut loaded = Vec::<(String, String)>::new();
    let mut total_bytes = 0_u64;
    let references = preview_references(&source);
    let mut replacements = Vec::new();
    for (start, end, reference) in references {
        let Some(path) = resolve_preview_path(html_path, &reference) else {
            continue;
        };
        if let Some((_, data_url)) = loaded
            .iter()
            .find(|(loaded_path, _)| loaded_path == path.as_str())
        {
            replacements.push((start, end, data_url.clone()));
            continue;
        }
        if total_bytes >= MAX_PREVIEW_TOTAL_ASSET_BYTES {
            continue;
        }
        let Ok(file) = files
            .read_binary(workspace, &path, MAX_PREVIEW_ASSET_BYTES)
            .await
        else {
            continue;
        };
        let file_size = u64::try_from(file.content.len()).unwrap_or(u64::MAX);
        if total_bytes.saturating_add(file_size) > MAX_PREVIEW_TOTAL_ASSET_BYTES {
            continue;
        }
        total_bytes = total_bytes.saturating_add(file_size);
        let data_url = format!(
            "data:{};base64,{}",
            preview_asset_mime(path.as_str()),
            BASE64.encode(file.content),
        );
        loaded.push((path.as_str().to_owned(), data_url));
        if let Some((_, data_url)) = loaded
            .iter()
            .find(|(loaded_path, _)| loaded_path == path.as_str())
        {
            replacements.push((start, end, data_url.clone()));
        }
    }
    for (start, end, replacement) in replacements.into_iter().rev() {
        source.replace_range(start..end, &replacement);
    }
    source
}

fn preview_references(source: &str) -> Vec<(usize, usize, String)> {
    let mut references = Vec::new();
    for attribute in ["src", "href"] {
        for quote in ['"', '\''] {
            let needle = format!("{attribute}={quote}");
            let mut cursor = 0_usize;
            while let Some(relative_start) = source[cursor..].find(&needle) {
                let start = cursor + relative_start;
                let value_start = start + needle.len();
                let Some(relative_end) = source[value_start..].find(quote) else {
                    break;
                };
                let end = value_start + relative_end;
                references.push((value_start, end, source[value_start..end].to_owned()));
                cursor = end + quote.len_utf8();
            }
        }
    }
    references.sort_unstable_by_key(|(start, _, _)| *start);
    references
}

fn resolve_preview_path(document_path: &str, reference: &str) -> Option<RelativePath> {
    let reference = reference
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    if reference.is_empty()
        || reference.starts_with('/')
        || reference.starts_with("#")
        || reference.starts_with("data:")
        || reference.starts_with("blob:")
        || reference.starts_with("javascript:")
        || reference.contains("://")
    {
        return None;
    }

    let mut segments = document_path
        .rsplit_once('/')
        .map_or_else(Vec::new, |(parent, _)| {
            parent.split('/').map(str::to_owned).collect()
        });
    for segment in reference.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            segment => segments.push(segment.to_owned()),
        }
    }
    RelativePath::try_from(segments.join("/")).ok()
}

fn preview_asset_mime(path: &str) -> &'static str {
    if let Some(mime) = image_mime(path) {
        return mime;
    }
    let extension = path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "css" => "text/css",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "md" | "markdown" => "text/markdown",
        "txt" => "text/plain",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn is_html_path(path: &str) -> bool {
    matches!(
        path.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase()),
        Some(extension) if matches!(extension.as_str(), "htm" | "html")
    )
}

fn hex_preview(bytes: &[u8]) -> String {
    let shown = bytes.iter().take(64).copied().collect::<Vec<_>>();
    let mut result = shown
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    if bytes.len() > shown.len() {
        result.push_str(" …");
    }
    result
}

fn duplicate_path(path: &RelativePath) -> String {
    let (stem, extension) = path
        .as_str()
        .rsplit_once('.')
        .filter(|(_, extension)| !extension.contains('/'))
        .unwrap_or((path.as_str(), ""));
    if extension.is_empty() {
        format!("{stem}-copy")
    } else {
        format!("{stem}-copy.{extension}")
    }
}

fn path_is_within(path: &str, parent: &str) -> bool {
    path == parent
        || path
            .strip_prefix(parent)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn remap_path(path: &str, source: &RelativePath, destination: &RelativePath) -> Option<String> {
    if path == source.as_str() {
        return Some(destination.as_str().to_owned());
    }
    path.strip_prefix(source.as_str())
        .filter(|suffix| suffix.starts_with('/'))
        .map(|suffix| format!("{}{}", destination.as_str(), suffix))
}

#[component]
fn FileRow(
    node: ExplorerNode,
    active: bool,
    confirm_delete: bool,
    on_open: EventHandler<FileEntry>,
    on_delete: EventHandler<FileEntry>,
    on_operation: EventHandler<(FileEntry, FileOperationKind)>,
) -> Element {
    let entry = node.entry;
    let label = entry.name.clone();
    let kind = entry.kind;
    let is_directory = kind == EntryKind::Directory;
    let padding = 6 + node.depth * 14;
    let _row_actions = (on_delete, on_operation, confirm_delete);
    rsx! {
        div {
            class: if active { "guest-file-row guest-file-row-active" } else { "guest-file-row" },
            role: "treeitem",
            "aria-selected": active,
            "aria-expanded": is_directory.then_some(node.expanded),
            button {
                class: "flex h-full min-h-7.25 min-w-0 flex-1 items-center gap-1.5 rounded-sm border-0 bg-transparent pr-1.5 text-left text-xs text-foreground/90 outline-none hover:bg-accent focus-visible:ring-1 focus-visible:ring-ring",
                style: "padding-left: {padding}px",
                title: label.clone(),
                onclick: {
                    let entry = entry.clone();
                    move |_| on_open.call(entry.clone())
                },
                span { class: "w-2.25 shrink-0 text-[9px] text-muted-foreground",
                    if is_directory {
                        if node.expanded {
                            "▾"
                        } else {
                            "▸"
                        }
                    }
                }
                FileIcon {
                    path: entry.path.as_str().to_owned(),
                    directory: is_directory,
                    expanded: node.expanded,
                    size: 15,
                }
                span { class: "truncate", "{label}" }
            }
        }
    }
}

#[component]
fn GuestProjectIcon(name: String) -> Element {
    let initial = name.trim().chars().next().map_or_else(
        || "?".to_owned(),
        |character| character.to_uppercase().collect(),
    );
    rsx! {
        span { class: "grid size-7 shrink-0 place-items-center overflow-hidden rounded-md border border-border/70 bg-muted/50 text-[9px] font-bold text-muted-foreground",
            {initial}
        }
    }
}

#[component]
fn GuestNavItem(
    label: String,
    icon: AppIcon,
    active: bool,
    #[props(default = false)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            class: if active { "flex w-26 flex-col items-center justify-center gap-1 border-t-2 border-transparent bg-transparent px-2.5 pt-2 pb-1.5 text-foreground max-md:w-1/5 max-md:pb-2" } else { "flex w-26 flex-col items-center justify-center gap-1 border-t-2 border-transparent bg-transparent px-2.5 pt-2 pb-1.5 text-muted-foreground hover:bg-accent/50 hover:text-foreground max-md:w-1/5 max-md:pb-2" },
            disabled,
            aria_current: if active { "page" } else { "" },
            onclick: move |event| onclick.call(event),
            span { class: "h-5 text-base leading-5",
                Icon { icon, size: 18 }
            }
            small { class: "text-[10px]", {label} }
        }
    }
}

fn focus_guest_terminal_input(input: Signal<Option<Rc<MountedData>>>) {
    let Some(input) = input() else {
        return;
    };
    spawn(async move {
        let _ = input.set_focus(true).await;
    });
}

#[derive(Clone, Debug, PartialEq)]
struct Notice {
    message: String,
    error: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum StorageLocation {
    Private,
    Local(String),
}

impl StorageLocation {
    fn badge_label(&self) -> String {
        match self {
            Self::Private => "Private".into(),
            Self::Local(_) => "Local".into(),
        }
    }
}

impl Notice {
    fn success(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            error: false,
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            error: true,
        }
    }
}

fn guest_workspace(slug: String) -> WorkspaceRecord {
    let name = if slug == "browser" {
        "Browser workspace".to_owned()
    } else {
        slug.replace('-', " ")
    };
    WorkspaceRecord {
        id: WorkspaceId::new("browser-opfs"),
        slug,
        name,
        root: "opfs://syntaxis-guest".into(),
        icon: WorkspaceIcon::Symbol {
            name: WorkspaceIconSymbol::Folder,
        },
        profile: WorkspaceProfile::default(),
        registered_at_unix_ms: 0,
        last_opened_unix_ms: 0,
        last_section: WorkspaceSection::Files,
        availability: WorkspaceAvailability::Available,
    }
}

fn slug_for_project(name: &str) -> String {
    let slug = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "browser".to_owned()
    } else {
        slug.to_owned()
    }
}

async fn populate_tree_to_file(
    files: &OpfsWorkspaceFiles,
    workspace: &WorkspaceRecord,
    path: &RelativePath,
    mut tree: Signal<ExplorerTree>,
) {
    let segments = path
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let mut directory = RelativePath::root();
    for segment in segments.iter().take(segments.len().saturating_sub(1)) {
        let Ok(items) = files.list(workspace, &directory).await else {
            return;
        };
        tree.write().replace_directory(directory.as_str(), items);
        let Ok(next) = child_path(&directory, segment) else {
            return;
        };
        tree.write().expand(next.as_str());
        directory = next;
    }
    if let Ok(items) = files.list(workspace, &directory).await {
        tree.write().replace_directory(directory.as_str(), items);
    }
}

fn path_contains_bulky_generated_directory(path: &str) -> bool {
    path.split('/').any(is_bulky_generated_directory_name)
}

fn expand_tree_path(mut tree: Signal<ExplorerTree>, path: &str) {
    let mut parent = String::new();
    for segment in path.split('/').filter(|segment| !segment.is_empty()) {
        if !parent.is_empty() {
            parent.push('/');
        }
        parent.push_str(segment);
        tree.write().expand(&parent);
    }
}

fn child_path(parent: &RelativePath, name: &str) -> Result<RelativePath, ()> {
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        return Err(());
    }
    let path = if parent.is_root() {
        name.to_owned()
    } else {
        format!("{}/{}", parent.as_str(), name)
    };
    RelativePath::try_from(path).map_err(|_| ())
}

fn parent_path(path: &RelativePath) -> RelativePath {
    path.as_str()
        .rsplit_once('/')
        .map_or_else(RelativePath::root, |(parent, _)| {
            RelativePath::try_from(parent).unwrap_or_else(|_| RelativePath::root())
        })
}

fn display_path(path: &RelativePath) -> String {
    if path.is_root() {
        "Browser workspace".into()
    } else {
        format!("Browser workspace / {}", path.as_str())
    }
}
