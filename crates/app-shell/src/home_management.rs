#![allow(
    clippy::clone_on_ref_ptr,
    clippy::possible_missing_else,
    reason = "Home management keeps runtime-specific ports alive across async UI handlers"
)]

use dioxus::prelude::*;
use std::collections::HashSet;
use syntaxis_app_contracts::{AppError, ErrorSource};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, Checkbox, DialogActions, DialogForm, Icon, Modal, ProjectIcon,
    SlideToConfirm, StatusBadge, TextArea, TextAreaResize, Tone,
};
use syntaxis_workspace::{EntryKind, RelativePath, WorkspaceAvailability, WorkspaceRecord};

use crate::{AppServices, Route};

const UPDATE_TOOLS_COMMAND: &str = r"if ! command -v mise >/dev/null 2>&1; then
    echo 'mise is not installed in this runtime.' >&2
    exit 127
fi

mise trust --yes
mise upgrade --local";

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
    rsx! {
        section { "aria-labelledby": "recent-title",
            div { class: "mb-3 flex items-center justify-between gap-3",
                h2 { id: "recent-title", class: "text-[17px] font-semibold text-muted-foreground", "Recent projects" }
                details { class: "relative",
                    summary { class: "touch-target grid size-8 cursor-pointer list-none place-items-center rounded-lg text-muted-foreground hover:bg-accent", title: "Manage runtime storage", "aria-label": "Manage runtime storage",
                        Icon { icon: AppIcon::MoreVertical, size: 15 }
                    }
                    div { class: "absolute right-0 z-40 mt-1 w-52 rounded-lg border border-border bg-popover p-1 shadow-xl",
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
            if workspaces.is_empty() {
                p { class: "rounded-xl border border-border bg-card p-4 text-sm text-muted-foreground", "No recent workspaces." }
            } else {
                div { class: "rounded-xl border border-border bg-card shadow-sm",
                    for workspace in workspaces {
                        ManagedWorkspaceRow {
                            key: "{workspace.id.0}",
                            workspace,
                            on_dialog: move |next| dialog.set(next),
                            on_changed,
                            on_notice,
                        }
                    }
                }
            }
        }
        match dialog() {
            ManagementDialog::None => rsx! {},
            ManagementDialog::Bootstrap(workspace) => rsx! { BootstrapDialog { workspace, dialog, on_changed, on_notice } },
            ManagementDialog::UpdateTools(workspace) => rsx! { ToolTerminalDialog { workspace, command: UPDATE_TOOLS_COMMAND, title: "Update project tools", description: "Upgrade project-local mise tools within their configured version ranges.", dialog, on_notice } },
            ManagementDialog::Notes(workspace) => rsx! { NotesDialog { workspace, dialog, on_notice } },
            ManagementDialog::Cleanup(workspace) => rsx! { CleanupDialog { workspace, dialog, on_changed, on_notice } },
            ManagementDialog::Delete(workspace) => rsx! { DeleteDialog { workspace, dialog, on_changed, on_notice } },
            ManagementDialog::RuntimeCleanup => rsx! { RuntimeCleanupDialog { dialog, on_notice } },
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
        article { class: "flex min-h-20 min-w-0 items-center border-b border-border last:border-b-0 hover:bg-accent/60",
            Link {
                class: if available { "flex min-w-0 flex-1 items-center gap-3 px-3 py-3" } else { "flex min-w-0 flex-1 items-center gap-3 px-3 py-3 opacity-65" },
                to: Route::for_workspace_section(workspace.slug.clone(), workspace.last_section),
                onclick: move |event: MouseEvent| if !available { event.prevent_default(); },
                ProjectIcon { name: workspace.name.clone(), icon: workspace.icon.clone() }
                div { class: "min-w-0 flex-1",
                    div { class: "flex items-center gap-2",
                        strong { class: "truncate text-sm", "{workspace.name}" }
                        if missing { StatusBadge { label: "Missing", tone: Tone::Destructive } }
                        else if workspace.availability == WorkspaceAvailability::Checking { StatusBadge { label: "Checking", tone: Tone::Neutral } }
                    }
                    small { class: "block truncate font-mono text-[11px] text-muted-foreground", "{workspace.root}" }
                }
            }
            details { class: "relative mr-1 shrink-0",
                summary { class: "touch-target grid size-8 cursor-pointer list-none place-items-center rounded-lg text-muted-foreground hover:bg-accent", title: format!("Project actions for {}", workspace.name), "aria-label": format!("Project actions for {}", workspace.name),
                    Icon { icon: AppIcon::MoreVertical, size: 15 }
                }
                div { class: "absolute right-0 z-40 mt-1 w-42 rounded-lg border border-border bg-popover p-1 shadow-xl",
                    if !missing {
                        RuntimeAction { label: "Bootstrap", disabled: refreshing() || !available, onclick: { let workspace = workspace.clone(); move |_| on_dialog.call(ManagementDialog::Bootstrap(workspace.clone())) } }
                        RuntimeAction { label: "Update tools", disabled: refreshing() || !available, onclick: { let workspace = workspace.clone(); move |_| on_dialog.call(ManagementDialog::UpdateTools(workspace.clone())) } }
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
            else { TextArea { class: "min-h-72 font-mono text-sm", rows: 16, resize: TextAreaResize::Vertical, value: notes(), disabled: busy(), aria_label: "Workspace notes", oninput: move |event: FormEvent| { notes.set(event.value()); error.set(None); } } }
            if let Some(message) = error() { p { class: "text-xs text-destructive", role: "alert", "{message}" } }
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
    rsx! { Modal {
        title: format!("Remove {}?", workspace.name),
        description: "The workspace will be removed from your recent projects.",
        on_close: move |()| if !busy() { dialog.set(ManagementDialog::None) },
        DialogForm {
            label { class: "flex items-start gap-2 rounded-lg border border-border p-3",
                Checkbox { checked: delete_files(), disabled: busy(), aria_label: "Also delete project files", on_checked_change: move |checked| { delete_files.set(checked); confirmed.set(false); } }
                span { strong { class: "block", "Also delete project files" } small { class: "text-[11px] text-muted-foreground", "This cannot be undone." } }
            }
            if delete_files() { SlideToConfirm { disabled: busy(), tone: Tone::Destructive, label: "Slide to confirm delete", confirmed_label: "Deletion confirmed", on_confirmed: move |value| confirmed.set(value) } }
            if let Some(message) = error() { p { class: "text-xs text-destructive", role: "alert", "{message}" } }
            DialogActions {
                Button { label: "Cancel", kind: ButtonKind::Ghost, disabled: busy(), onclick: move |_| dialog.set(ManagementDialog::None) }
                Button { label: if busy() { "Removing…" } else if delete_files() { "Delete files and remove" } else { "Remove workspace" }, kind: ButtonKind::Danger, disabled: busy() || (delete_files() && !confirmed()), onclick: {
                    let management = management.clone(); let workspace = workspace.clone();
                    move |_| { busy.set(true); error.set(None); let management = management.clone(); let workspace = workspace.clone(); let remove_files = delete_files(); spawn(async move {
                        match management.remove(&workspace, remove_files).await { Ok(()) => { on_notice.call(("Workspace removed.".into(), Tone::Success)); on_changed.call(()); dialog.set(ManagementDialog::None); }, Err(problem) => { error.set(Some(problem.message)); busy.set(false); } }
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
        description: "Remove reproducible caches or installed developer tools.",
        on_close: move |()| if !busy() { dialog.set(ManagementDialog::None) },
        DialogForm {
            CleanupChoice { checked: caches(), disabled: busy(), title: "Downloaded packages and build caches", on_change: move |value| caches.set(value) }
            CleanupChoice { checked: mise(), disabled: busy(), title: "Tools installed with Mise", on_change: move |value| { mise.set(value); confirmed.set(false); } }
            CleanupChoice { checked: tools(), disabled: busy(), title: "Other installed developer tools", on_change: move |value| { tools.set(value); confirmed.set(false); } }
            if destructive { SlideToConfirm { disabled: busy(), tone: Tone::Destructive, label: "Slide to confirm removing tools", confirmed_label: "Tool removal confirmed", on_confirmed: move |value| confirmed.set(value) } }
            if let Some(message) = error() { p { class: "text-xs text-destructive", role: "alert", "{message}" } }
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
    on_change: EventHandler<bool>,
) -> Element {
    rsx! { label { class: "flex items-center gap-2 rounded-lg border border-border p-3",
        Checkbox { checked, disabled, aria_label: title.clone(), on_checked_change: move |value| on_change.call(value) }
        strong { class: "text-sm", "{title}" }
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
    let target = workspace.clone();
    let bootstrap_files = files.clone();
    let command = use_resource(move || {
        let target = target.clone();
        let bootstrap_files = bootstrap_files.clone();
        async move { bootstrap_command(&bootstrap_files, &target).await }
    });
    match command() {
        None => {
            rsx! { Modal { title: format!("Bootstrap {}", workspace.name), description: "Inspecting the project…", on_close: move |()| dialog.set(ManagementDialog::None), div { class: "p-8 text-center text-sm text-muted-foreground", "Inspecting project tool requirements…" } } }
        }
        Some(Err(problem)) => {
            rsx! { Modal { title: format!("Bootstrap {}", workspace.name), description: "Project inspection failed.", on_close: move |()| dialog.set(ManagementDialog::None), div { class: "p-5 text-sm text-destructive", "{problem.message}" } } }
        }
        Some(Ok(command)) => {
            rsx! { ToolTerminalDialog { workspace, command, title: "Bootstrap project", description: "Install this project's toolchain with mise.", dialog, on_notice: move |notice: (String, Tone)| { if notice.1 == Tone::Success { on_changed.call(()); } on_notice.call(notice); } } }
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
        title,
        description,
        content_class: "max-w-180",
        on_close: move |()| dialog.set(ManagementDialog::None),
        div { class: "px-5 pt-3 pb-5",
            div { class: "h-[min(34rem,calc(100svh-13rem))] min-h-72 overflow-hidden rounded-lg border border-border bg-background",
                syntaxis_module_terminal::ProjectInitializerTerminal { workspace, command, label: "Workspace tool setup", on_finished: move |success| { result.set(Some(success)); on_notice.call((if success { format!("{workspace_name} is ready.") } else { format!("Tool setup failed for {workspace_name}.") }, if success { Tone::Success } else { Tone::Destructive })); } }
            }
            p { class: "mt-2 text-xs text-muted-foreground", if result() == Some(true) { "Setup finished successfully." } else if result() == Some(false) { "Setup exited with an error." } else { "You can leave while setup continues in its terminal session." } }
            div { class: "mt-4 flex justify-end", Button { label: "Close", kind: ButtonKind::Primary, onclick: move |_| dialog.set(ManagementDialog::None) } }
        }
    } }
}

async fn bootstrap_command(
    files: &syntaxis_module_files::FilesPorts,
    workspace: &WorkspaceRecord,
) -> Result<String, AppError> {
    let root = files
        .files()
        .list(workspace, &RelativePath::root())
        .await
        .map_err(AppError::from)?;
    let names = root
        .iter()
        .filter(|entry| entry.kind == EntryKind::File)
        .map(|entry| entry.name.as_str())
        .collect::<HashSet<_>>();
    let configured = [
        "mise.toml",
        ".mise.toml",
        "mise.local.toml",
        ".mise.local.toml",
        ".tool-versions",
    ]
    .iter()
    .any(|name| names.contains(name));
    if configured {
        return Ok("mise trust --yes && mise install --yes".into());
    }
    let mut tools = Vec::new();
    if names.contains("Cargo.toml") {
        tools.push("rust@stable");
    }
    if names.contains("deno.json") || names.contains("deno.jsonc") {
        tools.push("deno@latest");
    } else if names.contains("bun.lock") || names.contains("bun.lockb") {
        tools.push("bun@latest");
    } else if names.contains("package.json") {
        tools.push("node@lts");
    }
    if names.contains("pyproject.toml") || names.contains("requirements.txt") {
        tools.push("python@latest");
    }
    if names.contains("go.mod") || names.contains("go.work") {
        tools.push("go@latest");
    }
    if names.contains("global.json") {
        tools.push("dotnet@latest");
    }
    if tools.is_empty() {
        return Err(AppError::unsupported(
            "No mise configuration or supported project markers were found.",
            ErrorSource::Workspace,
        ));
    }
    Ok(format!(
        "mise use --yes --env local {} && mise install --yes",
        tools.join(" ")
    ))
}

#[cfg(test)]
mod tests {
    use super::UPDATE_TOOLS_COMMAND;

    #[test]
    fn project_update_is_scoped_to_local_configuration() {
        assert!(UPDATE_TOOLS_COMMAND.contains("mise upgrade --local"));
        assert!(UPDATE_TOOLS_COMMAND.contains("mise trust --yes"));
    }
}
