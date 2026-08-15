use dioxus::prelude::*;
use syntaxis_agent::PiCommand;
use syntaxis_ui::prelude::{AppIcon, Icon};

#[component]
pub(super) fn SlashCommandMenu(
    commands: Vec<PiCommand>,
    draft: Signal<String>,
    selected: usize,
) -> Element {
    rsx! {
        if !commands.is_empty() {
            div { class: "absolute right-0 bottom-[calc(100%+7px)] left-0 z-60 overflow-hidden rounded-xl border border-border bg-popover shadow-2xl",
                div { class: "flex items-center gap-2 border-b border-border px-3 py-2 text-[10px] text-muted-foreground",
                    Icon { icon: AppIcon::Command, size: 13 }
                    "Agent commands"
                    span { class: "ml-auto max-[520px]:hidden", "Enter to insert" }
                    span { class: "ml-auto hidden max-[520px]:inline", "Tap to insert" }
                }
                div { class: "max-h-[min(16rem,35dvh)] overflow-y-auto p-1.5",
                    for (index, command) in commands.into_iter().enumerate() {
                        SlashCommandRow {
                            key: "{command.name}",
                            command,
                            draft,
                            active: index == selected,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SlashCommandRow(command: PiCommand, mut draft: Signal<String>, active: bool) -> Element {
    let insertion = format!("/{} ", command.name);
    rsx! {
        button {
            class: if active { "flex min-h-10 w-full items-center gap-3 rounded-lg bg-accent px-2.5 py-2 text-left max-[520px]:min-h-11" } else { "flex min-h-10 w-full items-center gap-3 rounded-lg px-2.5 py-2 text-left hover:bg-accent max-[520px]:min-h-11" },
            onclick: move |_| {
                draft.set(insertion.clone());
                crate::ai::agent_view::focus_ai_composer();
            },
            span { class: "grid size-6 shrink-0 place-items-center rounded-md bg-secondary font-mono text-[10px] text-primary",
                "/"
            }
            span { class: "min-w-0 flex-1",
                strong { class: "block truncate font-mono text-[11px]", "/{command.name}" }
                if let Some(argument_hint) = command.argument_hint.as_ref() {
                    small { class: "ml-1 font-mono text-[9px] text-muted-foreground",
                        "{argument_hint}"
                    }
                }
                if !command.description.is_empty() {
                    small { class: "block truncate text-[9px] text-muted-foreground",
                        "{command.description}"
                    }
                }
            }
            span { class: "shrink-0 rounded bg-secondary px-1.5 py-0.5 text-[8px] text-muted-foreground",
                if command.invocation.as_deref() == Some("interactive") {
                    "terminal"
                } else {
                    "{command.source}"
                }
            }
        }
    }
}

pub(super) fn matching_commands(commands: &[PiCommand], draft: &str) -> Vec<PiCommand> {
    let Some(query) = draft.strip_prefix('/') else {
        return Vec::new();
    };
    if query.chars().any(char::is_whitespace) {
        return Vec::new();
    }
    let query = query.to_ascii_lowercase();
    commands
        .iter()
        .filter(|command| {
            query.is_empty()
                || command.name.to_ascii_lowercase().contains(&query)
                || command.description.to_ascii_lowercase().contains(&query)
        })
        .take(10)
        .cloned()
        .collect()
}

pub(super) fn exact_command<'a>(commands: &'a [PiCommand], draft: &str) -> Option<&'a PiCommand> {
    let name = draft.trim().strip_prefix('/')?.split_whitespace().next()?;
    commands.iter().find(|command| command.name == name)
}
