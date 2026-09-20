#![allow(
    clippy::clone_on_ref_ptr,
    clippy::possible_missing_else,
    reason = "Home management keeps runtime-specific ports alive across async UI handlers"
)]

use dioxus::prelude::*;
use std::collections::{BTreeSet, HashSet};
use syntaxis_app_contracts::AppError;
use syntaxis_editor::{
    LanguageServerDefinition, language_server_by_id, language_servers_for_language,
    language_slug_for_path, profile_language_id,
};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, Checkbox, DialogActions, DialogForm, Icon, Modal, ProjectIcon,
    ProjectLanguageBadge, ProjectTechnologyBadge, SlideToConfirm, StatusBadge, TextArea,
    TextAreaResize, Tone,
};
use syntaxis_workspace::{
    EntryKind, FileEntry, RelativePath, WorkspaceAvailability, WorkspaceProfile, WorkspaceRecord,
    WorkspaceTechnology,
};

use crate::{AppServices, Route};

const UPDATE_TOOLS_COMMAND: &str = r"if ! command -v mise >/dev/null 2>&1; then
    echo 'mise is not installed in this runtime.' >&2
    exit 127
fi

mise trust --yes
mise upgrade --local";
const MISE_CHECK: &str = r"if ! command -v mise >/dev/null 2>&1; then
    echo 'mise is not installed in this runtime.' >&2
    exit 127
fi";
const CONFIGURED_BOOTSTRAP_COMMAND: &str = r"if ! command -v mise >/dev/null 2>&1; then
    echo 'mise is not installed in this runtime.' >&2
    exit 127
fi

echo 'Found a mise-compatible project configuration.'
mise trust --yes
mise install --yes
echo 'Project toolchain is ready.'";
const MIN_LANGUAGE_PERMILLE: u64 = 20;

#[derive(Clone, Debug, Eq, PartialEq)]
enum BootstrapPlan {
    Configured,
    Inferred(Vec<&'static str>),
}

#[derive(Clone, Debug, PartialEq)]
enum ManagementDialog {
    None,
    Bootstrap(WorkspaceRecord),
    UpdateTools(WorkspaceRecord),
    Notes(WorkspaceRecord),
    Cleanup(WorkspaceRecord),
    Delete(WorkspaceRecord),
    RuntimeCleanup,
}

#[component]
pub(crate) fn ManagedRecentProjects(
    workspaces: Vec<WorkspaceRecord>,
    #[props(default)] android: Option<crate::AndroidState>,
    managed_toolchains: bool,
    on_changed: EventHandler<()>,
    on_notice: EventHandler<(String, Tone)>,
) -> Element {
    let services = use_context::<AppServices>();
    let management = services
        .workspace_management()
        .cloned()
        .expect("managed projects require a workspace management port");
    let mut dialog = use_signal(|| ManagementDialog::None);
    let mut maintaining = use_signal(|| false);
    let android_port = services.android_shell().cloned();
    let local_label = android.is_some();
    let mut projects: Vec<_> = workspaces
        .into_iter()
        .map(|workspace| (false, workspace))
        .collect();
    if let Some(state) = &android {
        projects.extend(
            state
                .projects
                .iter()
                .cloned()
                .map(|workspace| (true, workspace)),
        );
        projects.sort_by_key(|entry| std::cmp::Reverse(entry.1.last_opened_unix_ms));
    }
    rsx! {
        section { "aria-labelledby": "recent-title",
            div { class: "mb-3 flex items-center justify-between gap-3",
                h2 { id: "recent-title", class: "text-[17px] font-semibold text-muted-foreground", "Recent projects" }
                if managed_toolchains || android.is_some() {
                    details { class: "relative",
                        summary { class: "touch-target grid size-8 cursor-pointer list-none place-items-center rounded-lg text-muted-foreground hover:bg-accent", title: "Manage runtime storage", "aria-label": "Manage runtime storage",
                            Icon { icon: AppIcon::MoreVertical, size: 15 }
                        }
                        div { class: "absolute right-0 z-40 mt-1 w-52 rounded-lg border border-border bg-popover p-1 shadow-xl",
                            if let Some(state) = &android {
                                RuntimeAction { label: if state.remote_configured { "Remote settings" } else { "Add Remote" }, disabled: false, onclick: {
                                    let port = android_port.clone();
                                    move |_| if let Some(port) = port.clone() {
                                        spawn(async move { if let Err(error) = port.configure_remote().await { on_notice.call((error.message, Tone::Destructive)); } });
                                    }
                                } }
                            }
                            if managed_toolchains {
                            RuntimeAction { label: if maintaining() { "Updating tools…" } else { "Update installed tools" }, disabled: maintaining(), onclick: {
                                let management = management.clone();
                                move |_| {
                                    maintaining.set(true);
                                    let management = management.clone();
                                    spawn(async move {
                                        match management.update_installed_tools().await {
                                            Ok(()) => on_notice.call(("Installed mise tools are up to date.".into(), Tone::Success)),
                                            Err(error) => on_notice.call((error.message, Tone::Destructive)),
                                        }
                                        maintaining.set(false);
                                    });
                                }
                            } }
                            RuntimeAction { label: if maintaining() { "Pruning tools…" } else { "Prune unused tools" }, disabled: maintaining(), onclick: {
                                let management = management.clone();
                                move |_| {
                                    maintaining.set(true);
                                    let management = management.clone();
                                    spawn(async move {
                                        match management.prune_installed_tools().await {
                                            Ok(()) => on_notice.call(("Unused mise tools were pruned.".into(), Tone::Success)),
                                            Err(error) => on_notice.call((error.message, Tone::Destructive)),
                                        }
                                        maintaining.set(false);
                                    });
                                }
                            } }
                            RuntimeAction { label: "Free up space…", destructive: true, disabled: maintaining(), onclick: move |_| dialog.set(ManagementDialog::RuntimeCleanup) }
                            }
                        }
                    }
                }
            }
            if let Some(error) = android.as_ref().and_then(|state| state.error.as_ref()) {
                p { class: "mb-3 text-sm text-muted-foreground", "{error}" }
            }
            if projects.is_empty() {
                div { class: "flex min-h-70 flex-col items-center justify-center rounded-xl border border-border bg-card/90 px-5.5 py-9 text-center",
                    div { class: "mb-3 grid size-11.5 place-items-center rounded-xl bg-primary/10 text-[22px] text-primary", "◇" }
                    h3 { class: "text-[15px] font-semibold text-foreground", "No recent projects" }
                    p { class: "mt-1.5 max-w-96 text-xs leading-relaxed text-muted-foreground", "Open a workspace folder or clone a Git repository to get started." }
                }
            } else {
                div { class: "rounded-xl border border-border bg-card shadow-sm",
                    for (remote, workspace) in projects {
                        if remote {
                            crate::android::AndroidRemoteProject { key: "remote:{workspace.id.0}", workspace }
                        } else {
                        ManagedWorkspaceRow {
                            key: "local:{workspace.id.0}",
                            workspace,
                            local_label,
                            managed_toolchains,
                            on_dialog: move |next| dialog.set(next),
                            on_changed,
                            on_notice,
                        }
                        }
                    }
                }
            }
        }
        match dialog() {
            ManagementDialog::None => rsx! {},
            ManagementDialog::Bootstrap(workspace) => rsx! { BootstrapDialog { workspace, dialog, on_changed, on_notice } },
            ManagementDialog::UpdateTools(workspace) => rsx! { ToolTerminalDialog { workspace, command: UPDATE_TOOLS_COMMAND, title: "Update tools", description: "Upgrade project-local mise tools within their configured version ranges.", dialog, on_notice } },
            ManagementDialog::Notes(workspace) => rsx! { NotesDialog { workspace, dialog, on_notice } },
            ManagementDialog::Cleanup(workspace) => rsx! { CleanupDialog { workspace, dialog, on_changed, on_notice } },
            ManagementDialog::Delete(workspace) => rsx! { DeleteDialog { workspace, dialog, on_changed, on_notice } },
            ManagementDialog::RuntimeCleanup => rsx! { RuntimeCleanupDialog { dialog, on_notice } },
        }
    }
}

#[component]
pub(crate) fn ManagedRecentProjectsLoading() -> Element {
    rsx! {
        section { "aria-labelledby": "recent-title",
            h2 { id: "recent-title", class: "mb-3 text-[17px] font-semibold text-muted-foreground", "Recent projects" }
            div { class: "overflow-hidden rounded-xl border border-border bg-card shadow-sm", aria_busy: "true", aria_label: "Loading recent projects",
                for index in 0..4 {
                    div { class: "flex h-22 items-center gap-3 border-b border-border px-3 py-3 last:border-b-0",
                        span { class: "size-10 shrink-0 animate-pulse rounded-lg bg-secondary", aria_hidden: true }
                        span { class: "min-w-0 flex-1", aria_hidden: true,
                            span { class: if index % 2 == 0 { "mb-2 block h-3 w-1/2 animate-pulse rounded-md bg-secondary" } else { "mb-2 block h-3 w-2/3 animate-pulse rounded-md bg-secondary" } }
                            span { class: "block h-2 w-3/4 animate-pulse rounded-md bg-secondary" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn ManagedRecentProjectsError(on_retry: EventHandler<()>) -> Element {
    rsx! {
        section { "aria-labelledby": "recent-title",
            h2 { id: "recent-title", class: "mb-3 text-[17px] font-semibold text-muted-foreground", "Recent projects" }
            div { class: "flex min-h-70 flex-col items-center justify-center rounded-xl border border-border bg-card/90 px-5.5 py-9 text-center max-md:min-h-62.5", role: "alert",
                div { class: "mb-3 grid size-11.5 place-items-center rounded-xl bg-destructive/10 text-[22px] text-destructive", aria_hidden: true, "!" }
                h3 { class: "text-[15px] font-semibold text-foreground", "Recent projects are unavailable" }
                p { class: "mt-1.5 max-w-96 text-xs leading-relaxed text-muted-foreground", "The workspace registry could not be read. Your project files were not affected." }
                div { class: "mt-4", Button { label: "Try again", kind: ButtonKind::Secondary, onclick: move |_| on_retry.call(()) } }
            }
        }
    }
}

#[component]
fn RuntimeAction(
    label: String,
    #[props(default)] destructive: bool,
    disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! { button {
        r#type: "button",
        class: if destructive { "block min-h-8 w-full rounded px-2 text-left text-xs text-destructive hover:bg-accent disabled:opacity-40" } else { "block min-h-8 w-full rounded px-2 text-left text-xs hover:bg-accent disabled:opacity-40" },
        disabled,
        onclick: move |event| onclick.call(event),
        "{label}"
    } }
}

#[component]
fn ManagedWorkspaceRow(
    workspace: WorkspaceRecord,
    local_label: bool,
    managed_toolchains: bool,
    on_dialog: EventHandler<ManagementDialog>,
    on_changed: EventHandler<()>,
    on_notice: EventHandler<(String, Tone)>,
) -> Element {
    let services = use_context::<AppServices>();
    let management = services
        .workspace_management()
        .cloned()
        .expect("managed projects require a workspace management port");
    let available = workspace.availability == WorkspaceAvailability::Available;
    let missing = workspace.availability == WorkspaceAvailability::Missing;
    let mut refreshing = use_signal(|| false);
    rsx! {
        article { class: "flex min-h-22 min-w-0 items-center border-b border-border first:rounded-t-xl last:rounded-b-xl last:border-b-0 hover:bg-accent/60 max-md:min-h-16",
            Link {
                class: if available { "grid min-w-0 flex-1 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 px-3 py-3 max-md:grid-cols-[auto_minmax(0,1fr)] max-md:py-2.5" } else { "grid min-w-0 flex-1 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 px-3 py-3 opacity-65 max-md:grid-cols-[auto_minmax(0,1fr)] max-md:py-2.5" },
                to: Route::for_workspace_section(workspace.slug.clone(), workspace.last_section),
                onclick: move |event: MouseEvent| if !available { event.prevent_default(); },
                ProjectIcon { name: workspace.name.clone(), icon: workspace.icon.clone() }
                div { class: "min-w-0",
                    div { class: "flex min-w-0 items-center gap-2",
                        strong { class: "min-w-0 truncate text-sm font-semibold max-md:max-w-[42%] max-md:shrink-0", "{workspace.name}" }
                        if local_label { span { class: "text-[11px] text-muted-foreground", "Local" } }
                        if missing { StatusBadge { label: "Missing", tone: Tone::Destructive } }
                        else if workspace.availability == WorkspaceAvailability::Checking { StatusBadge { label: "Checking", tone: Tone::Neutral } }
                        small { class: "hidden min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground max-md:block", "{workspace.root}" }
                    }
                    small { class: "block truncate font-mono text-[11px] text-muted-foreground max-md:hidden", "{workspace.root}" }
                    ProjectMetadata { workspace: workspace.clone(), desktop: false }
                }
                ProjectMetadata { workspace: workspace.clone(), desktop: true }
            }
            details { class: "relative mr-1 shrink-0",
                summary { class: "touch-target grid size-8 cursor-pointer list-none place-items-center rounded-lg text-muted-foreground hover:bg-accent", title: format!("Project actions for {}", workspace.name), "aria-label": format!("Project actions for {}", workspace.name),
                    Icon { icon: AppIcon::MoreVertical, size: 15 }
                }
                div { class: "absolute right-0 z-40 mt-1 w-42 rounded-lg border border-border bg-popover p-1 shadow-xl",
                    if !missing {
                        RuntimeAction { label: "Bootstrap", disabled: !managed_toolchains || refreshing() || !available, onclick: { let workspace = workspace.clone(); move |_| on_dialog.call(ManagementDialog::Bootstrap(workspace.clone())) } }
                        RuntimeAction { label: "Update tools", disabled: !managed_toolchains || refreshing() || !available, onclick: { let workspace = workspace.clone(); move |_| on_dialog.call(ManagementDialog::UpdateTools(workspace.clone())) } }
                        RuntimeAction { label: "Notes", disabled: refreshing(), onclick: { let workspace = workspace.clone(); move |_| on_dialog.call(ManagementDialog::Notes(workspace.clone())) } }
                    }
                    RuntimeAction { label: if refreshing() { "Refreshing…" } else if missing { "Check again" } else { "Refresh" }, disabled: refreshing() || (!available && !missing), onclick: {
                        let workspace = workspace.clone();
                        let management = management.clone();
                        move |_| {
                            refreshing.set(true);
                            let workspace = workspace.clone();
                            let management = management.clone();
                            spawn(async move {
                                match management.refresh(&workspace).await {
                                    Ok(refreshed) => {
                                        let message = if refreshed.availability == WorkspaceAvailability::Missing { format!("{} is still missing.", workspace.name) } else { format!("Refreshed {}.", workspace.name) };
                                        on_notice.call((message, Tone::Success));
                                        on_changed.call(());
                                    }
                                    Err(error) => on_notice.call((error.message, Tone::Destructive)),
                                }
                                refreshing.set(false);
                            });
                        }
                    } }
                    if !missing { RuntimeAction { label: "Cleanup files", destructive: true, disabled: refreshing() || !available, onclick: { let workspace = workspace.clone(); move |_| on_dialog.call(ManagementDialog::Cleanup(workspace.clone())) } } }
                    RuntimeAction { label: if missing { "Remove" } else { "Delete" }, destructive: true, disabled: refreshing(), onclick: { let workspace = workspace.clone(); move |_| on_dialog.call(ManagementDialog::Delete(workspace.clone())) } }
                }
            }
        }
    }
}

#[component]
fn ProjectMetadata(workspace: WorkspaceRecord, desktop: bool) -> Element {
    let total_bytes = workspace.profile.total_language_bytes();
    let technologies = workspace
        .profile
        .technologies
        .iter()
        .copied()
        .take(5)
        .collect::<Vec<_>>();
    let languages = workspace
        .profile
        .languages
        .iter()
        .filter(|language| {
            total_bytes > 0
                && language.bytes.saturating_mul(1_000)
                    >= total_bytes.saturating_mul(MIN_LANGUAGE_PERMILLE)
        })
        .filter(|language| !language_represented_by_technology(&language.name, &technologies))
        .take(5)
        .cloned()
        .collect::<Vec<_>>();
    let technology_count = technologies.len();
    let has_badges = technology_count > 0 || !languages.is_empty();
    let last_opened = recent_label(workspace.last_opened_unix_ms);
    rsx! {
        if desktop {
            div { class: "flex shrink-0 flex-col items-end gap-1.5 text-right max-md:hidden",
                if has_badges { ProjectBadgeList { technologies, languages, total_bytes, large: true } }
                time { class: "self-stretch whitespace-nowrap text-right text-[10px] text-muted-foreground/75", title: "Last opened {last_opened}", "Opened {last_opened}" }
            }
        } else {
            div { class: "mt-2 flex min-w-0 items-center gap-2 md:hidden",
                if has_badges {
                    ProjectBadgeList { technologies, languages, total_bytes, large: false }
                    span { class: "h-3 w-px shrink-0 bg-border", aria_hidden: true }
                }
                time { class: "shrink-0 whitespace-nowrap text-left text-[10px] text-muted-foreground", title: "Last opened {last_opened}", "Opened {last_opened}" }
            }
        }
    }
}

#[component]
fn ProjectBadgeList(
    technologies: Vec<WorkspaceTechnology>,
    languages: Vec<syntaxis_workspace::WorkspaceLanguage>,
    total_bytes: u64,
    large: bool,
) -> Element {
    let technology_count = technologies.len();
    rsx! {
        span { class: if large { "flex min-w-0 items-center gap-1.5 overflow-hidden" } else { "flex min-w-0 items-center gap-1 overflow-hidden" }, aria_label: "Detected project technologies and languages",
            for (index, technology) in technologies.into_iter().enumerate() {
                ProjectTechnologyBadge { key: "technology-{technology:?}", technology, large, class: badge_visibility_class(index) }
            }
            for (index, language) in languages.into_iter().enumerate() {
                ProjectLanguageBadge { key: "language-{language.name}", language, total_bytes, large, class: badge_visibility_class(technology_count + index) }
            }
        }
    }
}

const fn badge_visibility_class(index: usize) -> &'static str {
    match index {
        0..=1 => "",
        2..=3 => "max-[479px]:hidden",
        4..=6 => "max-md:hidden",
        _ => "max-lg:hidden",
    }
}

fn language_represented_by_technology(
    language: &str,
    technologies: &[WorkspaceTechnology],
) -> bool {
    technologies.iter().any(|technology| {
        matches!(
            (technology, language),
            (WorkspaceTechnology::Astro, "Astro")
                | (WorkspaceTechnology::Docker, "Dockerfile")
                | (WorkspaceTechnology::Graphql, "GraphQL")
                | (WorkspaceTechnology::Just, "Just")
                | (WorkspaceTechnology::Nginx, "Nginx")
                | (WorkspaceTechnology::Prisma, "Prisma")
                | (WorkspaceTechnology::Svelte, "Svelte")
                | (WorkspaceTechnology::Terraform, "HCL" | "Terraform Template")
                | (WorkspaceTechnology::Vue, "Vue")
        )
    })
}

fn recent_label(timestamp: i64) -> String {
    let now = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(timestamp);
    let minutes = now.saturating_sub(timestamp) / 60_000;
    match minutes {
        0 => "Just now".into(),
        1..=59 => format!("{minutes}m ago"),
        60..=1_439 => format!("{}h ago", minutes / 60),
        _ => format!("{}d ago", minutes / 1_440),
    }
}

#[component]
fn NotesDialog(
    workspace: WorkspaceRecord,
    mut dialog: Signal<ManagementDialog>,
    on_notice: EventHandler<(String, Tone)>,
) -> Element {
    let services = use_context::<AppServices>();
    let management = services
        .workspace_management()
        .cloned()
        .expect("workspace notes require a workspace management port");
    let loader = management.clone();
    let target = workspace.clone();
    let loaded = use_resource(move || {
        let loader = loader.clone();
        let target = target.clone();
        async move { loader.load_notes(&target).await }
    });
    let mut notes = use_signal(String::new);
    let mut initialized = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    use_effect(move || {
        if !initialized()
            && let Some(result) = loaded()
        {
            initialized.set(true);
            match result {
                Ok(value) => notes.set(value),
                Err(problem) => error.set(Some(problem.message)),
            }
        }
    });
    rsx! { Modal {
        title: format!("Notes for {}", workspace.name),
        description: "Private notes stored with this workspace's application data.",
        content_class: "max-w-170",
        on_close: move |()| if !busy() { dialog.set(ManagementDialog::None) },
        DialogForm {
            if !initialized() { p { class: "py-10 text-center text-sm text-muted-foreground", "Loading notes…" } }
            else { TextArea { class: "min-h-72 font-mono text-sm", rows: 16, resize: TextAreaResize::Vertical, value: notes(), disabled: busy(), aria_label: "Workspace notes", placeholder: "Write anything you want to remember about this project…", oninput: move |event: FormEvent| { notes.set(event.value()); error.set(None); } } }
            if let Some(message) = error() { p { class: "rounded-md border border-destructive/35 bg-destructive/10 px-2.5 py-2 text-xs text-destructive", role: "alert", "{message}" } }
            DialogActions {
                Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: busy(), onclick: move |_| dialog.set(ManagementDialog::None) }
                Button { label: if busy() { "Saving…" } else { "Save notes" }, kind: ButtonKind::Primary, disabled: busy() || !initialized(), onclick: {
                    let management = management.clone(); let workspace = workspace.clone();
                    move |_| { busy.set(true); error.set(None); let management = management.clone(); let workspace = workspace.clone(); let value = notes(); spawn(async move {
                        match management.save_notes(&workspace, &value).await { Ok(()) => { on_notice.call(("Workspace notes saved.".into(), Tone::Success)); dialog.set(ManagementDialog::None); }, Err(problem) => { error.set(Some(problem.message)); busy.set(false); } }
                    }); }
                } }
            }
        }
    } }
}

#[component]
fn CleanupDialog(
    workspace: WorkspaceRecord,
    mut dialog: Signal<ManagementDialog>,
    on_changed: EventHandler<()>,
    on_notice: EventHandler<(String, Tone)>,
) -> Element {
    let services = use_context::<AppServices>();
    let management = services
        .workspace_management()
        .cloned()
        .expect("workspace cleanup requires a workspace management port");
    let loader = management.clone();
    let target = workspace.clone();
    let entries = use_resource(move || {
        let loader = loader.clone();
        let target = target.clone();
        async move { loader.cleanup_entries(&target).await }
    });
    let mut selected = use_signal(HashSet::<String>::new);
    let mut initialized = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    use_effect(move || {
        if !initialized()
            && let Some(result) = entries()
        {
            initialized.set(true);
            match result {
                Ok(items) => selected.set(items.into_iter().map(|item| item.path).collect()),
                Err(problem) => error.set(Some(problem.message)),
            }
        }
    });
    let items = entries().and_then(Result::ok).unwrap_or_default();
    rsx! { Modal {
        title: format!("Cleanup files in {}?", workspace.name),
        description: "Select ignored build artifacts and caches to remove. Local configuration is excluded.",
        content_class: "max-w-150",
        on_close: move |()| if !busy() { dialog.set(ManagementDialog::None) },
        DialogForm {
            if !initialized() { p { class: "py-10 text-center text-sm text-muted-foreground", "Finding cleanup candidates…" } }
            else if items.is_empty() && error().is_none() { p { class: "rounded-lg border border-border p-5 text-center text-sm text-muted-foreground", "There are no ignored files to clean up." } }
            else { div { class: "max-h-80 space-y-1 overflow-y-auto rounded-lg border border-border p-2",
                for item in items { { let path = item.path.clone(); let selected_path = path.clone(); rsx! { label { class: "flex items-center gap-2 rounded px-2 py-2 hover:bg-accent",
                    Checkbox { checked: selected.read().contains(&path), disabled: busy(), aria_label: format!("Clean up {path}"), on_checked_change: move |checked| { if checked { selected.write().insert(selected_path.clone()); } else { selected.write().remove(&selected_path); } } }
                    span { class: "min-w-0 flex-1 truncate font-mono text-xs", "{path}" }
                    if item.directory { small { class: "text-[10px] text-muted-foreground", "directory" } }
                } } } }
            } }
            if let Some(message) = error() { p { class: "text-xs text-destructive", role: "alert", "{message}" } }
            DialogActions {
                Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: busy(), onclick: move |_| dialog.set(ManagementDialog::None) }
                Button { label: if busy() { "Cleaning…" } else { "Cleanup selected" }, kind: ButtonKind::Danger, disabled: busy() || !initialized() || selected.read().is_empty(), onclick: {
                    let management = management.clone(); let workspace = workspace.clone();
                    move |_| { busy.set(true); let management = management.clone(); let workspace = workspace.clone(); let chosen = selected.read().iter().cloned().collect(); spawn(async move {
                        match management.cleanup(&workspace, chosen).await { Ok(count) => { on_notice.call((format!("Removed {count} cleanup entries."), Tone::Success)); on_changed.call(()); dialog.set(ManagementDialog::None); }, Err(problem) => { error.set(Some(problem.message)); busy.set(false); } }
                    }); }
                } }
            }
        }
    } }
}

#[component]
fn DeleteDialog(
    workspace: WorkspaceRecord,
    mut dialog: Signal<ManagementDialog>,
    on_changed: EventHandler<()>,
    on_notice: EventHandler<(String, Tone)>,
) -> Element {
    let services = use_context::<AppServices>();
    let management = services
        .workspace_management()
        .cloned()
        .expect("workspace removal requires a workspace management port");
    let mut delete_files = use_signal(|| false);
    let mut confirmed = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut delete_files_failed = use_signal(|| false);
    rsx! { Modal {
        title: format!("Remove {}?", workspace.name),
        description: "The workspace will be removed from your recent projects.",
        on_close: move |()| if !busy() { dialog.set(ManagementDialog::None) },
        DialogForm {
            label { class: "flex items-start gap-2 rounded-lg border border-border p-3",
                Checkbox { checked: delete_files(), disabled: busy(), aria_label: "Also delete project files", on_checked_change: move |checked| { delete_files.set(checked); confirmed.set(false); delete_files_failed.set(false); error.set(None); } }
                span { strong { class: "block", "Also delete project files" } small { class: "mt-1 block text-[11px] text-muted-foreground", "This cannot be undone." } }
            }
            if delete_files() {
                div { class: "space-y-1.5",
                    SlideToConfirm { disabled: busy(), tone: Tone::Destructive, label: "Slide to confirm delete", confirmed_label: "Deletion confirmed", on_confirmed: move |value| confirmed.set(value) }
                    small { class: "block truncate px-1 text-[10px] text-muted-foreground", "Permanently deletes {workspace.root}" }
                }
            }
            if busy() {
                p { class: "flex min-h-9 items-center gap-2 rounded-md border border-primary/30 bg-primary/10 px-2.5 py-2 text-[11px] text-primary", role: "status",
                    span { class: "size-3.5 shrink-0 animate-spin rounded-full border-2 border-primary/30 border-t-primary" }
                    "Removing workspace safely…"
                }
            } else if let Some(message) = error() {
                p { class: "rounded-md border border-destructive/35 bg-destructive/10 px-2.5 py-2 text-xs leading-relaxed text-destructive", role: "alert", "{message}" }
            }
            DialogActions {
                Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: busy(), onclick: move |_| dialog.set(ManagementDialog::None) }
                Button { label: if busy() { "Removing…" } else if delete_files_failed() { "Remove entry only" } else if delete_files() { "Delete files and remove" } else { "Remove workspace" }, kind: ButtonKind::Danger, disabled: busy() || (delete_files() && !confirmed() && !delete_files_failed()), onclick: {
                    let management = management.clone(); let workspace = workspace.clone();
                    move |_| { busy.set(true); error.set(None); let management = management.clone(); let workspace = workspace.clone(); let remove_files = delete_files() && !delete_files_failed(); spawn(async move {
                        match management.remove(&workspace, remove_files).await {
                            Ok(()) => { on_notice.call(("Workspace removed.".into(), Tone::Success)); on_changed.call(()); dialog.set(ManagementDialog::None); }
                            Err(_problem) if remove_files => {
                                delete_files_failed.set(true);
                                error.set(Some("The project folder could not be deleted. You can still remove the workspace entry.".into()));
                                busy.set(false);
                            }
                            Err(problem) => { error.set(Some(problem.message)); busy.set(false); }
                        }
                    }); }
                } }
            }
        }
    } }
}

#[component]
fn RuntimeCleanupDialog(
    mut dialog: Signal<ManagementDialog>,
    on_notice: EventHandler<(String, Tone)>,
) -> Element {
    let services = use_context::<AppServices>();
    let management = services
        .workspace_management()
        .cloned()
        .expect("runtime cleanup requires a workspace management port");
    let mut caches = use_signal(|| true);
    let mut mise = use_signal(|| false);
    let mut tools = use_signal(|| false);
    let mut confirmed = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let destructive = mise() || tools();
    rsx! { Modal {
        title: "Free up space",
        description: "Choose what should be removed. Everything listed here can be downloaded or installed again later.",
        on_close: move |()| if !busy() { dialog.set(ManagementDialog::None) },
        DialogForm {
            CleanupChoice { checked: caches(), disabled: busy(), title: "Downloaded packages and build caches", description: "Clears temporary files created by Bun, npm, Cargo, Gradle, and other development tools. Your projects and installed tools stay available.", on_change: move |value| caches.set(value) }
            CleanupChoice { checked: mise(), disabled: busy(), title: "Tools installed with Mise", description: "Removes Node.js, Rust, and other tool versions managed by Mise. They can be reinstalled from your project setup files.", on_change: move |value| { mise.set(value); confirmed.set(false); } }
            CleanupChoice { checked: tools(), disabled: busy(), title: "Other installed developer tools", description: "Removes Bun global packages, Deno commands, and Rustup toolchains. You'll need to reinstall them before using them again.", on_change: move |value| { tools.set(value); confirmed.set(false); } }
            if destructive {
                div { class: "space-y-1.5",
                    SlideToConfirm { disabled: busy(), tone: Tone::Destructive, label: "Slide to confirm removing tools", confirmed_label: "Tool removal confirmed", on_confirmed: move |value| confirmed.set(value) }
                    small { class: "block px-1 text-[10px] text-muted-foreground", "Applies to every project in this workspace. Your project files, settings, credentials, lockfiles, and AI sessions will not be deleted." }
                }
            }
            if busy() {
                p { class: "flex min-h-9 items-center gap-2 rounded-md border border-primary/30 bg-primary/10 px-2.5 py-2 text-[11px] text-primary", role: "status",
                    span { class: "size-3.5 shrink-0 animate-spin rounded-full border-2 border-primary/30 border-t-primary" }
                    "Freeing up space…"
                }
            } else if let Some(message) = error() {
                p { class: "rounded-md border border-destructive/35 bg-destructive/10 px-2.5 py-2 text-xs leading-relaxed text-destructive", role: "alert", "{message}" }
            }
            DialogActions {
                Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: busy(), onclick: move |_| dialog.set(ManagementDialog::None) }
                Button { label: if busy() { "Freeing up space…" } else { "Free up space" }, kind: ButtonKind::Danger, disabled: busy() || (!caches() && !destructive) || (destructive && !confirmed()), onclick: {
                    let management = management.clone();
                    move |_| { let management = management.clone(); let clear_caches = caches(); let clear_mise = mise(); let clear_tools = tools(); busy.set(true); error.set(None); spawn(async move {
                        let result = async { if clear_caches { management.clear_runtime_caches().await?; } if clear_mise { management.clear_mise_tools().await?; } if clear_tools { management.clear_runtime_tools().await?; } Ok::<(), AppError>(()) }.await;
                        match result { Ok(()) => { on_notice.call(("Runtime cleanup complete.".into(), Tone::Success)); dialog.set(ManagementDialog::None); }, Err(problem) => { error.set(Some(problem.message)); busy.set(false); } }
                    }); }
                } }
            }
        }
    } }
}

#[component]
fn CleanupChoice(
    checked: bool,
    disabled: bool,
    title: String,
    description: String,
    on_change: EventHandler<bool>,
) -> Element {
    rsx! { label { class: "flex items-start gap-2.5 rounded-lg border border-border p-3",
        Checkbox { class: "mt-0.5", checked, disabled, aria_label: title.clone(), on_checked_change: move |value| on_change.call(value) }
        span {
            strong { class: "block text-sm", "{title}" }
            small { class: "mt-1 block text-[11px] text-muted-foreground", "{description}" }
        }
    } }
}

#[component]
fn BootstrapDialog(
    workspace: WorkspaceRecord,
    mut dialog: Signal<ManagementDialog>,
    on_changed: EventHandler<()>,
    on_notice: EventHandler<(String, Tone)>,
) -> Element {
    let services = use_context::<AppServices>();
    let files = services
        .files()
        .cloned()
        .expect("workspace bootstrap requires Files ports");
    let management = services.workspace_management().cloned();
    let target = workspace.clone();
    let bootstrap_files = files.clone();
    let mut plan = use_resource(move || {
        let target = target.clone();
        let bootstrap_files = bootstrap_files.clone();
        async move { detect_bootstrap_plan(&bootstrap_files, target).await }
    });
    let mut selected_command = use_signal(|| None::<String>);
    let mut inferred_command = use_signal(String::new);
    let mut initialized_command = use_signal(|| false);
    let mut result = use_signal(|| None::<bool>);

    use_effect(move || {
        if !initialized_command()
            && let Some(Ok(BootstrapPlan::Inferred(tools))) = plan()
        {
            let mise_command = inferred_mise_command(&tools);
            if tools.is_empty() {
                inferred_command.set(mise_command);
            } else {
                selected_command.set(Some(inferred_bootstrap_command(&mise_command)));
            }
            initialized_command.set(true);
        }
    });

    let detected_plan = plan();
    let command = match detected_plan.as_ref() {
        Some(Ok(BootstrapPlan::Configured)) => Some(CONFIGURED_BOOTSTRAP_COMMAND.to_owned()),
        Some(Ok(BootstrapPlan::Inferred(_))) => selected_command(),
        Some(Err(_)) | None => None,
    };
    let running = command.is_some();
    let needs_manual_command = matches!(
        detected_plan.as_ref(),
        Some(Ok(BootstrapPlan::Inferred(tools))) if tools.is_empty()
    );
    rsx! {
        Modal {
            title: format!("Bootstrap {}", workspace.name),
            description: format!("Install the toolchain required by {} with mise.", workspace.root),
            content_class: if running { "max-w-180" } else { "max-w-2xl" },
            on_close: move |()| {
                if running && result().is_none() {
                    on_notice.call(("Project bootstrap continues in its terminal session.".into(), Tone::Neutral));
                }
                dialog.set(ManagementDialog::None);
            },
            if let Some(command) = command {
                div { class: "px-5 pt-3 pb-5",
                    div { class: "h-[min(34rem,calc(100svh-13rem))] min-h-72 overflow-hidden rounded-lg border border-border bg-background",
                        syntaxis_module_terminal::ProjectInitializerTerminal {
                            workspace: workspace.clone(),
                            command,
                            label: "Bootstrap with mise",
                            on_finished: {
                                let management = management.clone();
                                let initialized = workspace.clone();
                                let workspace_name = workspace.name.clone();
                                move |success| {
                                    result.set(Some(success));
                                    let management = management.clone();
                                    let initialized = initialized.clone();
                                    let workspace_name = workspace_name.clone();
                                    spawn(async move {
                                        if let Some(management) = management {
                                            let _ = management.refresh(&initialized).await;
                                        }
                                        on_changed.call(());
                                        on_notice.call((
                                            if success {
                                                format!("{workspace_name} is ready.")
                                            } else {
                                                format!("Bootstrap failed for {workspace_name}.")
                                            },
                                            if success { Tone::Success } else { Tone::Destructive },
                                        ));
                                    });
                                }
                            },
                        }
                    }
                    p { class: "mt-2.5 flex items-center gap-2 text-xs text-muted-foreground",
                        span { class: if result() == Some(true) { "size-2 rounded-full bg-success" } else if result() == Some(false) { "size-2 rounded-full bg-destructive" } else { "size-2 animate-pulse rounded-full bg-primary" } }
                        if result() == Some(true) {
                            "Bootstrap finished successfully."
                        } else if result() == Some(false) {
                            "Bootstrap exited with an error. Review the terminal output above."
                        } else {
                            "You can leave this dialog while installation continues."
                        }
                    }
                    div { class: "mt-4 flex justify-end",
                        Button { label: "Close", kind: ButtonKind::Primary, onclick: move |_| dialog.set(ManagementDialog::None) }
                    }
                }
            } else if detected_plan.is_none() {
                div { class: "flex min-h-36 items-center justify-center gap-2 px-5 text-sm text-muted-foreground",
                    span { class: "size-4 animate-spin rounded-full border-2 border-border border-t-primary" }
                    "Inspecting the project…"
                }
            } else if let Some(Err(problem)) = detected_plan.as_ref() {
                div { class: "space-y-4 px-5 pt-3 pb-5",
                    p { class: "rounded-md border border-destructive/35 bg-destructive/10 px-3 py-2 text-sm text-destructive", role: "alert",
                        "Could not inspect this project: {problem.message}"
                    }
                    DialogActions {
                        Button { label: "Cancel", kind: ButtonKind::Ghost, onclick: move |_| dialog.set(ManagementDialog::None) }
                        Button { label: "Try again", kind: ButtonKind::Primary, onclick: move |_| plan.restart() }
                    }
                }
            } else if needs_manual_command {
                div { class: "space-y-4 px-5 pt-3 pb-5",
                    p { class: "text-sm leading-relaxed text-muted-foreground",
                        "No supported project markers were found. Enter the mise command you want to run."
                    }
                    syntaxis_ui::prelude::Field {
                        control_id: "bootstrap-mise-command",
                        label: "Command to execute",
                        TextArea {
                            class: "min-h-24 font-mono text-xs",
                            rows: 4,
                            resize: TextAreaResize::None,
                            value: inferred_command(),
                            autofocus: true,
                            placeholder: "mise use --yes --env local node@lts",
                            oninput: move |event: FormEvent| inferred_command.set(event.value()),
                        }
                    }
                    p { class: "text-xs leading-relaxed text-muted-foreground",
                        "This creates a checkout-local mise.local.toml. For Git repositories, that file and its lockfile are also excluded from commits."
                    }
                    DialogActions {
                        Button { label: "Cancel", kind: ButtonKind::Ghost, onclick: move |_| dialog.set(ManagementDialog::None) }
                        Button {
                            label: "Run command",
                            kind: ButtonKind::Primary,
                            disabled: inferred_command().trim().is_empty(),
                            onclick: move |_| selected_command.set(Some(inferred_bootstrap_command(&inferred_command()))),
                        }
                    }
                }
            } else {
                div { class: "flex min-h-36 items-center justify-center gap-2 px-5 text-sm text-muted-foreground",
                    span { class: "size-4 animate-spin rounded-full border-2 border-border border-t-primary" }
                    "Starting bootstrap…"
                }
            }
        }
    }
}

#[component]
fn ToolTerminalDialog(
    workspace: WorkspaceRecord,
    command: String,
    title: String,
    description: String,
    mut dialog: Signal<ManagementDialog>,
    on_notice: EventHandler<(String, Tone)>,
) -> Element {
    let mut result = use_signal(|| None::<bool>);
    let workspace_name = workspace.name.clone();
    rsx! { Modal {
        title: format!("{title} for {workspace_name}"),
        description,
        content_class: "max-w-180",
        on_close: move |()| {
            if result().is_none() {
                on_notice.call(("Tool updates continue in their terminal session.".into(), Tone::Neutral));
            }
            dialog.set(ManagementDialog::None);
        },
        div { class: "px-5 pt-3 pb-5",
            div { class: "h-[min(34rem,calc(100svh-13rem))] min-h-72 overflow-hidden rounded-lg border border-border bg-background",
                syntaxis_module_terminal::ProjectInitializerTerminal { workspace, command, label: "Update tools with mise", on_finished: move |success| { result.set(Some(success)); on_notice.call((if success { format!("Updated tools for {workspace_name}.") } else { format!("Tool setup failed for {workspace_name}.") }, if success { Tone::Success } else { Tone::Destructive })); } }
            }
            p { class: "mt-2.5 flex items-center gap-2 text-xs text-muted-foreground",
                span { class: if result() == Some(true) { "size-2 rounded-full bg-success" } else if result() == Some(false) { "size-2 rounded-full bg-destructive" } else { "size-2 animate-pulse rounded-full bg-primary" } }
                if result() == Some(true) { "Project tools are up to date." }
                else if result() == Some(false) { "mise exited with an error. Review the terminal output above." }
                else { "You can leave this dialog while updates continue." }
            }
            div { class: "mt-4 flex justify-end", Button { label: "Close", kind: ButtonKind::Primary, onclick: move |_| dialog.set(ManagementDialog::None) } }
        }
    } }
}

async fn detect_bootstrap_plan(
    files: &syntaxis_module_files::FilesPorts,
    workspace: WorkspaceRecord,
) -> Result<BootstrapPlan, AppError> {
    let root = files
        .files()
        .list(&workspace, &RelativePath::root())
        .await
        .map_err(AppError::from)?;
    if has_root_mise_config(&root) || has_nested_mise_config(files, &workspace, &root).await {
        return Ok(BootstrapPlan::Configured);
    }
    Ok(BootstrapPlan::Inferred(infer_tools(
        &root,
        &workspace.profile,
    )))
}

fn has_root_mise_config(entries: &[FileEntry]) -> bool {
    const CONFIGS: [&str; 5] = [
        "mise.toml",
        ".mise.toml",
        "mise.local.toml",
        ".mise.local.toml",
        ".tool-versions",
    ];
    entries
        .iter()
        .any(|entry| entry.kind == EntryKind::File && CONFIGS.contains(&entry.name.as_str()))
}

async fn has_nested_mise_config(
    files: &syntaxis_module_files::FilesPorts,
    workspace: &WorkspaceRecord,
    root: &[FileEntry],
) -> bool {
    let root_names = entry_names(root);
    for directory in ["mise", ".mise"] {
        if root_names.contains(directory)
            && list_contains_file(files, workspace, directory, "config.toml").await
        {
            return true;
        }
    }
    if !root_names.contains(".config") {
        return false;
    }
    let config = list_directory(files, workspace, ".config").await;
    if config
        .iter()
        .any(|entry| entry.kind == EntryKind::File && entry.name == "mise.toml")
    {
        return true;
    }
    if !entry_names(&config).contains("mise") {
        return false;
    }
    let mise = list_directory(files, workspace, ".config/mise").await;
    if mise
        .iter()
        .any(|entry| entry.kind == EntryKind::File && entry.name == "config.toml")
    {
        return true;
    }
    if !entry_names(&mise).contains("conf.d") {
        return false;
    }
    list_directory(files, workspace, ".config/mise/conf.d")
        .await
        .iter()
        .any(|entry| {
            entry.kind == EntryKind::File
                && std::path::Path::new(&entry.name)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
        })
}

async fn list_contains_file(
    files: &syntaxis_module_files::FilesPorts,
    workspace: &WorkspaceRecord,
    path: &str,
    name: &str,
) -> bool {
    list_directory(files, workspace, path)
        .await
        .iter()
        .any(|entry| entry.kind == EntryKind::File && entry.name == name)
}

async fn list_directory(
    files: &syntaxis_module_files::FilesPorts,
    workspace: &WorkspaceRecord,
    path: &str,
) -> Vec<FileEntry> {
    let Ok(path) = RelativePath::try_from(path) else {
        return Vec::new();
    };
    files
        .files()
        .list(workspace, &path)
        .await
        .unwrap_or_default()
}

fn entry_names(entries: &[FileEntry]) -> HashSet<&str> {
    entries.iter().map(|entry| entry.name.as_str()).collect()
}

fn infer_tools(entries: &[FileEntry], profile: &WorkspaceProfile) -> Vec<&'static str> {
    let files = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File)
        .map(|entry| entry.name.as_str())
        .collect::<BTreeSet<_>>();
    let has = |names: &[&str]| names.iter().any(|name| files.contains(name));
    let has_extension = |extension: &str| files.iter().any(|name| name.ends_with(extension));
    let mut tools = Vec::new();

    if has(&["Cargo.toml", "rust-toolchain", "rust-toolchain.toml"]) {
        tools.push("rust@stable");
    }
    if has(&["deno.json", "deno.jsonc", "deno.lock"]) {
        tools.push("deno@latest");
    } else if has(&["bun.lock", "bun.lockb"]) {
        tools.push("bun@latest");
    } else if has(&["package.json"]) {
        tools.push("node@lts");
        if has(&["pnpm-lock.yaml"]) {
            tools.push("pnpm@latest");
        } else if has(&["yarn.lock"]) {
            tools.push("yarn@latest");
        }
    }
    if has(&[
        "pyproject.toml",
        "requirements.txt",
        "setup.py",
        "setup.cfg",
        "Pipfile",
    ]) {
        tools.push("python@latest");
        if has(&["uv.lock"]) {
            tools.push("uv@latest");
        }
    }
    if has(&["go.mod", "go.work"]) {
        tools.push("go@latest");
    }
    if has(&["global.json"]) || has_extension(".csproj") || has_extension(".sln") {
        tools.push("dotnet@latest");
    }
    if has(&["pom.xml", "build.gradle", "build.gradle.kts", "gradlew"]) {
        tools.push("java@latest");
    }
    if has(&["Gemfile", ".ruby-version"]) {
        tools.push("ruby@latest");
    }
    if has(&["composer.json"]) {
        tools.extend(["php@latest", "composer@latest"]);
    }
    if has_extension(".tf") || has(&[".terraform.lock.hcl"]) {
        tools.push("terraform@latest");
    }
    if has(&["Justfile", "justfile"]) {
        tools.push("just@latest");
    }

    let mut language_ids = profile
        .languages
        .iter()
        .filter_map(|language| profile_language_id(&language.name))
        .collect::<Vec<_>>();
    language_ids.extend(
        files
            .iter()
            .map(|name| language_slug_for_path(name))
            .filter(|language| *language != "plaintext"),
    );
    language_ids.sort_unstable();
    language_ids.dedup();
    for language_id in language_ids {
        for server in language_servers_for_language(language_id, &profile.technologies) {
            push_server_tools(&mut tools, server);
        }
    }
    for server_id in profile
        .technologies
        .iter()
        .filter_map(|technology| match technology {
            WorkspaceTechnology::Astro => Some("astro"),
            WorkspaceTechnology::Svelte => Some("svelte"),
            WorkspaceTechnology::Tailwind => Some("tailwindcss"),
            WorkspaceTechnology::Vue => Some("vue"),
            _ => None,
        })
    {
        if let Some(server) = language_server_by_id(server_id) {
            push_server_tools(&mut tools, server);
        }
    }
    tools
}

fn push_server_tools(tools: &mut Vec<&'static str>, server: &'static LanguageServerDefinition) {
    for tool in server.mise_tools {
        if !tools.contains(tool) {
            tools.push(tool);
        }
    }
}

fn inferred_mise_command(tools: &[&str]) -> String {
    if tools.is_empty() {
        String::new()
    } else {
        format!("mise use --yes --env local {}", tools.join(" "))
    }
}

fn inferred_bootstrap_command(command: &str) -> String {
    format!(
        r#"{MISE_CHECK}

if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    exclude_path=$(git rev-parse --git-path info/exclude)
    for ignored in mise.local.toml mise.local.lock; do
        if ! grep -qxF "$ignored" "$exclude_path"; then
            printf '%s\n' "$ignored" >> "$exclude_path"
        fi
    done
fi
echo 'Writing mise.local.toml so the inferred setup stays local to this checkout.'
{command}
echo 'Project toolchain is ready.'"#,
        command = command.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::{
        CONFIGURED_BOOTSTRAP_COMMAND, UPDATE_TOOLS_COMMAND, infer_tools,
        inferred_bootstrap_command, inferred_mise_command,
    };
    use syntaxis_workspace::{
        EntryKind, FileEntry, RelativePath, WorkspaceLanguage, WorkspaceProfile,
        WorkspaceTechnology,
    };

    fn files(names: &[&str]) -> Vec<FileEntry> {
        names
            .iter()
            .map(|name| FileEntry {
                path: RelativePath::try_from(*name).unwrap(),
                name: (*name).to_owned(),
                kind: EntryKind::File,
                size: 0,
                version: None,
            })
            .collect()
    }

    #[test]
    fn project_update_is_scoped_to_local_configuration() {
        assert!(UPDATE_TOOLS_COMMAND.contains("mise upgrade --local"));
        assert!(UPDATE_TOOLS_COMMAND.contains("mise trust --yes"));
    }

    #[test]
    fn configured_bootstrap_trusts_and_installs_project_config() {
        assert!(CONFIGURED_BOOTSTRAP_COMMAND.contains("mise trust --yes"));
        assert!(CONFIGURED_BOOTSTRAP_COMMAND.contains("mise install --yes"));
    }

    #[test]
    fn manifests_and_profile_infer_runtime_and_language_server_tools() {
        let profile = WorkspaceProfile {
            technologies: vec![WorkspaceTechnology::Vue],
            languages: vec![
                WorkspaceLanguage {
                    name: "Rust".into(),
                    bytes: 100,
                },
                WorkspaceLanguage {
                    name: "TypeScript".into(),
                    bytes: 80,
                },
            ],
        };
        let tools = infer_tools(
            &files(&["Cargo.toml", "package.json", "pnpm-lock.yaml"]),
            &profile,
        );
        for expected in [
            "rust@stable",
            "node@lts",
            "pnpm@latest",
            "rust-analyzer@latest",
            "npm:typescript@latest",
            "npm:@vue/language-server@latest",
        ] {
            assert!(
                tools.contains(&expected),
                "missing inferred tool {expected}"
            );
        }
        assert!(inferred_mise_command(&tools).starts_with("mise use --yes --env local "));
    }

    #[test]
    fn inferred_command_stays_checkout_local() {
        let command = inferred_bootstrap_command("mise use --env local node@22");
        assert!(command.contains("mise use --env local node@22"));
        assert!(command.contains("mise.local.toml mise.local.lock"));
    }
}
