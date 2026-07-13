use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{
    DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger,
};

use crate::ui::{AppIcon, Button, ButtonKind, MenuTrigger, Modal, Toast};

#[derive(Clone, Copy, PartialEq, Eq)]
enum TerminalStatus {
    Ready,
    Connecting,
    Exited,
    Failed,
}

#[derive(Clone, PartialEq, Eq)]
struct TerminalSession {
    id: u32,
    name: String,
    status: TerminalStatus,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TerminalAction {
    Copy,
    Paste,
    Clear,
    Restart,
    Detach,
    Close,
    CloseOthers,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum QuickCommandAction {
    Ci,
    Test,
    Build,
    Dev,
    New,
}

#[component]
pub fn Terminal(slug: String) -> Element {
    let _ = slug;
    let mut active = use_signal(|| Some(1_u32));
    let mut sessions = use_signal(|| {
        vec![
            TerminalSession {
                id: 1,
                name: "dev server".into(),
                status: TerminalStatus::Ready,
            },
            TerminalSession {
                id: 2,
                name: "tests".into(),
                status: TerminalStatus::Exited,
            },
            TerminalSession {
                id: 3,
                name: "build".into(),
                status: TerminalStatus::Failed,
            },
        ]
    });
    let mut next_id = use_signal(|| 4_u32);
    let mut menu = use_signal(|| false);
    let mut play_menu = use_signal(|| false);
    let mut mobile_tabs_open = use_signal(|| false);
    let mut new_dialog = use_signal(|| false);
    let mut command_dialog = use_signal(|| false);
    let mut command_text = use_signal(String::new);
    let mut command_is_new = use_signal(|| false);
    let mut toast = use_signal(|| None::<String>);

    rsx! {
        section { class: "terminal-layout",
            div { class: "terminal-header",
                div { class: "terminal-tabs", role: "tablist",
                    for session in sessions() {
                        div { class: if active() == Some(session.id) { "terminal-tab active" } else { "terminal-tab" },
                            button {
                                class: "tab-select",
                                role: "tab",
                                "aria-selected": active() == Some(session.id),
                                onclick: move |_| active.set(Some(session.id)),
                                span { class: terminal_dot_class(session.status) }
                                span { {session.name.clone()} }
                            }
                            button {
                                class: "tab-close",
                                "aria-label": "Close {session.name}",
                                title: "Close {session.name}",
                                onclick: move |_| close_terminal(session.id, &session.name, sessions, active, toast),
                                crate::ui::Icon { icon: AppIcon::Close, size: 12 }
                            }
                        }
                    }
                }
                DropdownMenu {
                    class: "mobile-terminal-tabs",
                    open: mobile_tabs_open(),
                    on_open_change: move |open: bool| mobile_tabs_open.set(open),
                    DropdownMenuTrigger {
                        class: "mobile-terminal-tabs-trigger",
                        "aria-label": "Open terminal tabs",
                        "aria-expanded": mobile_tabs_open(),
                        span { class: "mobile-terminal-value",
                            if let Some(session) = sessions
                                .read()
                                .iter()
                                .find(|session| Some(session.id) == active())
                            {
                                span { class: terminal_dot_class(session.status) }
                                span { "{session.name}" }
                            } else {
                                "No terminal"
                            }
                        }
                        span { class: "mobile-tabs-chevron", "⌄" }
                    }
                    DropdownMenuContent { class: "dropdown mobile-terminal-tabs-dropdown",
                        if sessions.read().is_empty() {
                            div { class: "mobile-tabs-empty", "No terminal sessions" }
                        }
                        for (index, session) in sessions().into_iter().enumerate() {
                            div { class: if active() == Some(session.id) { "mobile-terminal-option active" } else { "mobile-terminal-option" },
                                DropdownMenuItem::<u32> {
                                    class: "mobile-terminal-select",
                                    value: session.id,
                                    index,
                                    on_select: move |id| {
                                        active.set(Some(id));
                                        mobile_tabs_open.set(false);
                                    },
                                    span { class: terminal_dot_class(session.status) }
                                    span { class: "mobile-terminal-label", "{session.name}" }
                                }
                                button {
                                    class: "mobile-terminal-close",
                                    "aria-label": "Close {session.name}",
                                    title: "Close {session.name}",
                                    onclick: move |event| {
                                        event.stop_propagation();
                                        close_terminal(session.id, &session.name, sessions, active, toast);
                                        mobile_tabs_open.set(false);
                                    },
                                    crate::ui::Icon { icon: AppIcon::Close, size: 15 }
                                }
                            }
                        }
                    }
                }
                button {
                    class: "new-tab",
                    "aria-label": "New terminal",
                    title: "New terminal",
                    onclick: move |_| new_dialog.set(true),
                    "+"
                }
                DropdownMenu {
                    class: "terminal-play menu-anchor",
                    open: play_menu(),
                    on_open_change: move |open: bool| play_menu.set(open),
                    MenuTrigger {
                        label: "Quick commands",
                        icon: AppIcon::Play,
                        open: play_menu(),
                    }
                    DropdownMenuContent { class: "dropdown align-right terminal-play-dropdown",
                        DropdownMenuItem::<QuickCommandAction> {
                            value: QuickCommandAction::Ci,
                            index: 0_usize,
                            on_select: move |_| {
                                command_is_new.set(false);
                                command_text.set("npm run ci".into());
                                command_dialog.set(true);
                            },
                            div { class: "terminal-command-entry",
                                span { class: "terminal-command-label", "npm run ci" }
                                span { class: "terminal-command-meta", "Run CI checks in a new terminal" }
                            }
                        }
                        DropdownMenuItem::<QuickCommandAction> {
                            value: QuickCommandAction::Test,
                            index: 1_usize,
                            on_select: move |_| {
                                command_is_new.set(false);
                                command_text.set("cargo test --workspace".into());
                                command_dialog.set(true);
                            },
                            div { class: "terminal-command-entry",
                                span { class: "terminal-command-label", "cargo test --workspace" }
                                span { class: "terminal-command-meta",
                                    "Execute the workspace test suite"
                                }
                            }
                        }
                        DropdownMenuItem::<QuickCommandAction> {
                            value: QuickCommandAction::Build,
                            index: 2_usize,
                            on_select: move |_| {
                                command_is_new.set(false);
                                command_text.set("cargo build --workspace".into());
                                command_dialog.set(true);
                            },
                            div { class: "terminal-command-entry",
                                span { class: "terminal-command-label", "cargo build --workspace" }
                                span { class: "terminal-command-meta", "Compile the current workspace" }
                            }
                        }
                        DropdownMenuItem::<QuickCommandAction> {
                            value: QuickCommandAction::Dev,
                            index: 3_usize,
                            on_select: move |_| {
                                command_is_new.set(false);
                                command_text.set("npm run dev".into());
                                command_dialog.set(true);
                            },
                            div { class: "terminal-command-entry",
                                span { class: "terminal-command-label", "npm run dev" }
                                span { class: "terminal-command-meta",
                                    "Start a local development server"
                                }
                            }
                        }
                        hr {}
                        DropdownMenuItem::<QuickCommandAction> {
                            value: QuickCommandAction::New,
                            index: 4_usize,
                            on_select: move |_| {
                                command_is_new.set(true);
                                command_text.set(String::new());
                                command_dialog.set(true);
                            },
                            "+ New command"
                        }
                    }
                }
                DropdownMenu {
                    class: "terminal-actions menu-anchor",
                    open: menu(),
                    on_open_change: move |open: bool| menu.set(open),
                    MenuTrigger {
                        label: "Terminal actions",
                        icon: AppIcon::Menu,
                        open: menu(),
                    }
                    DropdownMenuContent { class: "dropdown align-right terminal-actions-dropdown",
                        DropdownMenuItem::<TerminalAction> {
                            value: TerminalAction::Copy,
                            index: 0_usize,
                            disabled: active().is_none(),
                            on_select: move |_| toast.set(Some("Terminal output copied".into())),
                            "Copy terminal output"
                        }
                        DropdownMenuItem::<TerminalAction> {
                            value: TerminalAction::Paste,
                            index: 1_usize,
                            disabled: active().is_none(),
                            on_select: move |_| toast.set(Some("Clipboard content pasted (mock)".into())),
                            "Paste"
                        }
                        DropdownMenuItem::<TerminalAction> {
                            value: TerminalAction::Clear,
                            index: 2_usize,
                            disabled: active().is_none(),
                            on_select: move |_| toast.set(Some("Terminal cleared (mock)".into())),
                            "Clear terminal"
                        }
                        DropdownMenuItem::<TerminalAction> {
                            value: TerminalAction::Restart,
                            index: 3_usize,
                            disabled: active().is_none(),
                            on_select: move |_| {
                                set_selected_status(&mut sessions, active(), TerminalStatus::Connecting);
                                toast.set(Some("Terminal restarting…".into()));
                            },
                            "Restart terminal"
                        }
                        hr {}
                        DropdownMenuItem::<TerminalAction> {
                            value: TerminalAction::Detach,
                            index: 4_usize,
                            disabled: active().is_none(),
                            on_select: move |_| toast.set(Some("Session detached (mock)".into())),
                            "Detach session"
                        }
                        hr {}
                        DropdownMenuItem::<TerminalAction> {
                            value: TerminalAction::Close,
                            index: 5_usize,
                            class: "destructive-text",
                            disabled: active().is_none(),
                            on_select: move |_| {
                                if let Some(active_id) = active() {
                                    let mut current = sessions();
                                    let next = close_session(&mut current, active(), active_id);
                                    sessions.set(current);
                                    active.set(next);
                                    toast.set(Some("Terminal closed".into()));
                                }
                            },
                            "Close terminal"
                        }
                        DropdownMenuItem::<TerminalAction> {
                            value: TerminalAction::CloseOthers,
                            index: 6_usize,
                            class: "destructive-text",
                            disabled: active().is_none() || sessions.read().len() <= 1,
                            on_select: move |_| {
                                if let Some(active_id) = active() {
                                    sessions.write().retain(|session| session.id == active_id);
                                    toast.set(Some("Other terminals closed".into()));
                                }
                            },
                            "Close all others"
                        }
                    }
                }
            }
            div {
                class: "terminal-screen",
                role: "log",
                "aria-label": "Mock terminal output",
                if active().is_none() {
                    div { class: "terminal-failure",
                        strong { "No terminal sessions" }
                        p { "Create a terminal to start a new mock session." }
                        Button {
                            label: "New terminal",
                            kind: ButtonKind::Primary,
                            onclick: move |_| new_dialog.set(true),
                        }
                    }
                } else if selected_status(&sessions(), active()) == Some(TerminalStatus::Connecting) {
                    div { class: "terminal-overlay",
                        span { class: "spinner" }
                        "Connecting to workspace terminal…"
                    }
                } else if selected_status(&sessions(), active()) == Some(TerminalStatus::Failed) {
                    div { class: "terminal-failure",
                        strong { "Terminal failed to start" }
                        p { "The mock process exited before a shell could be attached." }
                        Button {
                            label: "Try again",
                            kind: ButtonKind::Primary,
                            onclick: move |_| set_selected_status(&mut sessions, active(), TerminalStatus::Connecting),
                        }
                    }
                } else {
                    pre {
                        span { class: "ansi-muted", "Syntaxis workspace terminal · replayed 1.7 KB\n\n" }
                        span { class: "ansi-green", "alex@workstation" }
                        span { class: "ansi-muted", ":" }
                        span { class: "ansi-blue", "~/projects/syntaxis" }
                        " $ "
                        span { "just ci-code\n" }
                        span { class: "ansi-cyan", "cargo fmt --all\n" }
                        span { class: "ansi-green", "✓ formatting complete\n" }
                        span { class: "ansi-cyan", "cargo clippy --workspace --all-targets\n" }
                        span { class: "ansi-yellow", "    Checking syntaxis v0.1.0\n" }
                        span { class: "ansi-green", "    Finished dev profile in 1.28s\n" }
                        span { class: "ansi-cyan", "cargo test --workspace\n" }
                        span { "running 4 tests\n" }
                        span { class: "ansi-green", "test result: ok. 4 passed; 0 failed\n\n" }
                        if selected_status(&sessions(), active()) == Some(TerminalStatus::Exited) {
                            span { class: "ansi-muted", "[process exited with code 0]\n" }
                        } else {
                            span { class: "ansi-green", "alex@workstation" }
                            span { class: "ansi-muted", ":" }
                            span { class: "ansi-blue", "~/projects/syntaxis" }
                            " $ "
                            span { class: "terminal-cursor", " " }
                        }
                    }
                }
            }
            footer { class: "terminal-footer",
                span { "Mock terminal · input disabled" }
                span { "80 × 24" }
            }
        }

        if new_dialog() {
            Modal {
                title: "New terminal",
                description: "Start a mock session in this workspace.",
                on_close: move |()| new_dialog.set(false),
                div { class: "form-stack",
                    label { r#for: "terminal-name", "Session name" }
                    input {
                        id: "terminal-name",
                        placeholder: "shell",
                        autofocus: true,
                    }
                    label { r#for: "terminal-command", "Startup command" }
                    input {
                        id: "terminal-command",
                        value: "/bin/bash",
                        disabled: true,
                    }
                    div { class: "modal-actions",
                        Button {
                            label: "Cancel",
                            kind: ButtonKind::Ghost,
                            onclick: move |_| new_dialog.set(false),
                        }
                        Button {
                            label: "Create terminal",
                            kind: ButtonKind::Primary,
                            onclick: move |_| {
                                let id = next_id();
                                next_id += 1;
                                sessions
                                    .write()
                                    .push(TerminalSession {
                                        id,
                                        name: format!("shell {id}"),
                                        status: TerminalStatus::Ready,
                                    });
                                active.set(Some(id));
                                new_dialog.set(false);
                                toast.set(Some("New terminal created".into()));
                            },
                        }
                    }
                }
            }
        }
        if command_dialog() {
            Modal {
                title: if command_is_new() { String::from("New command") } else { String::from("Run command") },
                description: if command_is_new() { String::from("Create a reusable mock command for this workspace.") } else { String::from("Open a new terminal and run this command (mock).") },
                on_close: move |()| command_dialog.set(false),
                div { class: "form-stack",
                    label { r#for: "terminal-command-text", "Command" }
                    input {
                        id: "terminal-command-text",
                        placeholder: "npm run ci",
                        value: "{command_text}",
                        autofocus: true,
                        oninput: move |event| command_text.set(event.value()),
                    }
                    div { class: "modal-actions",
                        Button {
                            label: "Cancel",
                            kind: ButtonKind::Ghost,
                            onclick: move |_| command_dialog.set(false),
                        }
                        Button {
                            label: if command_is_new() { String::from("Save command") } else { String::from("Open terminal") },
                            kind: ButtonKind::Primary,
                            onclick: move |_| {
                                if command_text().trim().is_empty() {
                                    toast.set(Some("Enter a command first".into()));
                                } else if command_is_new() {
                                    toast.set(Some(format!("Mock command saved: {}", command_text())));
                                    command_dialog.set(false);
                                } else {
                                    toast.set(Some(format!("Would run `{}` in a new terminal", command_text())));
                                    command_dialog.set(false);
                                }
                            },
                        }
                    }
                }
            }
        }
        if let Some(message) = toast() {
            Toast { message, on_close: move |()| toast.set(None) }
        }
    }
}

const fn terminal_dot_class(status: TerminalStatus) -> &'static str {
    match status {
        TerminalStatus::Ready => "terminal-dot ready",
        TerminalStatus::Connecting => "terminal-dot connecting",
        TerminalStatus::Exited => "terminal-dot exited",
        TerminalStatus::Failed => "terminal-dot failed",
    }
}

fn selected_status(sessions: &[TerminalSession], active: Option<u32>) -> Option<TerminalStatus> {
    let active = active?;
    sessions
        .iter()
        .find(|session| session.id == active)
        .map(|session| session.status)
}

fn set_selected_status(
    sessions: &mut Signal<Vec<TerminalSession>>,
    active: Option<u32>,
    status: TerminalStatus,
) {
    let Some(active) = active else {
        return;
    };
    if let Some(session) = sessions
        .write()
        .iter_mut()
        .find(|session| session.id == active)
    {
        session.status = status;
    }
}

fn close_terminal(
    id: u32,
    name: &str,
    mut sessions: Signal<Vec<TerminalSession>>,
    mut active: Signal<Option<u32>>,
    mut toast: Signal<Option<String>>,
) {
    let mut current = sessions();
    let next = close_session(&mut current, active(), id);
    sessions.set(current);
    active.set(next);
    toast.set(Some(format!("{name} closed")));
}

fn close_session(
    sessions: &mut Vec<TerminalSession>,
    active: Option<u32>,
    closing_id: u32,
) -> Option<u32> {
    let Some(closing_index) = sessions.iter().position(|session| session.id == closing_id) else {
        return active;
    };
    let closing_active = active == Some(closing_id);
    sessions.remove(closing_index);

    if !closing_active {
        return active;
    }
    if sessions.is_empty() {
        return None;
    }
    Some(sessions[closing_index.min(sessions.len() - 1)].id)
}

#[cfg(test)]
mod tests {
    use super::{close_session, TerminalSession, TerminalStatus};

    fn sessions() -> Vec<TerminalSession> {
        (1..=3)
            .map(|id| TerminalSession {
                id,
                name: format!("shell {id}"),
                status: TerminalStatus::Ready,
            })
            .collect()
    }

    #[test]
    fn closing_active_session_prefers_the_right_neighbor() {
        let mut sessions = sessions();
        assert_eq!(close_session(&mut sessions, Some(2), 2), Some(3));
    }

    #[test]
    fn closing_last_active_session_uses_the_left_neighbor() {
        let mut sessions = sessions();
        assert_eq!(close_session(&mut sessions, Some(3), 3), Some(2));
    }

    #[test]
    fn closing_inactive_session_preserves_active_id() {
        let mut sessions = sessions();
        assert_eq!(close_session(&mut sessions, Some(2), 1), Some(2));
    }

    #[test]
    fn closing_final_session_enters_empty_state() {
        let mut sessions = sessions();
        sessions.truncate(1);
        assert_eq!(close_session(&mut sessions, Some(1), 1), None);
        assert!(sessions.is_empty());
    }
}
