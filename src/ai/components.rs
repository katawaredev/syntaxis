use std::collections::BTreeMap;

use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use dioxus_primitives::popover::{PopoverContent, PopoverRoot, PopoverTrigger};
use syntaxis_agent::{AgentSnapshot, ModelSummary, SessionStats, ThinkingLevel};
use syntaxis_ui::prelude::{AppIcon, Icon, IconButton, MenuContent, MenuTrigger};

mod composer;
mod extension_dialog;
mod session_sidebar;
mod timeline;

pub(super) use composer::{load_images, AgentComposer, ComposerSubmission};
pub(super) use extension_dialog::ExtensionRequestDialog;
pub(super) use session_sidebar::AgentSessionSidebar;
pub(super) use timeline::AgentTimeline;

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
                class: if open() { "flex h-8 min-w-0 max-w-58 items-center gap-2 rounded-lg border border-primary/30 bg-accent px-2.5 text-left shadow-sm max-[590px]:max-w-34 max-[520px]:size-10 max-[520px]:max-w-none max-[520px]:justify-center max-[520px]:p-0" } else { "flex h-8 min-w-0 max-w-58 items-center gap-2 rounded-lg border border-input bg-background/80 px-2.5 text-left shadow-xs transition-colors hover:bg-accent max-[590px]:max-w-34 max-[520px]:size-10 max-[520px]:max-w-none max-[520px]:justify-center max-[520px]:p-0" },
                aria_label: "Choose Pi model",
                aria_expanded: open(),
                disabled: disabled || models.is_empty(),
                span { class: "grid size-5 shrink-0 place-items-center rounded-md bg-primary/10 text-primary",
                    ProviderMark { provider: selected_provider.clone(), size: 12 }
                }
                span { class: "min-w-0 flex-1 max-[520px]:hidden",
                    strong { class: "block truncate text-[11px] font-medium", "{selected_name}" }
                    small { class: "block truncate text-[9px] text-muted-foreground",
                        "{selected_provider}"
                    }
                }
                span { class: "max-[520px]:hidden",
                    Icon { icon: AppIcon::ChevronDown, size: 13 }
                }
            }
            PopoverContent { class: "touch-popover absolute top-[calc(100%+6px)] right-0 z-80 w-[min(430px,calc(100vw-1rem))] overflow-hidden rounded-xl border border-border bg-popover shadow-2xl",
                div { class: "flex items-center gap-2 border-b border-border px-3 py-2",
                    Icon { icon: AppIcon::Search, size: 14 }
                    input {
                        class: "h-8 min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-muted-foreground",
                        value: query(),
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
fn ThinkingPicker(
    selected: ThinkingLevel,
    disabled: bool,
    on_select: EventHandler<ThinkingLevel>,
) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        DropdownMenu {
            class: "relative",
            open: open(),
            disabled,
            on_open_change: move |next: bool| open.set(next),
            MenuTrigger {
                label: format!("Thinking level: {}", selected.as_str()),
                icon: AppIcon::BrainCog,
                class: "max-[520px]:size-10",
                open: open(),
                on_toggle: move |()| open.toggle(),
            }
            MenuContent { class: "right-0 w-44",
                for (index, level) in ThinkingLevel::ALL.into_iter().enumerate() {
                    DropdownMenuItem::<ThinkingLevel> {
                        value: level,
                        index,
                        on_select: move |next| on_select.call(next),
                        span { "{level.as_str()}" }
                        if level == selected {
                            Icon { icon: AppIcon::Check, size: 13 }
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
                class: if open() { "relative grid size-8 place-items-center rounded-lg bg-accent text-foreground max-[520px]:size-10" } else { "relative grid size-8 place-items-center rounded-lg text-muted-foreground hover:bg-accent hover:text-foreground max-[520px]:size-10" },
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
            PopoverContent { class: "touch-popover absolute top-[calc(100%+6px)] right-0 z-80 w-76 rounded-xl border border-border bg-popover p-3 shadow-2xl",
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
