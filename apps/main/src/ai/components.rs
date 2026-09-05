use dioxus::prelude::*;
use dioxus_primitives::collapsible::{Collapsible, CollapsibleContent, CollapsibleTrigger};
use syntaxis_agent::{AgentSnapshot, ModelSummary, SessionStats, ThinkingLevel};
use syntaxis_ui::prelude::{AiChatHeader, AppIcon, Icon, InteractivePopover, ProviderIcon};
mod composer;
mod extension_dialog;
mod session_sidebar;
mod timeline;
pub(super) use composer::{AgentComposer, ComposerSubmission, load_images};
pub(super) use extension_dialog::ExtensionRequestDialog;
pub(super) use session_sidebar::AgentSessionSidebar;
pub(super) use timeline::AgentTimeline;
mod model_controls;
mod usage;
use model_controls::ModelPicker;
use usage::UsageMenu;
#[component]
pub(super) fn AgentHeader(
    workspace_id: String,
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
    on_model: EventHandler<(String, String, ThinkingLevel)>,
    on_thinking: EventHandler<ThinkingLevel>,
    on_compact: EventHandler<()>,
) -> Element {
    let connection_ready = connection == "Pi connected";
    let workspace_locked_reason = if !connection_ready {
        Some("Connect to the agent before changing workspace".to_owned())
    } else if workspace_locked {
        Some("Workspace cannot be changed after the chat starts".to_owned())
    } else {
        None
    };
    rsx! {
        AiChatHeader {
            title: session_title,
            connected: connection_ready,
            sidebar_open,
            on_toggle_sidebar,
            on_open_sidebar,
            actions: rsx! {
                WorkspacePicker {
                    workspace_name: workspace_name.clone(),
                    locked_reason: workspace_locked_reason,
                    new_worktree_disabled_reason,
                    on_new_worktree,
                }
                ModelPicker {
                    workspace_id,
                    selected: snapshot.model.clone(),
                    models: snapshot.models.clone(),
                    thinking_level: snapshot.thinking_level,
                    disabled: controls_disabled,
                    on_select: on_model,
                    on_thinking,
                }
                UsageMenu {
                    stats: snapshot.session_stats.clone(),
                    statuses: snapshot.extension_statuses.clone(),
                    compact_disabled: controls_disabled || snapshot.session_stats.is_none(),
                    on_compact,
                }
            },
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
        InteractivePopover {
            id: "ai-workspace-picker",
            label: format!("Workspace: {workspace_name}"),
            title,
            class: "min-w-0",
            open: open(),
            on_open_change: move |next| open.set(next),
            disabled: locked_reason
                                                                                                                                                                                            .is_some(),
            trigger_class: "flex h-8 max-w-44 items-center gap-1.5 rounded-lg px-2 text-[10px] text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40 max-[520px]:size-10 max-[520px]:max-w-none max-[520px]:justify-center max-[520px]:p-0",
            content_class: "absolute top-[calc(100%+6px)] left-0 z-80 w-52 rounded-xl border border-border bg-popover p-1.5 shadow-2xl",
            trigger: rsx! {
                Icon { icon: AppIcon::Worktree, size: 13 }
                span { class: "truncate max-[520px]:hidden", "Current checkout" }
                span { class: "max-[520px]:hidden",
                    Icon { icon: AppIcon::ChevronDown, size: 11 }
                }
            },
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
