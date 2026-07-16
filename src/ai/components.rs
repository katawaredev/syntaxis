use std::collections::BTreeMap;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem, DropdownMenuTrigger};
use dioxus_primitives::popover::{PopoverContent, PopoverRoot, PopoverTrigger};
use serde::Deserialize;
use syntaxis_agent::{
    AgentSessionSummary, AgentSnapshot, AgentStatus, ChatItem, ExtensionUiRequest, ImageAttachment,
    ItemStatus, ModelSummary, PiCommand, SessionStats, ThinkingLevel, MAX_IMAGE_BYTES,
    MAX_PROMPT_IMAGES, MAX_TOTAL_IMAGE_BYTES,
};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, DialogActions, DialogForm, Icon, IconButton, MenuContent, Modal,
};

use crate::files::preview::render_markdown;

#[derive(Clone)]
pub(super) struct ComposerSubmission {
    pub(super) text: String,
    pub(super) images: Vec<ImageAttachment>,
}

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
        header { class: "flex min-h-12 items-center gap-2 border-b border-border bg-background px-2.5",
            div { class: "shrink-0 max-md:hidden",
                IconButton {
                    label: if sidebar_open { "Hide chats" } else { "Show chats" },
                    icon: AppIcon::Explorer,
                    pressed: sidebar_open,
                    onclick: move |_| on_toggle_sidebar.call(()),
                }
            }
            div { class: "hidden shrink-0 max-md:block",
                IconButton {
                    label: "Open chats",
                    icon: AppIcon::Explorer,
                    onclick: move |_| on_open_sidebar.call(()),
                }
            }
            div {
                class: "flex min-w-0 items-center gap-2",
                title: "{workspace_name} · {connection}",
                span { class: if connection_ready { "size-1.5 shrink-0 rounded-full bg-success" } else { "size-1.5 shrink-0 rounded-full bg-warning" } }
                strong { class: "max-w-42 truncate text-xs max-[520px]:max-w-24", "{session_title}" }
            }
            WorkspacePicker {
                workspace_name: workspace_name.clone(),
                locked_reason: workspace_locked_reason,
                new_worktree_disabled_reason,
                on_new_worktree,
            }
            div { class: "ml-auto flex min-w-0 items-center gap-1",
                ModelPicker {
                    selected: snapshot.model.clone(),
                    models: snapshot.models.clone(),
                    disabled: controls_disabled,
                    on_select: on_model,
                }
                select {
                    class: "h-8 rounded-lg border border-input bg-background px-2 text-[10px] text-foreground max-[430px]:w-16",
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
                UsageMenu { stats: snapshot.session_stats.clone() }
            }
        }
    }
}

#[component]
fn ModelPicker(
    selected: Option<ModelSummary>,
    models: Vec<ModelSummary>,
    disabled: bool,
    on_select: EventHandler<(String, String)>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut query = use_signal(String::new);
    let selected_key = selected.as_ref().map(ModelSummary::key);
    let selected_name = selected
        .as_ref()
        .map_or_else(|| "Default model".to_owned(), |model| model.name.clone());
    let selected_provider = selected
        .as_ref()
        .map_or_else(|| "Pi".to_owned(), |model| model.provider.clone());
    let groups = group_models(models.clone(), &query());
    rsx! {
        PopoverRoot {
            class: "relative min-w-0",
            is_modal: false,
            open: open(),
            on_open_change: move |next| {
                open.set(next);
                if next {
                    query.set(String::new());
                }
            },
            PopoverTrigger {
                class: if open() { "flex h-8 min-w-0 max-w-58 items-center gap-2 rounded-lg border border-primary/30 bg-accent px-2.5 text-left shadow-sm max-[590px]:max-w-34" } else { "flex h-8 min-w-0 max-w-58 items-center gap-2 rounded-lg border border-input bg-background/80 px-2.5 text-left shadow-xs transition-colors hover:bg-accent max-[590px]:max-w-34" },
                aria_label: "Choose Pi model",
                aria_expanded: open(),
                disabled: disabled || models.is_empty(),
                span { class: "grid size-5 shrink-0 place-items-center rounded-md bg-primary/10 text-primary",
                    ProviderMark { provider: selected_provider.clone(), size: 12 }
                }
                span { class: "min-w-0 flex-1",
                    strong { class: "block truncate text-[11px] font-medium", "{selected_name}" }
                    small { class: "block truncate text-[9px] text-muted-foreground",
                        "{selected_provider}"
                    }
                }
                Icon { icon: AppIcon::ChevronDown, size: 13 }
            }
            PopoverContent { class: "absolute top-[calc(100%+6px)] right-0 z-80 w-[min(430px,calc(100vw-1rem))] overflow-hidden rounded-xl border border-border bg-popover shadow-2xl",
                div { class: "flex items-center gap-2 border-b border-border px-3 py-2",
                    Icon { icon: AppIcon::Search, size: 14 }
                    input {
                        class: "h-8 min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-muted-foreground",
                        value: query(),
                        autofocus: true,
                        placeholder: "Search models or providers…",
                        aria_label: "Search Pi models",
                        oninput: move |event| query.set(event.value()),
                    }
                }
                div { class: "max-h-[min(420px,70vh)] overflow-y-auto p-1.5",
                    if groups.is_empty() {
                        p { class: "px-3 py-8 text-center text-xs text-muted-foreground",
                            "No matching models"
                        }
                    }
                    for (provider, provider_models) in groups {
                        ModelGroup {
                            key: "{provider}",
                            provider,
                            models: provider_models,
                            selected_key: selected_key.clone(),
                            on_select: move |selection| {
                                on_select.call(selection);
                                open.set(false);
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ModelGroup(
    provider: String,
    models: Vec<ModelSummary>,
    selected_key: Option<String>,
    on_select: EventHandler<(String, String)>,
) -> Element {
    rsx! {
        section { class: "not-last:mb-1.5",
            div { class: "sticky top-0 z-1 flex items-center gap-2 bg-popover/95 px-2 py-1.5 text-[9px] font-semibold tracking-wider text-muted-foreground uppercase backdrop-blur",
                span { class: "grid size-4 place-items-center text-foreground",
                    ProviderMark { provider: provider.clone(), size: 13 }
                }
                "{provider}"
                span { class: "ml-auto font-normal tracking-normal", "{models.len()}" }
            }
            for model in models {
                ModelRow {
                    key: "{model.key()}",
                    selected: selected_key.as_deref() == Some(model.key().as_str()),
                    model,
                    on_select,
                }
            }
        }
    }
}

#[component]
fn ModelRow(
    model: ModelSummary,
    selected: bool,
    on_select: EventHandler<(String, String)>,
) -> Element {
    rsx! {
        button {
            class: if selected { "grid min-h-10 w-full grid-cols-[minmax(0,1fr)_7rem_1rem] items-center gap-2 rounded-lg bg-primary/10 px-2.5 py-1.5 text-left text-xs text-foreground" } else { "grid min-h-10 w-full grid-cols-[minmax(0,1fr)_7rem_1rem] items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-xs text-muted-foreground hover:bg-accent hover:text-foreground" },
            onclick: move |_| on_select.call((model.provider.clone(), model.id.clone())),
            span { class: "min-w-0",
                strong { class: "block truncate font-medium", "{model.name}" }
                if model.name != model.id {
                    small { class: "block truncate font-mono text-[9px] text-muted-foreground",
                        "{model.id}"
                    }
                }
            }
            span { class: "flex min-w-0 justify-end gap-1 overflow-hidden",
                if model.supports_images {
                    span { class: "rounded bg-secondary px-1.5 py-0.5 text-[8px] text-muted-foreground",
                        "vision"
                    }
                }
                if model.reasoning {
                    span { class: "rounded bg-secondary px-1.5 py-0.5 text-[8px] text-muted-foreground",
                        "reasoning"
                    }
                }
            }
            span { class: "grid size-4 place-items-center",
                if selected {
                    Icon { icon: AppIcon::Check, size: 13 }
                }
            }
        }
    }
}

#[component]
fn ProviderMark(provider: String, size: u32) -> Element {
    let normalized = provider.to_ascii_lowercase();
    if normalized.contains("openai") || normalized.contains("codex") {
        rsx! {
            svg {
                width: size,
                height: size,
                view_box: "0 0 256 260",
                "aria-hidden": "true",
                fill: "currentColor",
                path {
                    d: "M239.184 106.203a64.716 64.716 0 0 0-5.576-53.103C219.452 28.459 191 15.784 163.213 21.74A65.586 65.586 0 0 0 52.096 45.22a64.716 64.716 0 0 0-43.23 31.36c-14.31 24.602-11.061 55.634 8.033 76.74a64.665 64.665 0 0 0 5.525 53.102c14.174 24.65 42.644 37.324 70.446 31.36a64.72 64.72 0 0 0 48.754 21.744c28.481.025 53.714-18.361 62.414-45.481a64.767 64.767 0 0 0 43.229-31.36c14.137-24.558 10.875-55.423-8.083-76.483Zm-97.56 136.338a48.397 48.397 0 0 1-31.105-11.255l53.205-30.695a8.595 8.595 0 0 0 4.247-7.367v-72.85l21.845 12.636v60.93c-.056 26.818-21.783 48.545-48.601 48.601Zm-104.466-44.61a48.345 48.345 0 0 1-5.781-32.589l53.256 30.747a8.339 8.339 0 0 0 8.441 0l63.181-36.425v25.221l-52.693 30.849c-23.257 13.398-52.97 5.431-66.404-17.803ZM23.549 85.38a48.499 48.499 0 0 1 25.58-21.333v61.39a8.288 8.288 0 0 0 4.195 7.316l62.874 36.272-21.845 12.636-53-30.131c-23.211-13.454-31.171-43.144-17.804-66.405Zm179.466 41.695-63.08-36.63 21.795-12.585 53.001 30.184a48.6 48.6 0 0 1-7.316 87.635v-61.391a8.544 8.544 0 0 0-4.4-7.213Zm21.742-32.69-53.154-31.003a8.39 8.39 0 0 0-8.492 0L99.98 99.808V74.587l52.54-30.798a48.652 48.652 0 0 1 72.236 50.391ZM88.061 139.097l-21.845-12.585V65.685a48.652 48.652 0 0 1 79.757-37.346l-53.615 30.695a8.595 8.595 0 0 0-4.246 7.367l-.051 72.697Zm11.868-25.58 28.138-16.217 28.188 16.218v32.434l-28.086 16.218-28.188-16.218-.052-32.434Z",
                }
            }
        }
    } else if normalized.contains("google") || normalized.contains("gemini") {
        rsx! {
            Icon { icon: AppIcon::Sparkles, size }
        }
    } else if normalized.contains("anthropic") || normalized.contains("claude") {
        rsx! {
            span { class: "font-serif text-[1em] font-bold", "A" }
        }
    } else {
        rsx! {
            Icon { icon: AppIcon::Bot, size }
        }
    }
}

fn group_models(models: Vec<ModelSummary>, query: &str) -> Vec<(String, Vec<ModelSummary>)> {
    let query = query.trim().to_ascii_lowercase();
    let mut groups = BTreeMap::<String, Vec<ModelSummary>>::new();
    for model in models {
        let searchable =
            format!("{} {} {}", model.provider, model.name, model.id).to_ascii_lowercase();
        if query.is_empty() || searchable.contains(&query) {
            groups
                .entry(model.provider.clone())
                .or_default()
                .push(model);
        }
    }
    groups
        .into_iter()
        .map(|(provider, mut models)| {
            models.sort_by_key(|model| model.name.to_lowercase());
            (provider, models)
        })
        .collect()
}

#[component]
fn UsageMenu(stats: Option<SessionStats>) -> Element {
    let mut open = use_signal(|| false);
    let percent = stats
        .as_ref()
        .and_then(|stats| stats.context_percent)
        .unwrap_or_default();
    let gauge_color = usage_color(percent);
    let gauge_style = format!(
        "background: conic-gradient({gauge_color} {}%, var(--muted) 0)",
        percent.min(100)
    );
    rsx! {
        PopoverRoot {
            class: "relative shrink-0",
            is_modal: false,
            open: open(),
            on_open_change: move |next| open.set(next),
            PopoverTrigger {
                class: if open() { "relative grid size-8 place-items-center rounded-lg bg-accent text-foreground" } else { "relative grid size-8 place-items-center rounded-lg text-muted-foreground hover:bg-accent hover:text-foreground" },
                aria_label: "Session usage",
                aria_expanded: open(),
                title: "Session usage · {percent}% context",
                span {
                    class: "relative grid size-6 place-items-center rounded-full",
                    style: gauge_style,
                    span { class: "grid size-4.5 place-items-center rounded-full bg-background",
                        Icon { icon: AppIcon::Usage, size: 11 }
                    }
                }
            }
            PopoverContent { class: "absolute top-[calc(100%+6px)] right-0 z-80 w-76 rounded-xl border border-border bg-popover p-3 shadow-2xl",
                UsagePopover { stats }
            }
        }
    }
}

#[component]
fn UsagePopover(stats: Option<SessionStats>) -> Element {
    rsx! {
        div { class: "mb-3 flex items-center gap-2",
            div { class: "grid size-7 place-items-center rounded-lg bg-primary/10 text-primary",
                Icon { icon: AppIcon::Usage, size: 14 }
            }
            strong { class: "text-xs", "Session usage" }
        }
        if let Some(stats) = stats {
            ContextUsage { stats: stats.clone() }
            dl { class: "mt-2 grid grid-cols-2 gap-1.5 text-[10px]",
                UsageStat {
                    label: "Session tokens",
                    value: compact_number(stats.tokens.total),
                }
                UsageStat {
                    label: "Estimated cost",
                    value: format_cost(stats.cost_microusd),
                }
                UsageStat {
                    label: "Messages",
                    value: stats.total_messages.to_string(),
                }
                UsageStat { label: "Tool calls", value: stats.tool_calls.to_string() }
            }
        } else {
            p { class: "rounded-lg bg-background/60 px-3 py-5 text-center text-[10px] text-muted-foreground",
                "Usage appears after the first response."
            }
        }
    }
}

#[component]
fn ContextUsage(stats: SessionStats) -> Element {
    let percent = stats.context_percent.unwrap_or_default();
    let label = match (stats.context_tokens, stats.context_window) {
        (Some(tokens), Some(window)) => {
            format!(
                "{} of {} tokens",
                compact_number(tokens),
                compact_number(window)
            )
        }
        _ => "Waiting for context data".to_owned(),
    };
    rsx! {
        div { class: "rounded-lg border border-border bg-background/60 p-2.5",
            div { class: "flex items-center justify-between text-[10px]",
                span { class: "text-muted-foreground", "Context window" }
                strong { "{percent}%" }
            }
            div { class: "mt-2 h-1.5 overflow-hidden rounded-full bg-muted",
                div {
                    class: usage_bar_class(percent),
                    style: "width: {percent}%",
                }
            }
            small { class: "mt-1.5 block text-[9px] text-muted-foreground", "{label}" }
        }
    }
}

#[component]
fn UsageStat(label: String, value: String) -> Element {
    rsx! {
        div { class: "rounded-lg bg-background/60 px-2.5 py-2",
            dt { class: "text-[9px] text-muted-foreground", "{label}" }
            dd { class: "mt-0.5 font-semibold", "{value}" }
        }
    }
}

fn usage_color(percent: u8) -> &'static str {
    match percent {
        85.. => "var(--destructive)",
        65.. => "var(--warning)",
        _ => "var(--primary)",
    }
}

fn usage_bar_class(percent: u8) -> &'static str {
    match percent {
        85.. => "h-full rounded-full bg-destructive",
        65.. => "h-full rounded-full bg-warning",
        _ => "h-full rounded-full bg-primary",
    }
}

fn compact_number(value: u64) -> String {
    match value {
        1_000_000.. => format!("{}.{}M", value / 1_000_000, (value % 1_000_000) / 100_000),
        1_000.. => format!("{}.{}k", value / 1_000, (value % 1_000) / 100),
        _ => value.to_string(),
    }
}

fn format_cost(microusd: u64) -> String {
    format!(
        "${}.{:04}",
        microusd / 1_000_000,
        (microusd % 1_000_000) / 100
    )
}

#[component]
pub(super) fn AgentSessionSidebar(
    sessions: Vec<AgentSessionSummary>,
    selected_id: Option<String>,
    connected: bool,
    on_select: EventHandler<String>,
    on_new: EventHandler<()>,
    on_delete: EventHandler<String>,
) -> Element {
    rsx! {
        nav {
            class: "flex h-full min-h-0 flex-col bg-sidebar",
            aria_label: "Pi chats",
            div { class: "flex min-h-12 items-center border-b border-border px-3",
                div { class: "min-w-0 flex-1",
                    strong { class: "block text-xs font-semibold", "Chats" }
                    small { class: "block text-[10px] text-muted-foreground", "This project" }
                }
                IconButton {
                    label: "New chat",
                    icon: AppIcon::NewChat,
                    disabled: !connected,
                    onclick: move |_| on_new.call(()),
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto p-2",
                if sessions.is_empty() {
                    div { class: "flex h-full flex-col items-center justify-center px-4 text-center",
                        div { class: "grid size-9 place-items-center rounded-xl bg-secondary text-primary",
                            Icon { icon: AppIcon::Sparkles, size: 17 }
                        }
                        p { class: "mt-3 text-xs font-medium", "No chats yet" }
                        p { class: "mt-1 text-[10px] leading-relaxed text-muted-foreground",
                            "Start a chat and Pi will keep it here for this project."
                        }
                        Button {
                            label: "New chat",
                            kind: ButtonKind::Ghost,
                            disabled: !connected,
                            onclick: move |_| on_new.call(()),
                        }
                    }
                } else {
                    div { class: "mb-1 px-2 py-1 text-[9px] font-semibold tracking-wider text-muted-foreground uppercase",
                        "Recent"
                    }
                    ul { class: "space-y-1",
                        for session in sessions {
                            AgentSessionRow {
                                key: "{session.id}",
                                active: selected_id.as_deref() == Some(session.id.as_str()),
                                session,
                                connected,
                                on_select,
                                on_delete,
                            }
                        }
                    }
                }
            }
            div { class: "border-t border-border px-3 py-2 text-[9px] leading-relaxed text-muted-foreground",
                "Chats keep running on the host when you leave this screen."
            }
        }
    }
}

#[component]
fn AgentSessionRow(
    session: AgentSessionSummary,
    active: bool,
    connected: bool,
    on_select: EventHandler<String>,
    on_delete: EventHandler<String>,
) -> Element {
    let mut menu_open = use_signal(|| false);
    let id = session.id.clone();
    let status_tone = match session.status {
        AgentStatus::Working | AgentStatus::Compacting => "bg-primary animate-pulse",
        AgentStatus::Ready => "bg-success",
        AgentStatus::Starting => "bg-warning",
        AgentStatus::Failed => "bg-destructive",
        AgentStatus::Stopped => "bg-muted-foreground/50",
    };
    let delete_id = session.id.clone();
    rsx! {
        li { class: if active { "group relative flex items-stretch rounded-lg border border-primary/25 bg-primary/10" } else { "group relative flex items-stretch rounded-lg border border-transparent hover:bg-accent" },
            button {
                class: "min-w-0 flex-1 px-2.5 py-2.5 text-left",
                aria_current: active.then_some("page"),
                onclick: move |_| on_select.call(id.clone()),
                div { class: "flex items-center gap-2",
                    span { class: "size-1.5 shrink-0 rounded-full {status_tone}" }
                    strong { class: "min-w-0 flex-1 truncate text-[11px] font-medium",
                        "{session.title}"
                    }
                }
                div { class: "mt-1 flex items-center gap-2 pl-3.5 text-[9px] text-muted-foreground",
                    span { class: "min-w-0 flex-1 truncate", "{session.status_message}" }
                    time { class: "shrink-0", {session_age(session.updated_at_ms)} }
                }
            }
            DropdownMenu {
                class: "relative flex shrink-0 items-center pr-1",
                open: menu_open(),
                on_open_change: move |open: bool| menu_open.set(open),
                DropdownMenuTrigger {
                    class: if menu_open() { "grid size-7 place-items-center rounded-md bg-accent text-foreground" } else { "grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-background/70 hover:text-foreground" },
                    aria_label: "Chat actions for {session.title}",
                    title: "Chat actions",
                    Icon { icon: AppIcon::MoreVertical, size: 15 }
                }
                MenuContent { class: "top-[calc(50%+16px)] right-0 w-44",
                    DropdownMenuItem::<String> {
                        value: delete_id.clone(),
                        index: 0_usize,
                        disabled: !connected,
                        class: "!text-destructive",
                        on_select: move |session_id| {
                            on_delete.call(session_id);
                            menu_open.set(false);
                        },
                        span { class: "flex items-center gap-2",
                            Icon { icon: AppIcon::Delete, size: 13 }
                            "Delete chat"
                        }
                    }
                }
            }
        }
    }
}

fn session_age(timestamp: u64) -> String {
    let now = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(timestamp);
    let minutes = now.saturating_sub(timestamp) / 60_000;
    match minutes {
        0 => "now".into(),
        1..=59 => format!("{minutes}m"),
        60..=1_439 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1_440),
    }
}

#[component]
pub(super) fn AgentTimeline(
    items: Vec<ChatItem>,
    status: AgentStatus,
    on_suggestion: EventHandler<String>,
) -> Element {
    let is_empty = items.is_empty();
    rsx! {
        div {
            class: "min-h-0 flex-1 overflow-y-auto overscroll-contain px-3 py-4 [scrollbar-gutter:stable] max-md:px-2.5",
            "data-agent-scroll": true,
            role: "log",
            "aria-live": "polite",
            if is_empty {
                div { class: "mx-auto flex min-h-full w-full max-w-2xl flex-col items-center justify-center px-3 py-8 text-center",
                    div { class: "grid size-12 place-items-center rounded-2xl border border-border bg-background text-primary shadow-sm",
                        Icon { icon: AppIcon::Sparkles, size: 23 }
                    }
                    h1 { class: "mt-4 text-lg font-semibold tracking-tight", "What should Pi work on?" }
                    p { class: "mt-1.5 max-w-sm text-xs leading-relaxed text-muted-foreground",
                        "Pi can inspect files, edit code, run commands, and verify the result in this workspace."
                    }
                    div { class: "mt-5 grid w-full max-w-md gap-2 sm:grid-cols-3",
                        for suggestion in ["Explain this project", "Find and fix a bug", "Run tests and resolve failures"] {
                            button {
                                class: "min-h-15 rounded-lg border border-border bg-background px-3 py-2 text-left text-[11px] leading-snug text-muted-foreground transition-colors hover:border-primary/40 hover:bg-accent hover:text-foreground",
                                onclick: move |_| on_suggestion.call(suggestion.into()),
                                "{suggestion}"
                            }
                        }
                    }
                }
            } else {
                div { class: "mx-auto flex w-full max-w-3xl flex-col gap-3 pb-2",
                    for item in items {
                        AgentTimelineItem { key: "{item.id()}", item }
                    }
                    if matches!(status, AgentStatus::Working | AgentStatus::Compacting) {
                        div { class: "flex items-center gap-2 px-1 py-1 text-[11px] text-muted-foreground",
                            span { class: "flex gap-1", aria_hidden: true,
                                span { class: "size-1.5 animate-pulse rounded-full bg-primary" }
                                span { class: "size-1.5 animate-pulse rounded-full bg-primary [animation-delay:150ms]" }
                                span { class: "size-1.5 animate-pulse rounded-full bg-primary [animation-delay:300ms]" }
                            }
                            if status == AgentStatus::Compacting {
                                "Compacting context…"
                            } else {
                                "Pi is working…"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AgentTimelineItem(item: ChatItem) -> Element {
    match item {
        ChatItem::User { text, images, .. } => {
            let rendered = render_markdown(&text);
            rsx! {
                article { class: "ml-auto max-w-[88%] rounded-xl rounded-br-sm border border-border bg-secondary px-3.5 py-2.5 text-[13px] leading-relaxed text-secondary-foreground shadow-sm",
                    if !images.is_empty() {
                        div { class: "mb-2 grid max-w-lg grid-cols-2 gap-1.5",
                            for image in images {
                                img {
                                    class: "max-h-52 min-h-20 w-full rounded-lg bg-black/10 object-cover",
                                    src: image.data_url(),
                                    alt: image.name,
                                }
                            }
                        }
                    }
                    if !text.is_empty() {
                        div {
                            class: "ai-markdown ai-user-markdown",
                            dangerous_inner_html: rendered,
                        }
                    }
                }
            }
        }
        ChatItem::Assistant {
            text,
            thinking,
            status,
            ..
        } => {
            let rendered = render_markdown(&text);
            rsx! {
                article { class: "max-w-full py-1 pr-2",
                    if !thinking.trim().is_empty() {
                        details { class: "mb-2 rounded-lg border border-border bg-background/60 text-[11px] text-muted-foreground",
                            summary { class: "cursor-pointer px-3 py-2 select-none", "Reasoning" }
                            div { class: "max-h-60 overflow-auto border-t border-border px-3 py-2 font-mono text-[10px] leading-relaxed whitespace-pre-wrap",
                                "{thinking}"
                            }
                        }
                    }
                    if text.is_empty() && status == ItemStatus::Streaming {
                        div { class: "h-4 w-32 animate-pulse rounded bg-muted" }
                    } else {
                        div {
                            class: "ai-markdown",
                            dangerous_inner_html: rendered,
                        }
                    }
                    if matches!(status, ItemStatus::Failed | ItemStatus::Stopped) {
                        small { class: "mt-1 block text-[10px] text-destructive",
                            if status == ItemStatus::Stopped {
                                "Stopped"
                            } else {
                                "Response failed"
                            }
                        }
                    }
                }
            }
        }
        ChatItem::Tool {
            name,
            summary,
            output,
            status,
            ..
        } => {
            let tone = match status {
                ItemStatus::Failed => "text-destructive",
                ItemStatus::Running | ItemStatus::Streaming => "text-primary",
                ItemStatus::Complete | ItemStatus::Stopped => "text-success",
            };
            rsx! {
                details { class: "rounded-lg border border-border bg-background/65 text-[11px]",
                    summary { class: "flex min-h-9 cursor-pointer list-none items-center gap-2 px-3 py-2 select-none [&::-webkit-details-marker]:hidden",
                        span { class: "size-2 shrink-0 rounded-full bg-current {tone}" }
                        strong { class: "font-mono text-[10px] font-semibold text-foreground",
                            "{name}"
                        }
                        span { class: "min-w-0 flex-1 truncate text-muted-foreground",
                            "{summary}"
                        }
                        small { class: "shrink-0 text-[9px] capitalize text-muted-foreground",
                            "{status:?}"
                        }
                    }
                    if !output.is_empty() {
                        pre { class: "max-h-70 overflow-auto border-t border-border bg-background px-3 py-2 font-mono text-[10px] leading-relaxed whitespace-pre-wrap text-muted-foreground",
                            "{output}"
                        }
                    }
                }
            }
        }
        ChatItem::Notice { text, status, .. } => rsx! {
            div { class: if status == ItemStatus::Failed { "rounded-lg border border-destructive/30 bg-destructive/8 px-3 py-2 text-[11px] text-destructive" } else { "rounded-lg border border-border bg-background px-3 py-2 text-[11px] text-muted-foreground" },
                "{text}"
            }
        },
    }
}

#[component]
pub(super) fn AgentComposer(
    mut draft: Signal<String>,
    mut attachments: Signal<Vec<ImageAttachment>>,
    mut composer_error: Signal<Option<String>>,
    connected: bool,
    working: bool,
    pending_messages: usize,
    commands: Vec<PiCommand>,
    accepts_images: bool,
    on_send: EventHandler<ComposerSubmission>,
    on_abort: EventHandler<()>,
) -> Element {
    let speech_active = use_speech_bridge(draft, composer_error);
    use_paste_bridge(attachments, composer_error);
    let images = attachments();
    let can_send = connected
        && (!draft().trim().is_empty() || !images.is_empty())
        && (images.is_empty() || accepts_images);
    let first_command = matching_commands(&commands, &draft()).first().cloned();
    let mut submit = move || {
        if can_send {
            on_send.call(ComposerSubmission {
                text: draft(),
                images: attachments(),
            });
            attachments.set(Vec::new());
        }
    };
    rsx! {
        footer { class: "bg-card px-2.5 pt-1 pb-[max(0.65rem,env(safe-area-inset-bottom))]",
            div { class: "relative mx-auto max-w-3xl",
                SlashCommandMenu { commands, draft }
                div { class: "overflow-hidden rounded-2xl border border-input bg-card shadow-[0_8px_30px_#0002] transition-[border,box-shadow] focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/20",
                    if !images.is_empty() {
                        ComposerAttachments {
                            images: images.clone(),
                            on_remove: move |index| {
                                attachments.write().remove(index);
                                composer_error.set(None);
                            },
                        }
                    }
                    div { class: "ai-composer-editor",
                        textarea {
                            id: "syntaxis-ai-composer",
                            class: "ai-composer-input",
                            rows: 3,
                            value: draft(),
                            disabled: !connected,
                            placeholder: if working { "Steer Pi while it works…" } else { "Ask Pi to change or inspect this project…" },
                            aria_label: "Message Pi",
                            "data-images-enabled": accepts_images && connected,
                            oninput: move |event| {
                                draft.set(event.value());
                                composer_error.set(None);
                            },
                            onkeydown: move |event: KeyboardEvent| {
                                if event.key() == Key::Enter && !event.modifiers().contains(Modifiers::SHIFT) {
                                    event.prevent_default();
                                    if let Some(command) = first_command.as_ref() {
                                        draft.set(format!("/{} ", command.name));
                                    } else {
                                        submit();
                                    }
                                }
                            },
                        }
                    }
                    div { class: "flex min-h-10 items-center gap-1 px-2 pb-2",
                        label {
                            class: if accepts_images && connected { "grid size-8 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" } else { "grid size-8 cursor-not-allowed place-items-center rounded-lg text-muted-foreground opacity-35" },
                            aria_label: if accepts_images { "Attach images" } else { "Selected model does not accept images" },
                            title: if accepts_images { "Attach images" } else { "Selected model does not accept images" },
                            input {
                                class: "hidden",
                                r#type: "file",
                                accept: "image/*",
                                multiple: true,
                                disabled: !accepts_images || !connected,
                                onchange: move |event: FormEvent| {
                                    spawn(load_images(event.files(), attachments, composer_error));
                                },
                            }
                            Icon { icon: AppIcon::Attachment, size: 15 }
                        }
                        IconButton {
                            label: if speech_active() { "Stop dictation" } else { "Dictate message" },
                            icon: AppIcon::Microphone,
                            pressed: speech_active(),
                            disabled: !connected,
                            onclick: move |_| toggle_speech(),
                        }
                        span { class: "min-w-0 flex-1 truncate px-1 text-[9px] text-muted-foreground max-[520px]:hidden",
                            if working {
                                if pending_messages > 0 {
                                    "Steer queued · {pending_messages} pending"
                                } else {
                                    "Enter steers · Shift+Enter adds a line"
                                }
                            } else {
                                "Markdown supported · Enter sends · Shift+Enter adds a line"
                            }
                        }
                        if working {
                            IconButton {
                                label: "Stop Pi",
                                icon: AppIcon::Stop,
                                danger: true,
                                onclick: move |_| on_abort.call(()),
                            }
                        }
                        button {
                            class: "grid size-8.5 place-items-center rounded-lg bg-primary text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-35",
                            disabled: !can_send,
                            aria_label: if working { "Steer Pi" } else { "Send message" },
                            title: if working { "Steer Pi" } else { "Send message" },
                            onclick: move |_| submit(),
                            Icon { icon: AppIcon::Send, size: 15 }
                        }
                    }
                }
                if !images.is_empty() && !accepts_images {
                    p { class: "px-2.5 pt-1.5 text-[10px] text-warning",
                        "Choose a vision-capable model to send these images."
                    }
                }
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
                class: "flex h-8 max-w-44 items-center gap-1.5 rounded-lg px-2 text-[10px] text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40",
                disabled: locked_reason.is_some(),
                title,
                aria_label: "Workspace: {workspace_name}",
                Icon { icon: AppIcon::Worktree, size: 13 }
                span { class: "truncate", "Current checkout" }
                Icon { icon: AppIcon::ChevronDown, size: 11 }
            }
            PopoverContent { class: "absolute top-[calc(100%+6px)] left-0 z-80 w-52 rounded-xl border border-border bg-popover p-1.5 shadow-2xl",
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

#[component]
fn SlashCommandMenu(commands: Vec<PiCommand>, draft: Signal<String>) -> Element {
    let matches = matching_commands(&commands, &draft());
    rsx! {
        if !matches.is_empty() {
            div { class: "absolute right-0 bottom-[calc(100%+7px)] left-0 z-60 overflow-hidden rounded-xl border border-border bg-popover shadow-2xl",
                div { class: "flex items-center gap-2 border-b border-border px-3 py-2 text-[10px] text-muted-foreground",
                    Icon { icon: AppIcon::Command, size: 13 }
                    "Pi commands"
                    span { class: "ml-auto", "Enter to insert" }
                }
                div { class: "max-h-64 overflow-y-auto p-1.5",
                    for command in matches {
                        SlashCommandRow { key: "{command.name}", command, draft }
                    }
                }
            }
        }
    }
}

#[component]
fn SlashCommandRow(command: PiCommand, mut draft: Signal<String>) -> Element {
    let insertion = format!("/{} ", command.name);
    rsx! {
        button {
            class: "flex min-h-10 w-full items-center gap-3 rounded-lg px-2.5 py-2 text-left hover:bg-accent",
            onclick: move |_| draft.set(insertion.clone()),
            span { class: "grid size-6 shrink-0 place-items-center rounded-md bg-secondary font-mono text-[10px] text-primary",
                "/"
            }
            span { class: "min-w-0 flex-1",
                strong { class: "block truncate font-mono text-[11px]", "/{command.name}" }
                if !command.description.is_empty() {
                    small { class: "block truncate text-[9px] text-muted-foreground",
                        "{command.description}"
                    }
                }
            }
            span { class: "shrink-0 rounded bg-secondary px-1.5 py-0.5 text-[8px] text-muted-foreground",
                "{command.source}"
            }
        }
    }
}

#[component]
fn ComposerAttachments(images: Vec<ImageAttachment>, on_remove: EventHandler<usize>) -> Element {
    rsx! {
        div { class: "flex gap-2 overflow-x-auto border-b border-border/70 px-3 pt-3 pb-2",
            for (index, image) in images.iter().enumerate() {
                AttachmentPreview {
                    key: "{index}-{image.name}",
                    image: image.clone(),
                    on_remove: move |()| on_remove.call(index),
                }
            }
        }
    }
}

#[component]
fn AttachmentPreview(image: ImageAttachment, on_remove: EventHandler<()>) -> Element {
    rsx! {
        div { class: "group relative size-18 shrink-0 overflow-hidden rounded-xl border border-border bg-background",
            img {
                class: "size-full object-cover",
                src: image.data_url(),
                alt: image.name.clone(),
            }
            button {
                class: "absolute top-1 right-1 grid size-5 place-items-center rounded-full bg-background/90 text-foreground opacity-0 shadow transition-opacity group-hover:opacity-100 focus-visible:opacity-100",
                aria_label: "Remove {image.name}",
                title: "Remove image",
                onclick: move |_| on_remove.call(()),
                Icon { icon: AppIcon::Close, size: 11 }
            }
            span { class: "absolute right-0 bottom-0 left-0 truncate bg-black/60 px-1.5 py-1 text-[8px] text-white",
                "{image.name}"
            }
        }
    }
}

pub(super) async fn load_images(
    files: Vec<dioxus::html::FileData>,
    mut attachments: Signal<Vec<ImageAttachment>>,
    mut error: Signal<Option<String>>,
) {
    for file in files {
        if attachments().len() >= MAX_PROMPT_IMAGES {
            error.set(Some(format!("Attach up to {MAX_PROMPT_IMAGES} images.")));
            break;
        }
        let mime_type = file.content_type().unwrap_or_default();
        if !mime_type.starts_with("image/") {
            error.set(Some(format!("{} is not an image.", file.name())));
            continue;
        }
        let total = attachments().iter().map(|image| image.size).sum::<u64>();
        if file.size() > MAX_IMAGE_BYTES
            || total.saturating_add(file.size()) > MAX_TOTAL_IMAGE_BYTES
        {
            error.set(Some("Images can be 8 MiB each and 16 MiB total.".into()));
            continue;
        }
        match file.read_bytes().await {
            Ok(bytes) => attachments.write().push(ImageAttachment {
                name: file.name(),
                mime_type,
                size: file.size(),
                data: BASE64.encode(bytes),
            }),
            Err(_) => error.set(Some(format!("Could not read {}.", file.name()))),
        }
    }
}

#[derive(Deserialize)]
struct PasteBridgeEvent {
    kind: String,
    name: Option<String>,
    mime_type: Option<String>,
    data: Option<String>,
    message: Option<String>,
}

fn use_paste_bridge(attachments: Signal<Vec<ImageAttachment>>, error: Signal<Option<String>>) {
    let mut bridge = use_signal(|| None::<dioxus::document::Eval>);
    use_effect(move || {
        let mut events = document::eval(
            r#"
            const id = await dioxus.recv();
            const listener = event => {
                if (event.detail?.id === id) dioxus.send(event.detail);
            };
            window.addEventListener("syntaxis-ai-paste", listener);
            await dioxus.recv();
            window.removeEventListener("syntaxis-ai-paste", listener);
            "#,
        );
        let _ = events.send("syntaxis-ai-composer");
        bridge.set(Some(events));
        spawn(async move {
            while let Ok(event) = events.recv::<PasteBridgeEvent>().await {
                apply_paste_event(event, attachments, error);
            }
        });
    });
    use_drop(move || {
        if let Some(events) = bridge() {
            let _ = events.send(true);
        }
    });
}

fn apply_paste_event(
    event: PasteBridgeEvent,
    mut attachments: Signal<Vec<ImageAttachment>>,
    mut error: Signal<Option<String>>,
) {
    if event.kind == "error" {
        error.set(event.message);
        return;
    }
    let Some(data) = event.data else {
        return;
    };
    let mime_type = event.mime_type.unwrap_or_default();
    if !mime_type.starts_with("image/") {
        return;
    }
    if attachments().len() >= MAX_PROMPT_IMAGES {
        error.set(Some(format!("Attach up to {MAX_PROMPT_IMAGES} images.")));
        return;
    }
    let max_encoded_size = usize::try_from(MAX_IMAGE_BYTES)
        .unwrap_or(usize::MAX)
        .saturating_mul(4)
        / 3
        + 4;
    if data.len() > max_encoded_size {
        error.set(Some("Images can be 8 MiB each and 16 MiB total.".into()));
        return;
    }
    let Ok(bytes) = BASE64.decode(&data) else {
        error.set(Some("Could not read the pasted image.".into()));
        return;
    };
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let total = attachments().iter().map(|image| image.size).sum::<u64>();
    if size > MAX_IMAGE_BYTES || total.saturating_add(size) > MAX_TOTAL_IMAGE_BYTES {
        error.set(Some("Images can be 8 MiB each and 16 MiB total.".into()));
        return;
    }
    attachments.write().push(ImageAttachment {
        name: event.name.unwrap_or_else(|| "Pasted image".into()),
        mime_type,
        size,
        data,
    });
    error.set(None);
}

#[derive(Deserialize)]
struct SpeechBridgeEvent {
    kind: String,
    text: Option<String>,
    message: Option<String>,
}

fn use_speech_bridge(draft: Signal<String>, error: Signal<Option<String>>) -> Signal<bool> {
    let active = use_signal(|| false);
    let mut bridge = use_signal(|| None::<dioxus::document::Eval>);
    use_effect(move || {
        let mut events = document::eval(
            r#"
            const id = await dioxus.recv();
            const listener = event => {
                if (event.detail?.id === id) dioxus.send(event.detail);
            };
            window.addEventListener("syntaxis-ai-speech", listener);
            await dioxus.recv();
            window.removeEventListener("syntaxis-ai-speech", listener);
            "#,
        );
        let _ = events.send("syntaxis-ai-composer");
        bridge.set(Some(events));
        spawn(async move {
            while let Ok(event) = events.recv::<SpeechBridgeEvent>().await {
                apply_speech_event(event, draft, active, error);
            }
        });
    });
    use_drop(move || {
        if let Some(events) = bridge() {
            let _ = events.send(true);
        }
    });
    active
}

fn apply_speech_event(
    event: SpeechBridgeEvent,
    mut draft: Signal<String>,
    mut active: Signal<bool>,
    mut error: Signal<Option<String>>,
) {
    match event.kind.as_str() {
        "start" => {
            active.set(true);
            error.set(None);
        }
        "end" => active.set(false),
        "transcript" => {
            if let Some(text) = event.text {
                let mut value = draft.write();
                if !value.is_empty() && !value.ends_with(char::is_whitespace) {
                    value.push(' ');
                }
                value.push_str(text.trim());
            }
        }
        "error" => {
            active.set(false);
            error.set(Some(event.message.unwrap_or_else(|| {
                "Speech recognition is unavailable in this browser.".into()
            })));
        }
        _ => {}
    }
}

fn toggle_speech() {
    let _ = document::eval(
        r#"
        window.SyntaxisAiChat?.toggleSpeech("syntaxis-ai-composer");
        "#,
    );
}

fn matching_commands(commands: &[PiCommand], draft: &str) -> Vec<PiCommand> {
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

#[component]
pub(super) fn ExtensionRequestDialog(
    request: ExtensionUiRequest,
    on_respond: EventHandler<(Option<String>, Option<bool>, bool)>,
) -> Element {
    let mut value = use_signal(|| request.prefill.clone().unwrap_or_default());
    let description = if request.message.is_empty() {
        "A Pi extension needs your input.".to_owned()
    } else {
        request.message.clone()
    };
    rsx! {
        Modal {
            title: request.title.clone(),
            description,
            on_close: move |()| on_respond.call((None, None, true)),
            DialogForm {
                if request.method == "select" {
                    div { class: "grid gap-2",
                        for option in request.options.clone() {
                            Button {
                                label: option.clone(),
                                kind: ButtonKind::Secondary,
                                onclick: move |_| on_respond.call((Some(option.clone()), None, false)),
                            }
                        }
                    }
                } else if request.method == "confirm" {
                    DialogActions {
                        Button {
                            label: "No",
                            kind: ButtonKind::Ghost,
                            onclick: move |_| on_respond.call((None, Some(false), false)),
                        }
                        Button {
                            label: "Yes",
                            kind: ButtonKind::Primary,
                            onclick: move |_| on_respond.call((None, Some(true), false)),
                        }
                    }
                } else {
                    textarea {
                        class: "min-h-28 w-full resize-y rounded-md border border-input bg-background p-3 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/20",
                        value: value(),
                        autofocus: true,
                        placeholder: request.placeholder.clone(),
                        oninput: move |event| value.set(event.value()),
                    }
                    DialogActions {
                        Button {
                            label: "Cancel",
                            kind: ButtonKind::Ghost,
                            onclick: move |_| on_respond.call((None, None, true)),
                        }
                        Button {
                            label: "Submit",
                            kind: ButtonKind::Primary,
                            disabled: value().trim().is_empty(),
                            onclick: move |_| on_respond.call((Some(value()), None, false)),
                        }
                    }
                }
            }
        }
    }
}
