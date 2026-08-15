//! Terminal session and renderer action menu composition.

use super::super::renderer::{RendererAction, RendererCommand};
use super::super::runtime::send_renderer_action;
use super::TerminalAction;
use super::components::TerminalMenuItem;
use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::DropdownMenu;
use syntaxis_terminal::{ClientMessage, SessionSummary};
use syntaxis_ui::prelude::{AppIcon, MenuContent, MenuTrigger};

pub(super) fn terminal_actions_menu(
    mut menu: Signal<bool>,
    selected: Option<&SessionSummary>,
    sessions: Signal<Vec<SessionSummary>>,
    connection_ready: bool,
    renderer_command: Signal<Option<RendererCommand>>,
    renderer_command_sequence: Signal<u64>,
    client: Coroutine<ClientMessage>,
) -> Element {
    let renderer_items = renderer_actions(
        selected.is_some(),
        selected,
        renderer_command,
        renderer_command_sequence,
        client,
    );
    let session_items = session_actions(selected, sessions, connection_ready, client);
    rsx! {
        DropdownMenu {
            class: "relative order-2 shrink-0",
            open: menu(),
            on_open_change: move |open: bool| menu.set(open),
            MenuTrigger {
                label: "Terminal actions",
                icon: AppIcon::Menu,
                open: menu(),
                on_toggle: move |()| menu.toggle(),
            }
            MenuContent { class: "right-0 w-53.75",
                {renderer_items}
                hr {}
                {session_items}
            }
        }
    }
}

fn renderer_actions(
    has_selection: bool,
    selected: Option<&SessionSummary>,
    mut renderer_command: Signal<Option<RendererCommand>>,
    mut renderer_command_sequence: Signal<u64>,
    client: Coroutine<ClientMessage>,
) -> Element {
    let selected = selected.cloned();
    rsx! {
        TerminalMenuItem {
            action: TerminalAction::Copy,
            index: 0,
            label: "Copy selection",
            disabled: !has_selection,
            on_select: move |_| send_renderer_action(
                &mut renderer_command,
                &mut renderer_command_sequence,
                RendererAction::Copy,
            ),
        }
        TerminalMenuItem {
            action: TerminalAction::CopyAll,
            index: 1,
            label: "Copy all",
            disabled: !has_selection,
            on_select: move |_| send_renderer_action(
                &mut renderer_command,
                &mut renderer_command_sequence,
                RendererAction::CopyAll,
            ),
        }
        TerminalMenuItem {
            action: TerminalAction::Paste,
            index: 2,
            label: "Paste",
            disabled: !has_selection,
            on_select: move |_| send_renderer_action(
                &mut renderer_command,
                &mut renderer_command_sequence,
                RendererAction::Paste,
            ),
        }
        TerminalMenuItem {
            action: TerminalAction::Clear,
            index: 3,
            label: "Clear terminal",
            disabled: !has_selection,
            on_select: move |_| send_renderer_action(
                &mut renderer_command,
                &mut renderer_command_sequence,
                RendererAction::Clear,
            ),
        }
        TerminalMenuItem {
            action: TerminalAction::Restart,
            index: 4,
            label: "Restart terminal",
            disabled: !has_selection,
            on_select: {
                let selected = selected.clone();
                move |_| {
                    if let Some(session) = selected.as_ref() {
                        client
                            .send(ClientMessage::Close {
                                session_id: session.id.clone(),
                            });
                        client
                            .send(ClientMessage::Create {
                                name: Some(session.name.clone()),
                                size: session.size,
                            });
                    }
                }
            },
        }
    }
}

fn session_actions(
    selected: Option<&SessionSummary>,
    sessions: Signal<Vec<SessionSummary>>,
    connection_ready: bool,
    client: Coroutine<ClientMessage>,
) -> Element {
    let selected = selected.cloned();
    rsx! {
        TerminalMenuItem {
            action: TerminalAction::Detach,
            index: 5,
            label: "Detach session",
            disabled: selected.is_none(),
            on_select: {
                let selected = selected.clone();
                move |_| {
                    if let Some(session) = selected.as_ref() {
                        client
                            .send(ClientMessage::Detach {
                                session_id: session.id.clone(),
                            });
                    }
                }
            },
        }
        TerminalMenuItem {
            action: TerminalAction::Refresh,
            index: 6,
            label: "Refresh sessions",
            disabled: !connection_ready,
            on_select: move |_| client.send(ClientMessage::List),
        }
        hr {}
        TerminalMenuItem {
            action: TerminalAction::Close,
            index: 7,
            label: "Close terminal",
            destructive: true,
            disabled: selected.is_none(),
            on_select: {
                let selected = selected.clone();
                move |_| {
                    if let Some(session) = selected.as_ref() {
                        client
                            .send(ClientMessage::Close {
                                session_id: session.id.clone(),
                            });
                    }
                }
            },
        }
        TerminalMenuItem {
            action: TerminalAction::CloseOthers,
            index: 8,
            label: "Close all others",
            destructive: true,
            disabled: selected.is_none() || sessions.read().len() < 2,
            on_select: {
                let selected = selected.clone();
                move |_| {
                    if let Some(selected) = selected.as_ref() {
                        for session in sessions() {
                            if session.id != selected.id {
                                client
                                    .send(ClientMessage::Close {
                                        session_id: session.id,
                                    });
                            }
                        }
                    }
                }
            },
        }
        TerminalMenuItem {
            action: TerminalAction::CloseAll,
            index: 9,
            label: "Close all terminals",
            destructive: true,
            disabled: sessions.read().is_empty(),
            on_select: move |_| client.send(ClientMessage::CloseAll),
        }
    }
}
