use dioxus::prelude::*;
use syntaxis_terminal::RunCommand;
use syntaxis_ui::prelude::{
    AppIcon, ControlSize, IconButton, NewTerminalDialog, PanelHeader, PanelTab, PanelTabIndicator,
    PanelTabList, PanelTabWidth, RunCommandMenu, TerminalActionsMenu, TerminalEmptyState,
    TerminalMenuAction, TerminalMobileTabs, TerminalStatusBar, TerminalTab, Toast, Tone,
};
use syntaxis_workspace::{ChangeKind, WorkspaceChange, WorkspaceRecord};

use crate::TerminalPorts;

const MAX_COMMAND_HISTORY: usize = 50;

#[derive(Clone, Debug, Eq, PartialEq)]
struct CommandRecord {
    command: String,
    stdout: String,
    stderr: String,
    exit_code: i32,
    changes: Vec<WorkspaceChange>,
    reconciliation_succeeded: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CommandTab {
    id: u64,
    name: String,
    command: String,
    history: Vec<CommandRecord>,
    history_cursor: Option<usize>,
}

impl CommandTab {
    fn new(id: u64, name: String) -> Self {
        Self {
            id,
            name,
            command: String::new(),
            history: Vec::new(),
            history_cursor: None,
        }
    }
}

#[component]
pub(crate) fn CommandTerminal(workspace: WorkspaceRecord, terminal_script: String) -> Element {
    let ports = use_context::<TerminalPorts>();
    let runner = ports
        .command_runner()
        .cloned()
        .expect("command mode requires a command runner port");
    let mut command = use_signal(String::new);
    let mut tabs = use_signal(Vec::<CommandTab>::new);
    let mut active_tab_id = use_signal(|| None::<u64>);
    let mut next_tab_id = use_signal(|| 1_u64);
    let mut history = use_signal(Vec::<CommandRecord>::new);
    let mut history_cursor = use_signal(|| None::<usize>);
    let mut running = use_signal(|| false);
    let mut notice = use_signal(|| None::<String>);
    let mut new_terminal_open = use_signal(|| false);
    let mut new_terminal_name = use_signal(String::new);
    let mut new_terminal_error = use_signal(|| None::<String>);
    let run_menu_open = use_signal(|| false);
    let terminal_menu_open = use_signal(|| false);
    let mobile_tabs_open = use_signal(|| false);
    let mut command_refresh = use_signal(|| 0_u64);

    let ready_runner = runner.clone();
    let bridge_status = use_resource(move || {
        let runner = ready_runner.clone();
        async move { runner.ready().await }
    });
    let bridge_ready = bridge_status().is_some_and(|result| result.is_ok());
    let bridge_message = match bridge_status() {
        None => "Loading the browser shell…".to_owned(),
        Some(Ok(())) => "just-bash · local sandbox".to_owned(),
        Some(Err(error)) => error.message,
    };

    let command_workspace = workspace.clone();
    let command_ports = ports.clone();
    let project_commands = use_resource(move || {
        let workspace = command_workspace.clone();
        let commands = command_ports.commands().cloned();
        let _refresh = command_refresh();
        async move {
            let Some(commands) = commands else {
                return Ok(Vec::new());
            };
            commands.list(&workspace).await
        }
    });
    let detected_commands = project_commands().and_then(Result::ok).unwrap_or_default();

    let submit_runner = runner.clone();
    let submit_workspace = workspace.clone();
    let run_command = EventHandler::new(move |requested: Option<RunCommand>| {
        if let Some(requested) = requested {
            command.set(requested.command);
        }
        let value = command().trim().to_owned();
        if value.is_empty() || running() {
            return;
        }
        command.set(String::new());
        history_cursor.set(None);
        running.set(true);
        let runner = submit_runner.clone();
        let workspace = submit_workspace.clone();
        spawn(async move {
            let record = match runner.execute(&workspace, &value).await {
                Ok(result) => CommandRecord {
                    command: value,
                    stdout: result.stdout,
                    stderr: result.stderr,
                    exit_code: result.exit_code,
                    changes: result.changes,
                    reconciliation_succeeded: result.reconciliation_succeeded,
                },
                Err(error) => CommandRecord {
                    command: value,
                    stdout: String::new(),
                    stderr: format!("{}\n", error.message),
                    exit_code: 1,
                    changes: Vec::new(),
                    reconciliation_succeeded: false,
                },
            };
            let mut records = history.write();
            if records.len() >= MAX_COMMAND_HISTORY {
                records.remove(0);
            }
            records.push(record);
            drop(records);
            persist_active_tab(tabs, active_tab_id(), &command(), &history(), history_cursor());
            running.set(false);
        });
    });

    rsx! {
        document::Script { src: terminal_script }
        section {
            class: "flex size-full min-h-0 flex-col bg-background",
            "aria-label": "Browser terminal",
            PanelHeader {
                PanelTabList {
                    for tab in tabs() {
                        PanelTab {
                            key: "{tab.id}",
                            label: tab.name.clone(),
                            active: active_tab_id() == Some(tab.id),
                            width: PanelTabWidth::Session,
                            indicator: PanelTabIndicator::Dot(Tone::Success),
                            on_select: move |_| {
                                if !running() {
                                    select_tab(tab.id, tabs, active_tab_id, command, history, history_cursor);
                                }
                            },
                            on_close: move |()| if !running() {
                                close_tab(tab.id, tabs, active_tab_id, command, history, history_cursor);
                            },
                        }
                    }
                }
                TerminalMobileTabs {
                    tabs: tabs().into_iter().map(|tab| TerminalTab {
                        id: tab.id.to_string(),
                        name: tab.name,
                        tone: Tone::Success,
                    }).collect(),
                    active_id: active_tab_id().map(|id| id.to_string()),
                    open: mobile_tabs_open,
                    on_select: move |id: String| if !running()
                        && let Ok(id) = id.parse::<u64>() {
                        select_tab(id, tabs, active_tab_id, command, history, history_cursor);
                    },
                    on_close: move |id: String| if !running()
                        && let Ok(id) = id.parse::<u64>() {
                        close_tab(id, tabs, active_tab_id, command, history, history_cursor);
                    },
                }
                IconButton {
                    label: "New terminal",
                    icon: AppIcon::Plus,
                    size: ControlSize::Small,
                    disabled: running() || !bridge_ready,
                    onclick: move |_| {
                        new_terminal_name.set(format!("shell {}", next_tab_id()));
                        new_terminal_open.set(true);
                    },
                }
                RunCommandMenu {
                    commands: detected_commands,
                    open: run_menu_open,
                    loading: project_commands.state() == UseResourceState::Pending,
                    disabled: !bridge_ready || running() || active_tab_id().is_none(),
                    show_add: false,
                    on_run: move |command| run_command.call(Some(command)),
                    on_add: move |()| {},
                    on_refresh: move |()| command_refresh += 1,
                    on_delete: move |_| {},
                }
                if running() {
                    IconButton {
                        label: "Stop command",
                        icon: AppIcon::Stop,
                        size: ControlSize::Small,
                        onclick: {
                            let runner = runner.clone();
                            move |_| if let Err(error) = runner.cancel() {
                                notice.set(Some(error.message));
                            }
                        },
                    }
                }
                TerminalActionsMenu {
                    open: terminal_menu_open,
                    terminal_available: active_tab_id().is_some() && !running(),
                    renderer_actions: false,
                    restart_available: active_tab_id().is_some() && !running(),
                    detach_available: false,
                    refresh_available: !running(),
                    terminal_count: if running() { 0 } else { tabs.read().len() },
                    on_action: move |action| if !running() {
                        match action {
                            TerminalMenuAction::Clear | TerminalMenuAction::Restart => {
                                history.set(Vec::new());
                                history_cursor.set(None);
                                command.set(String::new());
                            }
                            TerminalMenuAction::Refresh => command_refresh += 1,
                            TerminalMenuAction::Close => if let Some(id) = active_tab_id() {
                                close_tab(id, tabs, active_tab_id, command, history, history_cursor);
                            },
                            TerminalMenuAction::CloseOthers => if let Some(id) = active_tab_id() {
                                tabs.write().retain(|tab| tab.id == id);
                            },
                            TerminalMenuAction::CloseAll => {
                                tabs.write().clear();
                                active_tab_id.set(None);
                                command.set(String::new());
                                history.set(Vec::new());
                                history_cursor.set(None);
                            }
                            TerminalMenuAction::CopySelection
                            | TerminalMenuAction::CopyAll
                            | TerminalMenuAction::Paste
                            | TerminalMenuAction::Detach => {}
                        }
                    },
                }
            }
            div { class: "relative min-h-0 flex-1 overflow-auto bg-card p-3",
                if tabs.read().is_empty() {
                    TerminalEmptyState {
                        description: "Create a browser terminal in this workspace.",
                        disabled: !bridge_ready,
                        on_new: move |()| {
                            new_terminal_name.set(format!("shell {}", next_tab_id()));
                            new_terminal_open.set(true);
                        },
                    }
                } else {
                    div { class: "space-y-3 font-mono text-xs",
                        for record in history() {
                            article { class: "space-y-1",
                                div { class: "flex gap-2 text-muted-foreground",
                                    span { "dev@browser:/workspace$" }
                                    code { class: "text-foreground", "{record.command}" }
                                }
                                if !record.stdout.is_empty() { pre { class: "whitespace-pre-wrap", "{record.stdout}" } }
                                if !record.stderr.is_empty() { pre { class: "whitespace-pre-wrap text-destructive", "{record.stderr}" } }
                                if record.exit_code != 0 { small { "Exited with {record.exit_code}" } }
                                if record.reconciliation_succeeded && !record.changes.is_empty() {
                                    small { class: "text-muted-foreground", "Workspace updated: {change_summary(&record.changes)}" }
                                }
                            }
                        }
                        if running() { p { class: "text-muted-foreground", "Running locally…" } }
                        form {
                            class: "flex items-center gap-2",
                            onsubmit: move |event| { event.prevent_default(); run_command.call(None); },
                            span { class: "shrink-0 text-muted-foreground", "dev@browser:/workspace$" }
                            input {
                                class: "min-w-0 flex-1 bg-transparent text-foreground outline-none",
                                autofocus: true,
                                value: command,
                                disabled: running() || !bridge_ready,
                                autocomplete: "off",
                                autocapitalize: "off",
                                spellcheck: false,
                                title: bridge_message.clone(),
                                oninput: move |event| { command.set(event.value()); history_cursor.set(None); },
                                onkeydown: move |event: KeyboardEvent| {
                                    let records = history();
                                    if records.is_empty() || running() || !bridge_ready { return; }
                                    match event.key() {
                                        Key::ArrowUp => {
                                            event.prevent_default();
                                            let index = history_cursor().map_or(records.len() - 1, |index| index.saturating_sub(1));
                                            command.set(records[index].command.clone());
                                            history_cursor.set(Some(index));
                                        }
                                        Key::ArrowDown => {
                                            event.prevent_default();
                                            match history_cursor() {
                                                Some(index) if index + 1 < records.len() => {
                                                    command.set(records[index + 1].command.clone());
                                                    history_cursor.set(Some(index + 1));
                                                }
                                                Some(_) => { command.set(String::new()); history_cursor.set(None); }
                                                None => {}
                                            }
                                        }
                                        _ => {}
                                    }
                                },
                            }
                        }
                    }
                }
            }
            TerminalStatusBar {
                title: "Generated and internal directories are protected from the bounded browser shell.",
                label: "Browser command console · local just-bash · generated folders protected",
            }
        }
        if new_terminal_open() {
            NewTerminalDialog {
                open: new_terminal_open,
                name: new_terminal_name,
                error: new_terminal_error,
                busy: false,
                name_error: new_terminal_error(),
                create_disabled: new_terminal_name().trim().is_empty(),
                shell_label: "Browser just-bash",
                on_submit: move |()| {
                    let name = new_terminal_name().trim().to_owned();
                    if name.is_empty() {
                        new_terminal_error.set(Some("Enter a terminal name.".into()));
                        return;
                    }
                    if tabs.read().iter().any(|tab| tab.name == name) {
                        new_terminal_error.set(Some("Terminal names must be unique.".into()));
                        return;
                    }
                    persist_active_tab(tabs, active_tab_id(), &command(), &history(), history_cursor());
                    let id = next_tab_id();
                    next_tab_id += 1;
                    tabs.write().push(CommandTab::new(id, name));
                    active_tab_id.set(Some(id));
                    command.set(String::new());
                    history.set(Vec::new());
                    history_cursor.set(None);
                    new_terminal_name.set(String::new());
                    new_terminal_error.set(None);
                    new_terminal_open.set(false);
                },
            }
        }
        if let Some(message) = notice() {
            Toast { message, on_close: move |()| notice.set(None) }
        }
    }
}

fn persist_active_tab(
    mut tabs: Signal<Vec<CommandTab>>,
    active_id: Option<u64>,
    command: &str,
    history: &[CommandRecord],
    history_cursor: Option<usize>,
) {
    let Some(active_id) = active_id else { return };
    if let Some(tab) = tabs.write().iter_mut().find(|tab| tab.id == active_id) {
        tab.command = command.to_owned();
        tab.history = history.to_vec();
        tab.history_cursor = history_cursor;
    }
}

fn select_tab(
    id: u64,
    tabs: Signal<Vec<CommandTab>>,
    mut active_id: Signal<Option<u64>>,
    mut command: Signal<String>,
    mut history: Signal<Vec<CommandRecord>>,
    mut history_cursor: Signal<Option<usize>>,
) {
    persist_active_tab(tabs, active_id(), &command(), &history(), history_cursor());
    let Some(tab) = tabs.read().iter().find(|tab| tab.id == id).cloned() else { return };
    active_id.set(Some(id));
    command.set(tab.command);
    history.set(tab.history);
    history_cursor.set(tab.history_cursor);
}

fn close_tab(
    id: u64,
    mut tabs: Signal<Vec<CommandTab>>,
    mut active_id: Signal<Option<u64>>,
    mut command: Signal<String>,
    mut history: Signal<Vec<CommandRecord>>,
    mut history_cursor: Signal<Option<usize>>,
) {
    tabs.write().retain(|tab| tab.id != id);
    if active_id() != Some(id) {
        return;
    }
    if let Some(next) = tabs.read().first().cloned() {
        active_id.set(Some(next.id));
        command.set(next.command);
        history.set(next.history);
        history_cursor.set(next.history_cursor);
    } else {
        active_id.set(None);
        command.set(String::new());
        history.set(Vec::new());
        history_cursor.set(None);
    }
}

fn change_summary(changes: &[WorkspaceChange]) -> String {
    let created = changes.iter().filter(|change| change.kind == ChangeKind::Created).count();
    let modified = changes.iter().filter(|change| change.kind == ChangeKind::Modified).count();
    let removed = changes.iter().filter(|change| change.kind == ChangeKind::Removed).count();
    [
        (created, "created"),
        (modified, "modified"),
        (removed, "removed"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, label)| format!("{count} {label}"))
    .collect::<Vec<_>>()
    .join(", ")
}
