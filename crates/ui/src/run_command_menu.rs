use crate::{AppIcon, Icon, MenuContent, MenuTrigger};
use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use syntaxis_terminal::RunCommand;
/// Shared project-command dropdown used by native and browser terminals.
#[component]
pub fn RunCommandMenu(
    commands: Vec<RunCommand>,
    mut open: Signal<bool>,
    loading: bool,
    disabled: bool,
    #[props(default)] disabled_reason: Option<String>,
    #[props(default = true)] show_add: bool,
    on_run: EventHandler<RunCommand>,
    on_add: EventHandler<()>,
    on_refresh: EventHandler<()>,
    on_delete: EventHandler<String>,
) -> Element {
    rsx! {
        DropdownMenu {
            class: "relative order-3 shrink-0",
            open: open(),
            on_open_change: move |next: bool| open.set(next),
            MenuTrigger {
                label: "Run command",
                icon: AppIcon::Play,
                open: open(),
                on_toggle: move |()| open.toggle(),
            }
            MenuContent { class: "right-0 max-h-[min(32rem,calc(100svh-4rem))] w-72 overflow-y-auto",
                if loading && commands.is_empty() {
                    div { class: "px-2 py-2 text-xs text-muted-foreground",
                        "Detecting project commands…"
                    }
                } else if commands.is_empty() {
                    div { class: "px-2 py-2 text-xs text-muted-foreground",
                        "No project commands detected"
                    }
                }
                if let Some(reason) = disabled_reason.as_deref() {
                    div { class: "border-b border-border px-2 py-2 text-[10px] leading-relaxed text-muted-foreground",
                        "{reason}"
                    }
                }
                for (index, command) in commands.iter().cloned().enumerate() {
                    DropdownMenuItem::<usize> {
                        value: index,
                        index,
                        disabled,
                        on_select: {
                            let command = command.clone();
                            move |_| on_run.call(command.clone())
                        },
                        div { class: "flex min-w-0 flex-1 flex-col gap-0.5 text-left",
                            span { class: "truncate", "{command.label}" }
                            span { class: "truncate text-[10px] text-muted-foreground",
                                "{command.command}"
                            }
                        }
                        if command.custom {
                            button {
                                class: "-my-1 -mr-1 inline-flex size-7 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-destructive/12 hover:text-destructive",
                                r#type: "button",
                                title: "Delete custom command",
                                "aria-label": "Delete {command.label}",
                                onclick: {
                                    let command_id = command.id.clone();
                                    move |event: MouseEvent| {
                                        event.stop_propagation();
                                        on_delete.call(command_id.clone());
                                    }
                                },
                                Icon { icon: AppIcon::Delete, size: 13 }
                            }
                        }
                    }
                }
                hr {}
                if show_add {
                    DropdownMenuItem::<usize> {
                        value: commands.len(),
                        index: commands
                                                                                                                                                                                                                                                                                                                                                        .len(),
                        on_select: move |_| on_add.call(()),
                        span { class: "flex items-center gap-2",
                            Icon { icon: AppIcon::Plus, size: 14 }
                            "Add command"
                        }
                    }
                }
                DropdownMenuItem::<usize> {
                    value: commands.len() +
                                                                                                                                                                                                                                                                                                usize::from(show_add),
                    index: commands.len() + usize::from(show_add),
                    disabled: loading,
                    on_select: move |_| on_refresh.call(()),
                    span { class: "flex items-center gap-2",
                        Icon { icon: AppIcon::Refresh, size: 14 }
                        if loading {
                            "Refreshing…"
                        } else {
                            "Refresh"
                        }
                    }
                }
            }
        }
    }
}
