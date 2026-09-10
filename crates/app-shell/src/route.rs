#![allow(
    clippy::clone_on_ref_ptr,
    clippy::redundant_locals,
    clippy::no_effect_underscore_binding,
    reason = "Route handlers capture runtime-specific ports and reactive refresh signals"
)]

use dioxus::prelude::*;
use dioxus::router::Navigator;
use syntaxis_app_contracts::{AppError, ChangeOrigin, NavigationIntent};
use syntaxis_git::WorktreeInfo;
use syntaxis_module_files::{FilesPorts, FilesQuery};
use syntaxis_module_terminal::TerminalQuery;
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, DialogActions, DialogForm, Field, Icon, Modal, ProjectIcon,
    SkipLink, StatusBadge, TemplateIcon, TextInput, Toast, Tone, WorkspaceSourceAction,
    WorkspaceSourceFileAction,
};
use syntaxis_workspace::{
    ExecutionLocation, RuntimeState, WorkspaceAvailability, WorkspaceRecord, WorkspaceSection,
};

use crate::notifications::{NotificationMenu, use_notification_center};
use crate::{
    AiQuery, AiSettingsSection, AppServices, AuthAction, WorkspaceShellFrame,
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
        main { class: "app-viewport relative w-full overflow-y-auto bg-background",
            SkipLink { target_id: "home-main-content" }
            section { id: "home-main-content", tabindex: "-1", class: "mx-auto flex min-h-full w-[calc(100%-2.5rem)] max-w-205 flex-col pt-[max(9vh,env(safe-area-inset-top))] pb-6 max-md:w-[calc(100%-1.5rem)]",
                header { class: "mb-9 flex items-start justify-between gap-4",
                    div {
                        p { class: "text-[10px] font-bold tracking-[.14em] text-primary", "{runtime_presentation.eyebrow}" }
                        h1 { class: "mt-1 text-4xl font-semibold tracking-tight text-foreground max-md:text-3xl", "Welcome back!" }
                        p { class: "mt-1 text-[15px] text-muted-foreground", "Pick up where you left off or open another project." }
                    }
                    div { class: "flex items-center gap-1",
                        if services.notifications().is_some() { NotificationMenu {} }
                        if let Some(action) = auth_action { AuthActionButton { action } }
                    }
                }
                if services.workspace_folders().is_some() {
                    div { class: "mb-10 grid grid-cols-3 gap-3 max-md:grid-cols-1",
                        WorkspaceSourceAction {
                            icon: AppIcon::Folder,
                            title: "Open folder",
                            description: "Browse folders exposed by the runtime",
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
                            description: "Create an empty project workspace",
                            disabled: services.workspace_projects().is_none(),
                            onclick: move |_| dialog.set(HomeDialog::Project),
                        }
                    }
                } else if local_folders.is_some() {
                    div { class: "mb-10 grid grid-cols-3 gap-3 max-md:grid-cols-1",
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
                        None => rsx! { p { class: "text-sm text-muted-foreground", "aria-label": "Loading recent projects", "Loading workspaces…" } },
                        Some(Err(error)) => rsx! { p { class: "text-sm text-destructive", "{error.message}" } },
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
            HomeDialog::Folder => rsx! { WorkspaceFolderDialog { dialog } },
            HomeDialog::Git => rsx! { WorkspaceCloneDialog { dialog } },
            HomeDialog::Project => rsx! { WorkspaceProjectDialog { dialog } },
            HomeDialog::None => rsx! {},
        }
        if let Some((message, tone)) = notice() { Toast { message, tone, on_close: move |()| notice.set(None) } }
    }
}

#[component]
fn WorkspaceFolderDialog(mut dialog: Signal<HomeDialog>) -> Element {
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
            title: "Open folder",
            description: "Choose a folder exposed by the connected runtime.",
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
                div { class: "max-h-56 overflow-y-auto rounded-lg border border-border p-1",
                    if path().trim().is_empty() {
                        if let Some(Ok(items)) = roots() {
                            for root in items {
                                {
                                    let target = root.path.clone();
                                    rsx! { button { r#type: "button", class: "block w-full rounded-md px-2 py-2 text-left text-xs hover:bg-accent", onclick: move |_| path.set(target.clone()),
                                        strong { class: "block", "{root.name}" }
                                        small { class: "font-mono text-muted-foreground", "{root.path}" }
                                    } }
                                }
                            }
                        }
                    } else if let Some(Ok(items)) = directories() {
                        for item in items {
                            {
                                let target = item.path.clone();
                                rsx! { button { r#type: "button", class: "block w-full truncate rounded-md px-2 py-2 text-left font-mono text-xs hover:bg-accent", onclick: move |_| path.set(target.clone()), "{item.name}" } }
                            }
                        }
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
    let mut url = use_signal(String::new);
    let mut destination = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let navigator = use_navigator();
    rsx! {
        Modal { title: "Open Git URL", description: "Clone a repository into an exposed runtime folder.", on_close: move |()| if !busy() { dialog.set(HomeDialog::None) },
            DialogForm {
                Field { control_id: "git-url", label: "Repository URL",
                    TextInput { value: url(), placeholder: "https://github.com/owner/project.git", autofocus: true, disabled: busy(), oninput: move |event: FormEvent| { url.set(event.value()); error.set(None); } }
                }
                Field { control_id: "git-destination", label: "Destination parent", error: error(),
                    TextInput { value: destination(), placeholder: "/Projects", disabled: busy(), oninput: move |event: FormEvent| { destination.set(event.value()); error.set(None); } }
                }
                DialogActions {
                    Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: busy(), onclick: move |_| dialog.set(HomeDialog::None) }
                    Button { label: if busy() { "Cloning…" } else { "Clone repository" }, kind: ButtonKind::Primary, disabled: busy() || url().trim().is_empty() || destination().trim().is_empty(), onclick: move |_| {
                        busy.set(true);
                        let url_value = url().trim().to_owned();
                        let destination_value = destination().trim().to_owned();
                        let port = port.clone();
                        let navigator = navigator;
                        spawn(async move {
                            match port.clone_repository(&url_value, &destination_value).await {
                                Ok(workspace) => { navigator.push(Route::Files { slug: workspace.slug, query: FilesQuery::default() }); }
                                Err(problem) => { error.set(Some(problem.message)); busy.set(false); }
                            }
                        });
                    } }
                }
            }
        }
    }
}

#[component]
fn WorkspaceProjectDialog(mut dialog: Signal<HomeDialog>) -> Element {
    let services = use_context::<AppServices>();
    let Some(port) = services.workspace_projects().cloned() else {
        return rsx! {};
    };
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
            on_close: move |()| if !busy() { dialog.set(HomeDialog::None) },
            if let Some(workspace) = created() {
                div { class: "px-5 pt-3 pb-5",
                    if let Some(command) = selected_definition.command {
                        div { class: "h-[min(34rem,calc(100svh-13rem))] min-h-72 overflow-hidden rounded-lg border border-border bg-background",
                            syntaxis_module_terminal::ProjectInitializerTerminal {
                                workspace: workspace.clone(),
                                command: command.to_owned(),
                                label: format!("Initialize {}", selected_definition.label),
                                on_finished: move |success| setup_result.set(Some(success)),
                            }
                        }
                        p { class: "mt-2.5 text-xs text-muted-foreground",
                            if setup_result() == Some(true) { "Setup finished successfully." }
                            else if setup_result() == Some(false) { "Setup exited with an error. The terminal remains available for repairs." }
                            else { "You can interact with the initializer or leave it running." }
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
                        Button { label: "Back to home", kind: ButtonKind::Ghost, onclick: move |_| dialog.set(HomeDialog::None) }
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
}

fn runtime_presentation(state: Option<&Result<RuntimeState, AppError>>) -> RuntimePresentation {
    match state {
        Some(Ok(RuntimeState::Ready { identity, .. })) => RuntimePresentation {
            eyebrow: if identity.location == ExecutionLocation::Local {
                "LOCAL WORKSPACES".into()
            } else {
                "CONNECTED WORKSPACES".into()
            },
            label: identity.label.clone(),
            message: format!("{} is ready", identity.label),
            tone: Tone::Success,
        },
        Some(Ok(RuntimeState::Unavailable { message }) | Err(AppError { message, .. })) => {
            RuntimePresentation {
                eyebrow: "WORKSPACE DEVELOPMENT".into(),
                label: "Runtime unavailable".into(),
                message: message.clone(),
                tone: Tone::Destructive,
            }
        }
        Some(Ok(RuntimeState::Connecting)) | None => RuntimePresentation {
            eyebrow: "WORKSPACE DEVELOPMENT".into(),
            label: "Connecting".into(),
            message: "Connecting to the workspace runtime".into(),
            tone: Tone::Neutral,
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
            rsx! { syntaxis_module_ai::AiSettingsView { workspace, section, on_navigate } }
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
        } => navigator.push(Route::Ai {
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
    use super::validate_project_path;

    #[test]
    fn project_paths_allow_subdirectories_without_escape_components() {
        assert!(validate_project_path("testing/MyAwesomeIdea").is_none());
        assert!(validate_project_path("/testing/MyAwesomeIdea").is_none());
        assert!(validate_project_path("../outside").is_some());
        assert!(validate_project_path("testing//idea").is_some());
        assert!(validate_project_path("testing\\idea").is_some());
    }
}
