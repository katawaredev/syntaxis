#![allow(
    clippy::clone_on_ref_ptr,
    clippy::redundant_locals,
    clippy::no_effect_underscore_binding,
    reason = "Route handlers capture runtime-specific ports and reactive refresh signals"
)]

use dioxus::prelude::*;
use dioxus::router::Navigator;
use futures_util::{
    FutureExt, StreamExt,
    future::{Either, select},
    pin_mut,
};
use syntaxis_app_contracts::{AppError, ChangeOrigin, NavigationIntent};
use syntaxis_git::{CloneMode, ClonePhase, CloneProgress, CloneRequest, WorktreeInfo};
use syntaxis_module_files::{FilesPorts, FilesQuery};
use syntaxis_module_terminal::TerminalQuery;
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, DialogActions, DialogForm, Field, Icon, Modal, ProjectIcon,
    Select, SkipLink, StatusBadge, TemplateIcon, TextInput, Toast, Tone, WorkspaceSourceAction,
    WorkspaceSourceFileAction,
};
use syntaxis_workspace::{
    ExecutionLocation, RuntimeCapability, RuntimeState, WorkspaceAvailability, WorkspaceRecord,
    WorkspaceSection,
};

use crate::notifications::{NotificationMenu, use_notification_center};
use crate::{
    AiQuery, AiSettingsSection, AppServices, AuthAction, WorkspaceCloneEvent, WorkspaceShellFrame,
    project_templates::{
        CATEGORIES, ProjectTemplate, TEMPLATES, category_has_matches, definition,
        matches as template_matches,
    },
    use_active_workspace,
};

#[component]
pub fn SyntaxisApp() -> Element {
    let services = use_context::<AppServices>();
    let notifications = use_notification_center(services.notifications().cloned());
    use_context_provider(|| notifications);
    let ai_ui = syntaxis_module_ai::use_ai_ui_state();
    use_context_provider(|| ai_ui);
    rsx! { Router::<Route> {} }
}

#[derive(Clone, Debug, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[route("/")]
    Home {},
    #[layout(WorkspaceShell)]
    #[route("/workspaces/:slug/files?:..query")]
    Files { slug: String, query: FilesQuery },
    #[route("/workspaces/:slug/terminal?:..query")]
    Terminal { slug: String, query: TerminalQuery },
    #[route("/workspaces/:slug/git")]
    Git { slug: String },
    #[route("/workspaces/:slug/preview")]
    Preview { slug: String },
    #[route("/workspaces/:slug/ai?:..query")]
    Ai { slug: String, query: AiQuery },
    #[redirect("/workspaces/:slug/ai/settings", |slug: String| Route::AiSettings {
        slug,
        section: AiSettingsSection::General,
    })]
    #[route("/workspaces/:slug/ai/settings/:section")]
    AiSettings { slug: String, section: AiSettingsSection },
}

impl Route {
    #[must_use]
    pub fn for_workspace_section(slug: String, section: WorkspaceSection) -> Self {
        match section {
            WorkspaceSection::Files => Self::Files {
                slug,
                query: FilesQuery::default(),
            },
            WorkspaceSection::Terminal => Self::Terminal {
                slug,
                query: TerminalQuery::default(),
            },
            WorkspaceSection::Git => Self::Git { slug },
            WorkspaceSection::Preview => Self::Preview { slug },
            WorkspaceSection::Ai => Self::Ai {
                slug,
                query: AiQuery::default(),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum HomeDialog {
    #[default]
    None,
    Folder,
    Git,
    Project,
}

#[component]
fn Home() -> Element {
    let services = use_context::<AppServices>();
    let terminal = services
        .terminal()
        .cloned()
        .expect("Terminal services are required");
    use_context_provider(|| terminal);
    let catalog = services.workspace_catalog().cloned();
    let runtime_status = services.runtime_status().cloned();
    let runtime = use_resource(move || {
        let runtime_status = runtime_status.clone();
        async move {
            match runtime_status {
                Some(status) => status.state().await,
                None => Ok(RuntimeState::Connecting),
            }
        }
    });
    let runtime_presentation = runtime_presentation(runtime().as_ref());
    let auth_action = services.auth_action().cloned();
    let files = services.files().cloned();
    let mut workspaces = use_resource(move || {
        let catalog = catalog.clone();
        async move {
            match catalog {
                Some(catalog) => catalog.list().await,
                None => Ok(Vec::new()),
            }
        }
    });
    let mut notice = use_signal(|| None::<(String, Tone)>);
    let mut busy = use_signal(|| false);
    let mut transfer_busy = use_signal(|| false);
    let mut dialog = use_signal(HomeDialog::default);
    let navigator = use_navigator();
    let local_folders = files.as_ref().and_then(FilesPorts::local_folders).cloned();
    let transfer = files.as_ref().and_then(FilesPorts::transfer).cloned();
    let can_pick_folder = local_folders
        .as_ref()
        .is_some_and(|port| port.picker_supported());
    rsx! {
        document::Title { "Home · Syntaxis" }
        main { class: "app-viewport relative w-full overflow-x-hidden overflow-y-auto overscroll-contain bg-background",
            SkipLink { target_id: "home-main-content" }
            section { id: "home-main-content", tabindex: "-1", class: "mx-auto flex min-h-full w-[calc(100%-2.5rem)] max-w-205 flex-col pt-[max(9vh,env(safe-area-inset-top))] pb-[max(1.5rem,env(safe-area-inset-bottom))] max-md:w-[calc(100%-1.5rem)] max-md:max-w-155 max-md:pt-[max(2.125rem,env(safe-area-inset-top))]",
                header { class: "mb-9.5 flex items-start justify-between gap-4 max-md:mb-6.5",
                    div { class: "min-w-0",
                        p { class: "text-[10px] font-bold tracking-[.14em] text-primary max-[420px]:hidden", "{runtime_presentation.eyebrow}" }
                        h1 { class: "mt-1 text-4xl font-semibold tracking-tight text-foreground max-md:text-3xl max-[420px]:mt-0 max-[420px]:text-2xl", "Welcome back!" }
                        p { class: "mt-1 text-[15px] text-muted-foreground max-[420px]:text-[13px]", "Pick up where you left off or open another project." }
                    }
                    div { class: "flex items-center gap-1",
                        if services.notifications().is_some() { NotificationMenu {} }
                        if let Some(action) = auth_action { AuthActionButton { action } }
                    }
                }
                if services.workspace_folders().is_some() {
                    div { class: "mb-10.5 grid grid-cols-3 gap-3 max-md:mb-8 max-md:grid-cols-1",
                        WorkspaceSourceAction {
                            icon: AppIcon::Folder,
                            title: runtime_presentation.folder_action_title.clone(),
                            description: runtime_presentation.folder_action_description.clone(),
                            onclick: move |_| dialog.set(HomeDialog::Folder),
                        }
                        WorkspaceSourceAction {
                            icon: AppIcon::FolderGit2,
                            title: "Open Git URL",
                            description: "Clone a Git repository",
                            disabled: services.workspace_clone().is_none(),
                            onclick: move |_| dialog.set(HomeDialog::Git),
                        }
                        WorkspaceSourceAction {
                            icon: AppIcon::FolderPlus,
                            title: "New project",
                            description: "Scaffold in a live terminal",
                            disabled: services.workspace_projects().is_none(),
                            onclick: move |_| dialog.set(HomeDialog::Project),
                        }
                    }
                } else if local_folders.is_some() {
                    div { class: "mb-10 grid grid-cols-3 gap-3 max-md:grid-cols-1",
                        if services.workspace_clone().is_some() {
                            WorkspaceSourceAction {
                                icon: AppIcon::FolderGit2,
                                title: "Open Git URL",
                                description: "Clone a Git repository",
                                disabled: busy() || transfer_busy(),
                                onclick: move |_| dialog.set(HomeDialog::Git),
                            }
                        }
                        WorkspaceSourceAction {
                            icon: AppIcon::Folder,
                            title: "Open folder",
                            description: if can_pick_folder { "Use a folder from this device" } else { "Unavailable in this browser" },
                            disabled: !can_pick_folder || busy(),
                            onclick: {
                                let local_folders = local_folders.clone();
                                let navigator = navigator;
                                move |_| {
                                    let Some(local_folders) = local_folders.clone() else { return; };
                                    busy.set(true);
                                    let navigator = navigator;
                                    spawn(async move {
                                        match local_folders.select().await {
                                            Ok(workspace) => { navigator.push(Route::Files { slug: workspace.slug, query: FilesQuery::default() }); }
                                            Err(error) => notice.set(Some((error.message, Tone::Destructive))),
                                        }
                                        busy.set(false);
                                    });
                                }
                            },
                        }
                        WorkspaceSourceAction {
                            icon: AppIcon::FolderPlus,
                            title: "Browser workspace",
                            description: "Use private browser storage",
                            disabled: busy(),
                            onclick: {
                                let local_folders = local_folders.clone();
                                let navigator = navigator;
                                move |_| if let Some(port) = local_folders.as_ref() {
                                    let workspace = port.use_private_storage();
                                    navigator.push(Route::Files { slug: workspace.slug, query: FilesQuery::default() });
                                }
                            },
                        }
                        WorkspaceSourceFileAction {
                            icon: AppIcon::Upload,
                            title: "Open ZIP archive",
                            description: "Import a bounded workspace archive",
                            accept: ".zip,application/zip",
                            disabled: busy() || transfer_busy() || transfer.is_none(),
                            on_select: {
                                let transfer = transfer.clone();
                                let local_folders = local_folders.clone();
                                let navigator = navigator;
                                move |selected: Vec<dioxus::html::FileData>| {
                                    let (Some(file), Some(transfer), Some(folders)) = (selected.into_iter().next(), transfer.clone(), local_folders.clone()) else { return; };
                                    transfer_busy.set(true);
                                    let navigator = navigator;
                                    spawn(async move {
                                        let workspace = folders.use_private_storage();
                                        let result = match file.read_bytes().await {
                                            Ok(bytes) => transfer.import_archive(&workspace, bytes.to_vec()).await,
                                            Err(_) => Err(syntaxis_app_contracts::AppError::new(
                                                syntaxis_app_contracts::AppErrorCode::InvalidInput,
                                                "Could not read the selected ZIP file.",
                                                syntaxis_app_contracts::RetryAdvice::Never,
                                                syntaxis_app_contracts::ErrorSource::Files,
                                            )),
                                        };
                                        match result {
                                            Ok(summary) => {
                                                notice.set(Some((format!("Imported {} entries.", summary.entries), Tone::Success)));
                                                navigator.push(Route::Files { slug: workspace.slug, query: FilesQuery::default() });
                                            }
                                            Err(error) => notice.set(Some((error.message, Tone::Destructive))),
                                        }
                                        transfer_busy.set(false);
                                    });
                                }
                            },
                        }
                    }
                }
                if transfer_busy() {
                    div { class: "mb-6 flex items-center justify-between rounded-xl border border-border bg-card px-4 py-3",
                        p { class: "text-xs text-muted-foreground", "Importing workspace archive…" }
                        Button { label: "Cancel import", kind: ButtonKind::Ghost, onclick: {
                            let transfer = transfer.clone();
                            move |_| {
                                if let Some(transfer) = transfer.as_ref() {
                                    let _ = transfer.cancel();
                                }
                            }
                        } }
                    }
                }
                if services.workspace_management().is_some() && files.is_some() {
                    match workspaces() {
                        None => rsx! { crate::home_management::ManagedRecentProjectsLoading {} },
                        Some(Err(_)) => rsx! { crate::home_management::ManagedRecentProjectsError { on_retry: move |()| workspaces.restart() } },
                        Some(Ok(items)) => rsx! { crate::home_management::ManagedRecentProjects {
                            workspaces: items,
                            on_changed: move |()| workspaces.restart(),
                            on_notice: move |message| notice.set(Some(message)),
                        } },
                    }
                } else {
                    section { "aria-labelledby": "recent-title",
                        h2 { id: "recent-title", class: "mb-3 text-[17px] font-semibold text-muted-foreground", "Recent projects" }
                        div { class: "grid gap-2",
                            match workspaces() {
                                None => rsx! { p { class: "text-sm text-muted-foreground", "aria-label": "Loading recent projects", "Loading workspaces…" } },
                                Some(Err(error)) => rsx! { p { class: "text-sm text-destructive", "{error.message}" } },
                                Some(Ok(items)) if items.is_empty() => rsx! { p { class: "rounded-xl border border-border bg-card p-4 text-sm text-muted-foreground", "No recent workspaces." } },
                                Some(Ok(items)) => rsx! {
                                    for workspace in items {
                                        {
                                            let available = workspace.availability == WorkspaceAvailability::Available;
                                            rsx! { Link {
                                                class: if available { "flex min-h-20 items-center gap-3 rounded-xl border border-border bg-card px-4 py-3 hover:bg-accent" } else { "flex min-h-20 items-center gap-3 rounded-xl border border-border bg-card px-4 py-3 opacity-65" },
                                                to: Route::for_workspace_section(workspace.slug.clone(), workspace.last_section),
                                                onclick: move |event: MouseEvent| if !available { event.prevent_default(); },
                                                ProjectIcon { name: workspace.name.clone(), icon: workspace.icon.clone() }
                                                div { class: "min-w-0 flex-1",
                                                    strong { class: "block truncate text-sm", "{workspace.name}" }
                                                    small { class: "block truncate font-mono text-[11px] text-muted-foreground", "{workspace.root}" }
                                                }
                                                if workspace.availability == WorkspaceAvailability::Missing {
                                                    StatusBadge { label: "Missing", tone: Tone::Destructive }
                                                } else if workspace.availability == WorkspaceAvailability::Checking {
                                                    StatusBadge { label: "Checking", tone: Tone::Neutral }
                                                }
                                            } }
                                        }
                                    }
                                },
                            }
                        }
                    }
                }
                footer { class: "mt-auto pt-10 text-center text-[11px] text-muted-foreground", "{runtime_presentation.label}" }
            }
        }
        match dialog() {
            HomeDialog::Folder => rsx! { WorkspaceFolderDialog { dialog, title: runtime_presentation.folder_action_title.clone(), description: runtime_presentation.folder_dialog_description.clone() } },
            HomeDialog::Git => rsx! { WorkspaceCloneDialog { dialog } },
            HomeDialog::Project => rsx! { WorkspaceProjectDialog {
                dialog,
                on_changed: move |()| workspaces.restart(),
                on_notice: move |message| notice.set(Some((message, Tone::Neutral))),
            } },
            HomeDialog::None => rsx! {},
        }
        if let Some((message, tone)) = notice() { Toast { message, tone, on_close: move |()| notice.set(None) } }
    }
}

#[component]
fn WorkspaceFolderDialog(
    mut dialog: Signal<HomeDialog>,
    title: String,
    description: String,
) -> Element {
    let services = use_context::<AppServices>();
    let Some(port) = services.workspace_folders().cloned() else {
        return rsx! {};
    };
    let mut path = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let roots_port = port.clone();
    let roots = use_resource(move || {
        let port = roots_port.clone();
        async move { port.roots().await }
    });
    use_effect(move || {
        if !path().trim().is_empty() {
            return;
        }
        if let Some(Ok(items)) = roots()
            && let Some(root) = items.first()
        {
            path.set(root.path.clone());
        }
    });
    let directory_port = port.clone();
    let directories = use_resource(move || {
        let port = directory_port.clone();
        let path = path();
        async move {
            if path.trim().is_empty() {
                Ok(Vec::new())
            } else {
                port.directories(&path).await
            }
        }
    });
    let navigator = use_navigator();
    rsx! {
        Modal {
            title,
            description,
            on_close: move |()| if !busy() { dialog.set(HomeDialog::None) },
            DialogForm {
                Field { control_id: "workspace-folder", label: "Folder path", error: error(),
                    TextInput {
                        value: path(),
                        placeholder: "/Projects/example",
                        autofocus: true,
                        disabled: busy(),
                        oninput: move |event: FormEvent| { path.set(event.value()); error.set(None); },
                    }
                }
                div { class: "max-h-56 overflow-y-auto rounded-md border border-border bg-background p-1.5",
                    p { class: "px-2 py-1 text-[10px] font-bold tracking-widest text-muted-foreground", "DIRECTORIES" }
                    if path().trim().is_empty() {
                        match roots() {
                            None => rsx! { p { class: "px-2 py-2 text-xs text-muted-foreground", "Loading folders…" } },
                            Some(Err(problem)) => rsx! { p { class: "px-2 py-2 text-xs text-destructive", role: "alert", "{problem.message}" } },
                            Some(Ok(items)) => rsx! {
                                for root in items {
                                    {
                                        let target = root.path.clone();
                                        rsx! { button { r#type: "button", class: "flex w-full items-center gap-2 rounded-sm bg-transparent px-2 py-1.5 text-left text-xs hover:bg-accent", onclick: move |_| path.set(target.clone()),
                                            Icon { icon: AppIcon::Folder, size: 14 }
                                            span { class: "min-w-0",
                                                strong { class: "block truncate", "{root.name}" }
                                                small { class: "block truncate font-mono text-[10px] text-muted-foreground", "{root.path}" }
                                            }
                                        } }
                                    }
                                }
                            },
                        }
                    } else {
                        if let Some(parent) = parent_folder(&path()) {
                            button { r#type: "button", class: "flex w-full items-center gap-2 rounded-sm bg-transparent px-2 py-1.5 text-left text-xs text-muted-foreground hover:bg-accent", onclick: move |_| path.set(parent.clone()), span { "←" } "Up one folder" }
                        }
                        match directories() {
                            None => rsx! { p { class: "px-2 py-2 text-xs text-muted-foreground", "Loading folders…" } },
                            Some(Err(problem)) => rsx! { p { class: "px-2 py-2 text-xs text-destructive", role: "alert", "{problem.message}" } },
                            Some(Ok(items)) if items.is_empty() => rsx! { p { class: "px-2 py-2 text-xs text-muted-foreground", "No folders inside this directory." } },
                            Some(Ok(items)) => rsx! {
                                for item in items {
                                    {
                                        let target = item.path.clone();
                                        rsx! { button { r#type: "button", class: "flex w-full items-center gap-2 rounded-sm bg-transparent px-2 py-1.5 text-left text-xs hover:bg-accent", title: item.path.clone(), onclick: move |_| path.set(target.clone()),
                                            Icon { icon: AppIcon::Folder, size: 14 }
                                            span { class: "truncate", "{item.name}" }
                                        } }
                                    }
                                }
                            },
                        }
                    }
                }
                if busy() {
                    p { class: "flex min-h-9 items-center gap-2 rounded-md border border-primary/30 bg-primary/10 px-2.5 py-2 text-[11px] text-primary", role: "status",
                        span { class: "size-3.5 shrink-0 animate-spin rounded-full border-2 border-primary/30 border-t-primary" }
                        "Checking folder and registering workspace…"
                    }
                }
                DialogActions {
                    Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: busy(), onclick: move |_| dialog.set(HomeDialog::None) }
                    Button {
                        label: if busy() { "Opening…" } else { "Open folder" },
                        kind: ButtonKind::Primary,
                        disabled: busy() || path().trim().is_empty(),
                        onclick: move |_| {
                            busy.set(true);
                            error.set(None);
                            let selected = path().trim().to_owned();
                            let port = port.clone();
                            let navigator = navigator;
                            spawn(async move {
                                match port.register(&selected).await {
                                    Ok(workspace) => { navigator.push(Route::Files { slug: workspace.slug, query: FilesQuery::default() }); }
                                    Err(problem) => { error.set(Some(problem.message)); busy.set(false); }
                                }
                            });
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn WorkspaceCloneDialog(mut dialog: Signal<HomeDialog>) -> Element {
    let services = use_context::<AppServices>();
    let Some(port) = services.workspace_clone().cloned() else {
        return rsx! {};
    };
    use_context_provider(|| services.git().cloned().unwrap_or_default());
    let supports_blobless = port.supports_blobless();
    let destination_description = port.destination_description();
    let mut url = use_signal(String::new);
    let mut destination = use_signal(|| "/".to_owned());
    let mut clone_mode = use_signal(CloneMode::default);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut progress = use_signal(|| None::<CloneProgress>);
    let mut paste_pending = use_signal(|| false);
    let navigator = use_navigator();
    let clone_client = use_coroutine(
        move |mut commands: UnboundedReceiver<WorkspaceCloneCommand>| {
            let port = port.clone();
            async move {
                while let Some(command) = commands.next().await {
                    let WorkspaceCloneCommand::Start(request) = command else {
                        continue;
                    };
                    let mut stream = match port.start(request).await {
                        Ok(stream) => stream,
                        Err(problem) => {
                            error.set(Some(problem.message));
                            busy.set(false);
                            progress.set(None);
                            continue;
                        }
                    };
                    loop {
                        let outcome = {
                            let outgoing = commands.next().fuse();
                            let incoming = stream.receive().fuse();
                            pin_mut!(outgoing, incoming);
                            match select(outgoing, incoming).await {
                                Either::Left((Some(WorkspaceCloneCommand::Cancel), _)) => {
                                    WorkspaceClonePoll::Cancel
                                }
                                Either::Left((Some(WorkspaceCloneCommand::Start(_)), _)) => {
                                    WorkspaceClonePoll::IgnoreStart
                                }
                                Either::Left((None, _)) => WorkspaceClonePoll::CommandsClosed,
                                Either::Right((event, _)) => WorkspaceClonePoll::Event(event),
                            }
                        };
                        match outcome {
                            WorkspaceClonePoll::Cancel => {
                                if let Err(problem) = stream.cancel().await {
                                    error.set(Some(problem.message));
                                    busy.set(false);
                                    progress.set(None);
                                    break;
                                }
                            }
                            WorkspaceClonePoll::IgnoreStart
                            | WorkspaceClonePoll::Event(Ok(Some(WorkspaceCloneEvent::Started))) => {
                            }
                            WorkspaceClonePoll::CommandsClosed => return,
                            WorkspaceClonePoll::Event(Ok(Some(WorkspaceCloneEvent::Progress(
                                update,
                            )))) => progress.set(Some(update)),
                            WorkspaceClonePoll::Event(Ok(Some(
                                WorkspaceCloneEvent::Completed(workspace),
                            ))) => {
                                let workspace = *workspace;
                                busy.set(false);
                                progress.set(None);
                                dialog.set(HomeDialog::None);
                                navigator.push(Route::Files {
                                    slug: workspace.slug,
                                    query: FilesQuery::default(),
                                });
                                break;
                            }
                            WorkspaceClonePoll::Event(Ok(
                                Some(WorkspaceCloneEvent::Cancelled) | None,
                            )) => {
                                busy.set(false);
                                progress.set(None);
                                break;
                            }
                            WorkspaceClonePoll::Event(Err(problem)) => {
                                error.set(Some(problem.message));
                                busy.set(false);
                                progress.set(None);
                                break;
                            }
                        }
                    }
                }
            }
        },
    );
    rsx! {
        Modal { title: "Open Git URL", description: destination_description, on_close: move |()| if !busy() { dialog.set(HomeDialog::None) },
            DialogForm {
                syntaxis_module_git::GitConnectionForm {}
                Field { control_id: "git-url", label: "Repository URL", error: error().filter(|message| message == INVALID_GIT_URL),
                    TextInput {
                        input_type: syntaxis_ui::prelude::TextInputType::Url,
                        value: url(),
                        placeholder: "https://github.com/owner/project.git",
                        autofocus: true,
                        disabled: busy(),
                        onpaste: move |_| paste_pending.set(true),
                        oninput: move |event: FormEvent| {
                            let value = event.value();
                            let was_pasted = paste_pending();
                            paste_pending.set(false);
                            if was_pasted && matches!(destination().trim(), "" | "/")
                                && let Some(name) = repository_name_from_url(&value)
                            {
                                destination.set(format!("/{name}"));
                            }
                            url.set(value);
                            error.set(None);
                        },
                    }
                }
                Field { control_id: "git-destination", label: "Destination folder", error: error().filter(|message| message == INVALID_CLONE_DESTINATION),
                    TextInput { value: destination(), placeholder: "/project", disabled: busy(), oninput: move |event: FormEvent| { destination.set(event.value()); error.set(None); } }
                }
                Field { control_id: "git-clone-mode", label: "Clone mode",
                    Select {
                        value: match clone_mode() {
                            CloneMode::Full => "full",
                            CloneMode::Blobless => "blobless",
                            CloneMode::Shallow => "shallow",
                        },
                        disabled: busy(),
                        onchange: move |event: FormEvent| {
                            clone_mode.set(match event.value().as_str() {
                                "blobless" => CloneMode::Blobless,
                                "shallow" => CloneMode::Shallow,
                                _ => CloneMode::Full,
                            });
                        },
                        option { value: "full", "Full" }
                        if supports_blobless { option { value: "blobless", "Blobless" } }
                        option { value: "shallow", "Shallow" }
                    }
                }
                if busy() {
                    p { class: "flex min-h-9 items-center gap-2 rounded-md border border-primary/30 bg-primary/10 px-2.5 py-2 text-[11px] text-primary", role: "status",
                        span { class: "size-3.5 shrink-0 animate-spin rounded-full border-2 border-primary/30 border-t-primary" }
                        {clone_progress_label(progress())}
                    }
                } else if let Some(message) = error().filter(|message| message != INVALID_GIT_URL && message != INVALID_CLONE_DESTINATION) {
                    p { class: "rounded-md bg-destructive/10 px-3 py-2 text-[11px] text-destructive", "{message}" }
                }
                DialogActions {
                    Button { label: if busy() { "Cancel clone" } else { "Cancel" }, kind: ButtonKind::Ghost, onclick: move |_| if busy() { clone_client.send(WorkspaceCloneCommand::Cancel); } else { dialog.set(HomeDialog::None); } }
                    Button { label: if busy() { "Cloning…" } else if error().is_some() { "Try again" } else { "Clone repository" }, kind: ButtonKind::Primary, disabled: busy() || url().trim().is_empty() || parse_clone_destination(&destination()).is_none(), onclick: move |_| {
                        let url_value = url().trim().to_owned();
                        if !looks_like_git_url(&url_value) {
                            error.set(Some(INVALID_GIT_URL.into()));
                            return;
                        }
                        let Some(destination_value) = parse_clone_destination(&destination()) else {
                            error.set(Some(INVALID_CLONE_DESTINATION.into()));
                            return;
                        };
                        error.set(None);
                        busy.set(true);
                        progress.set(Some(CloneProgress { phase: ClonePhase::Preparing, percent: None }));
                        clone_client.send(WorkspaceCloneCommand::Start(CloneRequest {
                            url: url_value,
                            destination_parent: destination_value.parent,
                            directory_name: Some(destination_value.directory_name),
                            mode: clone_mode(),
                        }));
                    } }
                }
            }
        }
    }
}

const INVALID_GIT_URL: &str = "Enter an HTTPS, SSH, or Git repository URL.";
const INVALID_CLONE_DESTINATION: &str =
    "Enter a destination folder with no more than one leading slash.";

enum WorkspaceCloneCommand {
    Start(CloneRequest),
    Cancel,
}

enum WorkspaceClonePoll {
    Cancel,
    IgnoreStart,
    CommandsClosed,
    Event(Result<Option<WorkspaceCloneEvent>, AppError>),
}

struct CloneDestination {
    parent: String,
    directory_name: String,
}

fn clone_progress_label(progress: Option<CloneProgress>) -> String {
    let Some(progress) = progress else {
        return "Starting clone…".into();
    };
    let phase = match progress.phase {
        ClonePhase::Preparing => "Preparing clone",
        ClonePhase::Counting => "Counting objects",
        ClonePhase::Compressing => "Compressing objects",
        ClonePhase::Receiving => "Receiving objects",
        ClonePhase::Resolving => "Resolving deltas",
        ClonePhase::CheckingOut => "Checking out files",
        ClonePhase::Finalizing => "Registering workspace",
    };
    progress.percent.map_or_else(
        || format!("{phase}…"),
        |percent| format!("{phase}… {percent}%"),
    )
}

fn looks_like_git_url(url: &str) -> bool {
    let url = url.trim();
    url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with("ssh://")
        || url.starts_with("git://")
        || (url.starts_with("git@") && url.contains(':'))
}

fn repository_name_from_url(url: &str) -> Option<String> {
    if !looks_like_git_url(url) {
        return None;
    }
    let url = url.trim().split(['?', '#']).next()?.trim_end_matches('/');
    let name = url
        .rsplit(['/', ':'])
        .next()?
        .strip_suffix(".git")
        .unwrap_or_else(|| url.rsplit(['/', ':']).next().unwrap_or_default());
    parse_clone_destination(name).map(|destination| destination.directory_name)
}

fn parse_clone_destination(value: &str) -> Option<CloneDestination> {
    let value = value.trim();
    if value.is_empty() || value.starts_with("//") || value.contains('\\') {
        return None;
    }
    let normalized = if value.starts_with('/') {
        value.to_owned()
    } else {
        format!("/{value}")
    };
    if normalized[1..]
        .split('/')
        .any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
        return None;
    }
    let (parent, directory_name) = normalized.rsplit_once('/')?;
    if directory_name.len() > 255 || directory_name.chars().any(char::is_control) {
        return None;
    }
    Some(CloneDestination {
        parent: if parent.is_empty() { "/" } else { parent }.to_owned(),
        directory_name: directory_name.to_owned(),
    })
}

#[component]
fn WorkspaceProjectDialog(
    mut dialog: Signal<HomeDialog>,
    on_changed: EventHandler<()>,
    on_notice: EventHandler<String>,
) -> Element {
    let services = use_context::<AppServices>();
    let Some(port) = services.workspace_projects().cloned() else {
        return rsx! {};
    };
    let management = services.workspace_management().cloned();
    let mut path = use_signal(String::new);
    let mut selected = use_signal(ProjectTemplate::default);
    let mut filter = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut created = use_signal(|| None::<WorkspaceRecord>);
    let mut setup_result = use_signal(|| None::<bool>);
    let navigator = use_navigator();
    let selected_definition = definition(selected());
    let path_error = validate_project_path(&path());
    rsx! {
        Modal {
            title: if created().is_some() { "Building new project" } else { "New project" },
            description: if let Some(workspace) = created() { format!("{} · {}", workspace.root, selected_definition.label) } else { "Create inside an exposed workspace root, then scaffold it in a live terminal.".into() },
            content_class: "max-w-225",
            on_close: move |()| if !busy() {
                if created().is_some() && setup_result().is_none() {
                    on_notice.call("Project setup continues in its terminal session.".into());
                }
                dialog.set(HomeDialog::None);
            },
            if let Some(workspace) = created() {
                div { class: "px-5 pt-3 pb-5",
                    if let Some(command) = selected_definition.command {
                        div { class: "h-[min(34rem,calc(100svh-13rem))] min-h-72 overflow-hidden rounded-lg border border-border bg-background",
                            syntaxis_module_terminal::ProjectInitializerTerminal {
                                workspace: workspace.clone(),
                                command: command.to_owned(),
                                label: format!("Initialize {}", selected_definition.label),
                                on_finished: {
                                    let management = management.clone();
                                    let initialized = workspace.clone();
                                    move |success| {
                                        setup_result.set(Some(success));
                                        if success {
                                            on_notice.call(format!(
                                                "{} project is ready.",
                                                selected_definition.label
                                            ));
                                        }
                                        let Some(management) = management.clone() else {
                                            on_changed.call(());
                                            return;
                                        };
                                        let initialized = initialized.clone();
                                        spawn(async move {
                                            if let Ok(refreshed) = management.refresh(&initialized).await {
                                                created.set(Some(refreshed));
                                            }
                                            on_changed.call(());
                                        });
                                    }
                                },
                            }
                        }
                        p { class: "mt-2.5 flex items-center gap-2 text-xs text-muted-foreground",
                            span { class: if setup_result() == Some(true) { "size-2 rounded-full bg-success" } else if setup_result() == Some(false) { "size-2 rounded-full bg-destructive" } else { "size-2 animate-pulse rounded-full bg-primary" } }
                            if setup_result() == Some(true) { "Setup finished successfully." }
                            else if setup_result() == Some(false) { "Setup exited with an error. The terminal remains available for repairs." }
                            else { "You can interact with the initializer or leave it running in this terminal session." }
                        }
                    } else {
                        div { class: "grid min-h-52 place-items-center rounded-lg border border-dashed border-border bg-muted/20 text-center",
                            div {
                                h3 { class: "font-semibold text-foreground", "Empty project created" }
                                p { class: "mt-1 text-sm text-muted-foreground", "The workspace is registered and ready for files." }
                            }
                        }
                    }
                    div { class: "mt-4 flex justify-end gap-2",
                        Button { label: "Back to home", kind: ButtonKind::Ghost, onclick: move |_| {
                            if setup_result().is_none() {
                                on_notice.call("Project setup continues in its terminal session.".into());
                            }
                            dialog.set(HomeDialog::None);
                        } }
                        Button { label: "Open project", kind: ButtonKind::Primary, onclick: {
                            let slug = workspace.slug.clone();
                            let navigator = navigator;
                            move |_| { navigator.push(Route::Files { slug: slug.clone(), query: FilesQuery::default() }); }
                        } }
                    }
                }
            } else {
                DialogForm {
                    Field { control_id: "new-project-path", label: "Project name or path", error: error().or_else(|| path_error.clone()),
                        TextInput { value: path(), placeholder: "MyAwesomeIdea", autofocus: true, disabled: busy(), oninput: move |event: FormEvent| { path.set(event.value()); error.set(None); } }
                    }
                    fieldset { disabled: busy(),
                        legend { class: "mb-2 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase", "Start from" }
                        TextInput { value: filter(), placeholder: "Filter frameworks and runtimes…", disabled: busy(), oninput: move |event: FormEvent| filter.set(event.value()) }
                        div { class: "mt-3 max-h-[min(25rem,44svh)] space-y-4 overflow-y-auto pr-1",
                            for category in CATEGORIES {
                                if category_has_matches(category, &filter()) {
                                    section {
                                        h3 { class: "mb-1.5 text-[10px] font-semibold tracking-wide text-muted-foreground uppercase", "{category.label()}" }
                                        div { class: "grid grid-cols-4 gap-2 max-md:grid-cols-2",
                                            for template in TEMPLATES {
                                                if template.category == category && template_matches(&template, &filter()) {
                                                    button {
                                                        r#type: "button",
                                                        class: if selected() == template.template { "flex min-w-0 items-center gap-2.5 rounded-lg border border-primary bg-primary/8 p-3 text-left" } else { "flex min-w-0 items-center gap-2.5 rounded-lg border border-border bg-card p-3 text-left hover:bg-accent" },
                                                        "aria-pressed": selected() == template.template,
                                                        onclick: move |_| selected.set(template.template),
                                                        span { class: "grid size-8 shrink-0 place-items-center", TemplateIcon { icon: template.icon } }
                                                        span { class: "min-w-0",
                                                            strong { class: "block truncate text-xs", "{template.label}" }
                                                            small { class: "block truncate text-[10px] text-muted-foreground", "{template.description}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    DialogActions {
                        Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: busy(), onclick: move |_| dialog.set(HomeDialog::None) }
                        Button { label: if busy() { "Creating…" } else { "Create project" }, kind: ButtonKind::Primary, disabled: busy() || path().trim().is_empty() || path_error.is_some(), onclick: move |_| {
                            busy.set(true);
                            let requested_path = path().trim().to_owned();
                            let port = port.clone();
                            spawn(async move {
                                match port.create_project(&requested_path).await {
                                    Ok(workspace) => {
                                        setup_result.set(selected_definition.command.is_none().then_some(true));
                                        created.set(Some(workspace));
                                        on_changed.call(());
                                    }
                                    Err(problem) => error.set(Some(problem.message)),
                                }
                                busy.set(false);
                            });
                        } }
                    }
                }
            }
        }
    }
}

fn parent_folder(path: &str) -> Option<String> {
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        return None;
    }
    let parent = path.rsplit_once('/').map_or("/", |(parent, _)| parent);
    Some(if parent.is_empty() { "/" } else { parent }.to_owned())
}

fn validate_project_path(path: &str) -> Option<String> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let relative = path.strip_prefix('/').unwrap_or(path);
    (path.starts_with("//")
        || path.contains('\\')
        || relative.split('/').any(|component| {
            component.is_empty()
                || matches!(component, "." | "..")
                || component.len() > 255
                || component.chars().any(char::is_control)
        }))
    .then(|| "Use a project name or subpath without empty, dot, or parent folders.".into())
}

#[component]
fn WorkspaceShell() -> Element {
    let services = use_context::<AppServices>();
    let route = use_route::<Route>();
    let (slug, section, title) = route_workspace(&route);
    let runtime_status = services.runtime_status().cloned();
    let runtime = use_resource(move || {
        let runtime_status = runtime_status.clone();
        async move {
            match runtime_status {
                Some(status) => status.state().await,
                None => Ok(RuntimeState::Connecting),
            }
        }
    });
    let runtime_presentation = runtime_presentation(runtime().as_ref());
    let auth_action = services.auth_action().cloned();
    let notifications_enabled = services.notifications().is_some();
    let catalog = services.workspace_catalog().cloned();
    let resolve_slug = slug.clone();
    let resolved = use_resource(move || {
        let catalog = catalog.clone();
        let slug = resolve_slug.clone();
        async move {
            match catalog {
                Some(catalog) => catalog.resolve(&slug).await,
                None => Err(syntaxis_app_contracts::AppError::unsupported(
                    "Workspace catalog unavailable.",
                    syntaxis_app_contracts::ErrorSource::Workspace,
                )),
            }
        }
    });
    let active = use_active_workspace();
    use_context_provider(|| active);
    let files_controller = syntaxis_module_files::use_files_controller();
    use_context_provider(|| files_controller);
    let files_ui = syntaxis_module_files::use_files_ui_state();
    use_context_provider(|| files_ui);
    let files_ports = services
        .files()
        .cloned()
        .expect("Files services are required");
    let writer = syntaxis_module_files::use_files_session_writer(files_ports.clone());
    use_context_provider(|| writer);
    use_context_provider(|| files_ports);
    use_context_provider(|| services.workspace_events().clone());
    let terminal = services
        .terminal()
        .cloned()
        .expect("Terminal services are required");
    let git = services.git().cloned().expect("Git services are required");
    let preview = services
        .preview()
        .cloned()
        .expect("Preview services are required");
    let ai = services.ai().cloned().expect("AI services are required");
    use_context_provider(|| terminal);
    use_context_provider(|| git.clone());
    use_context_provider(|| preview);
    use_context_provider(|| ai);
    let worktrees = git.worktrees().cloned();
    let worktree_workspace = active.base();
    let worktree_refresh = active.refresh();
    let _worktree_reconciliation = use_resource(move || {
        let worktrees = worktrees.clone();
        let workspace = worktree_workspace.clone();
        let _refresh = worktree_refresh;
        async move {
            let (Some(port), Some(workspace)) = (worktrees, workspace) else {
                return;
            };
            if let Ok(items) = port.list(&workspace).await {
                active.reconcile(items);
            }
        }
    });
    let event_source = services.workspace_event_source().cloned();
    let event_bus = services.workspace_events().clone();
    let _workspace_events = use_resource(move || {
        let source = event_source.clone();
        let bus = event_bus.clone();
        let workspace = active.current();
        async move {
            let (Some(source), Some(workspace)) = (source, workspace) else {
                return;
            };
            let mut retry_delay = std::time::Duration::from_secs(1);
            loop {
                if let Ok(stream) = source.connect(&workspace).await {
                    retry_delay = std::time::Duration::from_secs(1);
                    while let Ok(batch) = stream.receive().await {
                        if !batch.changes.is_empty() {
                            let _ = bus.publish_changes(
                                workspace.id.clone(),
                                None,
                                ChangeOrigin::External,
                                batch.changes,
                            );
                        }
                    }
                }
                dioxus_sdk_time::sleep(retry_delay).await;
                retry_delay = retry_delay
                    .saturating_mul(2)
                    .min(std::time::Duration::from_secs(30));
            }
        }
    });
    let applied = use_signal(|| None::<String>);
    let persist_catalog = services.workspace_catalog().cloned();
    use_effect(move || {
        let Some(Ok(workspace)) = resolved() else {
            return;
        };
        if applied() != Some(workspace.id.0.clone()) {
            let mut applied = applied;
            applied.set(Some(workspace.id.0.clone()));
            active.set_base(workspace.clone());
            files_ui.activate(workspace.id.clone());
            if let Some(catalog) = persist_catalog.clone() {
                spawn(async move {
                    let _ = catalog.touch(&workspace).await;
                    let _ = catalog.remember_section(&workspace, section).await;
                });
            }
        }
    });
    if let Some(Ok(workspace)) = resolved() {
        return rsx! {
            WorkspaceShellFrame {
                workspace,
                active: section,
                section_title: title,
                runtime_label: runtime_presentation.label,
                runtime_message: runtime_presentation.message,
                runtime_tone: runtime_presentation.tone,
                header_actions: rsx! {
                    if notifications_enabled { NotificationMenu {} }
                    if let Some(action) = auth_action { AuthActionButton { action } }
                },
                Outlet::<Route> {}
            }
        };
    }
    if let Some(Err(problem)) = resolved() {
        return rsx! {
            div { class: "grid size-full place-items-center bg-card p-6 text-center",
                div { class: "max-w-md",
                    h1 { class: "text-lg font-semibold text-foreground", "Workspace unavailable" }
                    p { class: "mt-2 text-sm text-destructive", "{problem.message}" }
                    Link { class: "mt-4 inline-flex rounded-md bg-primary px-3 py-2 text-sm font-semibold text-primary-foreground", to: Route::Home {}, "Back to projects" }
                }
            }
        };
    }
    rsx! { div { class: "grid size-full place-items-center bg-card text-sm text-muted-foreground", "Loading workspace…" } }
}

#[derive(Clone)]
struct RuntimePresentation {
    eyebrow: String,
    label: String,
    message: String,
    tone: Tone,
    folder_action_title: String,
    folder_action_description: String,
    folder_dialog_description: String,
}

fn runtime_presentation(state: Option<&Result<RuntimeState, AppError>>) -> RuntimePresentation {
    match state {
        Some(Ok(RuntimeState::Ready {
            identity,
            capabilities,
        })) => {
            let local = identity.location == ExecutionLocation::Local;
            let unrestricted = capabilities.supports(RuntimeCapability::UnrestrictedWorkspaceRoots);
            RuntimePresentation {
                eyebrow: if local {
                    "LOCAL WORKSPACES".into()
                } else {
                    "CONNECTED WORKSPACES".into()
                },
                label: identity.label.clone(),
                message: format!("{} is ready", identity.label),
                tone: Tone::Success,
                folder_action_title: if unrestricted {
                    "Open folder".into()
                } else {
                    "Open workspace folder".into()
                },
                folder_action_description: if local {
                    "Browse local folders".into()
                } else {
                    "Browse exposed folders".into()
                },
                folder_dialog_description: if local {
                    "Choose a project folder on this device.".into()
                } else {
                    format!("Choose a project folder exposed by {}.", identity.label)
                },
            }
        }
        Some(Ok(RuntimeState::Unavailable { message }) | Err(AppError { message, .. })) => {
            RuntimePresentation {
                eyebrow: "WORKSPACE DEVELOPMENT".into(),
                label: "Runtime unavailable".into(),
                message: message.clone(),
                tone: Tone::Destructive,
                folder_action_title: "Open workspace folder".into(),
                folder_action_description: "Browse exposed folders".into(),
                folder_dialog_description:
                    "Choose a project folder exposed by the connected runtime.".into(),
            }
        }
        Some(Ok(RuntimeState::Connecting)) | None => RuntimePresentation {
            eyebrow: "WORKSPACE DEVELOPMENT".into(),
            label: "Connecting".into(),
            message: "Connecting to the workspace runtime".into(),
            tone: Tone::Neutral,
            folder_action_title: "Open workspace folder".into(),
            folder_action_description: "Browse exposed folders".into(),
            folder_dialog_description: "Choose a project folder exposed by the connected runtime."
                .into(),
        },
    }
}

#[component]
fn AuthActionButton(action: AuthAction) -> Element {
    rsx! {
        form { action: action.endpoint, method: "post",
            button {
                r#type: "submit",
                class: "touch-target grid size-8 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-accent hover:text-foreground",
                title: action.label.clone(),
                "aria-label": action.label,
                Icon { icon: AppIcon::Logout, size: 15 }
            }
        }
    }
}

fn route_workspace(route: &Route) -> (String, WorkspaceSection, String) {
    match route {
        Route::Files { slug, .. } => (slug.clone(), WorkspaceSection::Files, "Files".into()),
        Route::Terminal { slug, .. } => {
            (slug.clone(), WorkspaceSection::Terminal, "Terminal".into())
        }
        Route::Git { slug } => (slug.clone(), WorkspaceSection::Git, "Git".into()),
        Route::Preview { slug } => (slug.clone(), WorkspaceSection::Preview, "Preview".into()),
        Route::Ai { slug, .. } => (slug.clone(), WorkspaceSection::Ai, "AI".into()),
        Route::AiSettings { slug, section } => (
            slug.clone(),
            WorkspaceSection::Ai,
            format!("AI Settings · {}", section.label()),
        ),
        Route::Home {} => ("syntaxis".into(), WorkspaceSection::Files, "Files".into()),
    }
}

fn active_workspace() -> Option<WorkspaceRecord> {
    use_context::<crate::ActiveWorkspace>().current()
}

#[component]
fn Files(slug: String, query: FilesQuery) -> Element {
    let navigator = use_navigator();
    let route_slug = slug.clone();
    rsx! { syntaxis_module_files::FilesView { workspace: active_workspace(), query, on_navigate: move |query| { navigator.replace(Route::Files { slug: route_slug.clone(), query }); } } }
}

#[component]
fn Terminal(slug: String, query: TerminalQuery) -> Element {
    let notifications = use_context::<crate::NotificationCenter>();
    let navigator = use_navigator();
    let route_slug = slug.clone();
    let on_navigate = EventHandler::new(move |intent| navigate(intent, &route_slug, &navigator));
    let workspace_id = active_workspace().map(|workspace| workspace.id.0);
    let viewed_workspace_id = workspace_id.clone();
    rsx! { syntaxis_module_terminal::TerminalView {
        workspace: active_workspace(),
        query,
        on_navigate,
        on_view_session: move |session_id: Option<String>| {
            if let Some(workspace_id) = viewed_workspace_id.clone() {
                notifications.view(
                    workspace_id,
                    session_id.map(|session_id| syntaxis_notifications::NotificationTarget::Terminal { session_id }),
                );
            }
        },
        on_stop_viewing: move |()| {
            if let Some(workspace_id) = workspace_id.as_deref() {
                notifications.stop_viewing(workspace_id);
            }
        },
    } }
}

#[component]
fn Git(slug: String) -> Element {
    let _ = slug;
    let active = use_context::<crate::ActiveWorkspace>();
    match active.current() {
        Some(workspace) => {
            rsx! { syntaxis_module_git::GitView { workspace, on_activate_worktree: move |worktree| active.activate(worktree) } }
        }
        None => rsx! { "Loading Git…" },
    }
}

#[component]
fn Preview(slug: String) -> Element {
    let _ = slug;
    match active_workspace() {
        Some(workspace) => rsx! { syntaxis_module_preview::PreviewView { workspace } },
        None => rsx! { "Loading Preview…" },
    }
}

#[component]
fn Ai(slug: String, query: AiQuery) -> Element {
    let notifications = use_context::<crate::NotificationCenter>();
    let active = use_context::<crate::ActiveWorkspace>();
    let files_ui = use_context::<syntaxis_module_files::FilesUiState>();
    let navigator = use_navigator();
    let route_slug = slug.clone();
    let on_navigate = EventHandler::new(move |intent| navigate(intent, &route_slug, &navigator));
    let workspace_id = active_workspace().map(|workspace| workspace.id.0);
    let viewed_workspace_id = workspace_id.clone();
    match active_workspace() {
        Some(workspace) => rsx! { syntaxis_module_ai::AiView {
            key: "{workspace.id.0}",
            start_new_conversation: active.should_create_agent_session(&workspace.id),
            workspace,
            base_workspace: active.base(),
            current_head: active.current_head(),
            requested_conversation_id: query.session_id,
            on_navigate,
            on_view_conversation: move |session_id: Option<String>| {
                if session_id.is_some()
                    && let Some(workspace) = active.current()
                {
                    active.complete_agent_session_request(&workspace.id);
                }
                if let Some(workspace_id) = viewed_workspace_id.clone() {
                    notifications.view(
                        workspace_id,
                        session_id.map(|session_id| syntaxis_notifications::NotificationTarget::Agent { session_id }),
                    );
                }
            },
            on_stop_viewing: move |()| {
                if let Some(workspace_id) = workspace_id.as_deref() {
                    notifications.stop_viewing(workspace_id);
                }
            },
            on_activate_worktree: move |worktree: WorktreeInfo| {
                let workspace_id = worktree.workspace.id.clone();
                active.request_new_agent_session(workspace_id);
                active.activate(worktree);
                files_ui.reset();
            },
        } },
        None => rsx! { "Loading AI…" },
    }
}

#[component]
fn AiSettings(slug: String, section: AiSettingsSection) -> Element {
    let navigator = use_navigator();
    let route_slug = slug.clone();
    let on_navigate = EventHandler::new(move |intent| navigate(intent, &route_slug, &navigator));
    match active_workspace() {
        Some(workspace) => {
            rsx! { syntaxis_module_ai::AiSettingsView { key: "{workspace.id.0}", workspace, section, on_navigate } }
        }
        None => rsx! { "Loading AI settings…" },
    }
}

fn navigate(intent: NavigationIntent, slug: &str, navigator: &Navigator) {
    match intent {
        NavigationIntent::Files { location, .. } => navigator.push(Route::Files {
            slug: slug.to_owned(),
            query: location.map_or_else(FilesQuery::default, |location| FilesQuery {
                path: Some(location.path.as_str().to_owned()),
                line: location.line,
                column: location.column,
                end_line: location.end_line,
                end_column: location.end_column,
            }),
        }),
        NavigationIntent::Terminal { session_id, .. } => navigator.replace(Route::Terminal {
            slug: slug.to_owned(),
            query: TerminalQuery { session_id },
        }),
        NavigationIntent::Git { .. } => navigator.push(Route::Git {
            slug: slug.to_owned(),
        }),
        NavigationIntent::Preview { .. } => navigator.push(Route::Preview {
            slug: slug.to_owned(),
        }),
        NavigationIntent::Ai {
            conversation_id, ..
        } => navigator.replace(Route::Ai {
            slug: slug.to_owned(),
            query: AiQuery {
                session_id: conversation_id,
            },
        }),
        NavigationIntent::AiSettings { section, .. } => navigator.push(Route::AiSettings {
            slug: slug.to_owned(),
            section,
        }),
        NavigationIntent::Home => navigator.push(Route::Home {}),
    };
}

#[cfg(test)]
mod tests {
    use syntaxis_git::{ClonePhase, CloneProgress};

    use super::{
        clone_progress_label, looks_like_git_url, parent_folder, parse_clone_destination,
        repository_name_from_url, validate_project_path,
    };

    #[test]
    fn folder_navigation_stops_at_the_exposed_root() {
        assert_eq!(parent_folder("/teams/project"), Some("/teams".into()));
        assert_eq!(parent_folder("/project"), Some("/".into()));
        assert_eq!(parent_folder("/"), None);
    }

    #[test]
    fn project_paths_allow_subdirectories_without_escape_components() {
        assert!(validate_project_path("testing/MyAwesomeIdea").is_none());
        assert!(validate_project_path("/testing/MyAwesomeIdea").is_none());
        assert!(validate_project_path("../outside").is_some());
        assert!(validate_project_path("testing//idea").is_some());
        assert!(validate_project_path("testing\\idea").is_some());
    }

    #[test]
    fn accepts_common_git_url_forms() {
        assert!(looks_like_git_url("https://example.com/owner/repo.git"));
        assert!(looks_like_git_url("git@example.com:owner/repo.git"));
        assert!(!looks_like_git_url("owner/repo"));
    }

    #[test]
    fn derives_repository_names_for_destination_suggestions() {
        assert_eq!(
            repository_name_from_url("https://example.com/owner/repo.git"),
            Some("repo".into())
        );
        assert_eq!(
            repository_name_from_url("git@example.com:owner/repo.git"),
            Some("repo".into())
        );
    }

    #[test]
    fn clone_destinations_split_parent_and_directory() {
        let destination = parse_clone_destination("/teams/repo").unwrap();
        assert_eq!(destination.parent, "/teams");
        assert_eq!(destination.directory_name, "repo");
        assert!(parse_clone_destination("//repo").is_none());
        assert!(parse_clone_destination("/").is_none());
    }

    #[test]
    fn formats_clone_progress() {
        assert_eq!(
            clone_progress_label(Some(CloneProgress {
                phase: ClonePhase::Receiving,
                percent: Some(42),
            })),
            "Receiving objects… 42%"
        );
    }
}
