use std::collections::BTreeMap;

use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use dioxus_primitives::popover::{PopoverContent, PopoverRoot, PopoverTrigger};
use syntaxis_agent::{AgentSnapshot, ModelSummary, SessionStats, ThinkingLevel};
use syntaxis_ui::prelude::{
    AppIcon, BrandIcon, BrandMark, Icon, IconButton, MenuContent, MenuTrigger,
};

mod composer;
mod extension_dialog;
mod session_sidebar;
mod timeline;

pub(super) use composer::{load_images, AgentComposer, ComposerSubmission};
pub(super) use extension_dialog::ExtensionRequestDialog;
pub(super) use session_sidebar::AgentSessionSidebar;
pub(super) use timeline::AgentTimeline;

mod model_controls;
mod usage;

use model_controls::{ModelPicker, ThinkingPicker};
use usage::UsageMenu;

#[component]
pub(super) fn AgentHeader(
    workspace_name: String,
    connection: String,
    session_title: String,
    snapshot: AgentSnapshot,
    controls_disabled: bool,
    workspace_locked: bool,
    new_worktree_disabled_reason: Option<String>,
    sidebar_open: bool,
    on_toggle_sidebar: EventHandler<()>,
    on_open_sidebar: EventHandler<()>,
    on_new_worktree: EventHandler<()>,
    on_model: EventHandler<(String, String)>,
    on_thinking: EventHandler<ThinkingLevel>,
) -> Element {
    let connection_ready = connection == "Pi connected";
    let workspace_locked_reason = if !connection_ready {
        Some("Connect to Pi before changing workspace".to_owned())
    } else if workspace_locked {
        Some("Workspace cannot be changed after the chat starts".to_owned())
    } else {
        None
    };
    rsx! {
        header { class: "flex min-h-12 items-center gap-2 border-b border-border bg-background px-2.5 max-[520px]:gap-1.5 max-[520px]:px-2",
            div { class: "shrink-0 max-md:hidden",
                IconButton {
                    label: if sidebar_open { "Hide AI sidebar" } else { "Show AI sidebar" },
                    icon: AppIcon::Explorer,
                    pressed: sidebar_open,
                    onclick: move |_| on_toggle_sidebar.call(()),
                }
            }
            div { class: "hidden shrink-0 max-md:block",
                IconButton {
                    label: "Open AI sidebar",
                    icon: AppIcon::Explorer,
                    onclick: move |_| on_open_sidebar.call(()),
                }
            }
            div {
                class: "flex min-w-0 flex-1 items-center gap-2",
                title: "{workspace_name} · {connection}",
                span { class: if connection_ready { "size-1.5 shrink-0 rounded-full bg-success" } else { "size-1.5 shrink-0 rounded-full bg-warning" } }
                strong { class: "min-w-0 truncate text-xs", "{session_title}" }
            }
            div { class: "flex shrink-0 items-center gap-1",
                WorkspacePicker {
                    workspace_name: workspace_name.clone(),
                    locked_reason: workspace_locked_reason,
                    new_worktree_disabled_reason,
                    on_new_worktree,
                }
                ModelPicker {
                    selected: snapshot.model.clone(),
                    models: snapshot.models.clone(),
                    disabled: controls_disabled,
                    on_select: on_model,
                }
                select {
                    class: "h-8 rounded-lg border border-input bg-background px-2 text-[10px] text-foreground max-[520px]:hidden",
                    aria_label: "Thinking level",
                    disabled: controls_disabled,
                    value: snapshot.thinking_level.as_str(),
                    onchange: move |event| {
                        if let Some(level) = ThinkingLevel::ALL
                            .into_iter()
                            .find(|level| level.as_str() == event.value())
                        {
                            on_thinking.call(level);
                        }
                    },
                    for level in ThinkingLevel::ALL {
                        option { value: level.as_str(), "{level.as_str()}" }
                    }
                }
                div { class: "hidden max-[520px]:block",
                    ThinkingPicker {
                        selected: snapshot.thinking_level,
                        disabled: controls_disabled,
                        on_select: on_thinking,
                    }
                }
                UsageMenu { stats: snapshot.session_stats.clone() }
            }
        }
    }
}

#[component]
fn WorkspacePicker(
    workspace_name: String,
    locked_reason: Option<String>,
    new_worktree_disabled_reason: Option<String>,
    on_new_worktree: EventHandler<()>,
) -> Element {
    let mut open = use_signal(|| false);
    let title = locked_reason
        .clone()
        .unwrap_or_else(|| "Choose workspace".to_owned());
    rsx! {
        PopoverRoot {
            class: "relative min-w-0",
            is_modal: false,
            open: open(),
            on_open_change: move |next| open.set(next),
            PopoverTrigger {
                class: "flex h-8 max-w-44 items-center gap-1.5 rounded-lg px-2 text-[10px] text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40 max-[520px]:size-10 max-[520px]:max-w-none max-[520px]:justify-center max-[520px]:p-0",
                disabled: locked_reason.is_some(),
                title,
                aria_label: "Workspace: {workspace_name}",
                Icon { icon: AppIcon::Worktree, size: 13 }
                span { class: "truncate max-[520px]:hidden", "Current checkout" }
                span { class: "max-[520px]:hidden",
                    Icon { icon: AppIcon::ChevronDown, size: 11 }
                }
            }
            PopoverContent { class: "touch-popover absolute top-[calc(100%+6px)] left-0 z-80 w-52 rounded-xl border border-border bg-popover p-1.5 shadow-2xl",
                div { class: "px-2 py-1.5 text-[9px] font-semibold tracking-wider text-muted-foreground uppercase",
                    "Workspace"
                }
                button {
                    class: "flex min-h-9 w-full items-center gap-2 rounded-lg bg-accent/60 px-2.5 text-left text-xs",
                    disabled: true,
                    Icon { icon: AppIcon::Check, size: 13 }
                    span { class: "min-w-0 flex-1",
                        strong { class: "block truncate font-medium", "Current checkout" }
                        small { class: "block truncate text-[9px] text-muted-foreground",
                            "{workspace_name}"
                        }
                    }
                }
                button {
                    class: "mt-1 flex min-h-9 w-full items-center gap-2 rounded-lg px-2.5 text-left text-xs text-muted-foreground hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40",
                    disabled: new_worktree_disabled_reason.is_some(),
                    title: new_worktree_disabled_reason.clone().unwrap_or_default(),
                    onclick: move |_| {
                        open.set(false);
                        on_new_worktree.call(());
                    },
                    Icon { icon: AppIcon::Worktree, size: 13 }
                    "New worktree"
                }
                if let Some(reason) = new_worktree_disabled_reason.as_deref() {
                    p { class: "px-2.5 py-1.5 text-[9px] leading-relaxed text-muted-foreground",
                        "{reason}"
                    }
                }
            }
        }
    }
}
