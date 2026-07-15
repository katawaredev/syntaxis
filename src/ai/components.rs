use dioxus::prelude::*;
use syntaxis_agent::{
    AgentSessionSummary, AgentSnapshot, AgentStatus, ChatItem, ExtensionUiRequest, ItemStatus,
    ThinkingLevel,
};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, DialogActions, DialogForm, Icon, IconButton, Modal,
};

use crate::files::preview::render_markdown;

#[component]
pub(super) fn AgentHeader(
    workspace_name: String,
    connection: String,
    session_title: String,
    snapshot: AgentSnapshot,
    controls_disabled: bool,
    new_session_disabled: bool,
    sidebar_open: bool,
    on_toggle_sidebar: EventHandler<()>,
    on_open_sidebar: EventHandler<()>,
    on_new_session: EventHandler<()>,
    on_model: EventHandler<(String, String)>,
    on_thinking: EventHandler<ThinkingLevel>,
) -> Element {
    let connection_ready = connection == "Pi connected";
    let model_key = snapshot
        .model
        .as_ref()
        .map(syntaxis_agent::ModelSummary::key);
    let models = snapshot.models;
    let models_empty = models.is_empty();
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
            div { class: "grid size-7.5 shrink-0 place-items-center rounded-lg bg-primary/12 text-primary",
                Icon { icon: AppIcon::Sparkles, size: 16 }
            }
            div { class: "min-w-0",
                div { class: "flex items-center gap-1.5",
                    strong { class: "max-w-38 truncate text-xs max-[520px]:max-w-24", "{session_title}" }
                    span { class: if connection_ready { "size-1.5 rounded-full bg-success" } else { "size-1.5 rounded-full bg-warning" } }
                }
                small { class: "block max-w-34 truncate text-[10px] text-muted-foreground",
                    "{workspace_name} · {connection}"
                }
            }
            div { class: "ml-auto flex min-w-0 items-center gap-1.5",
                select {
                    class: "h-8 min-w-0 max-w-50 rounded-md border border-input bg-background px-2 text-[11px] text-foreground max-[520px]:max-w-30",
                    aria_label: "Pi model",
                    disabled: controls_disabled || models_empty,
                    value: model_key,
                    onchange: move |event| {
                        if let Some((provider, model_id)) = event.value().split_once('\u{1f}') {
                            on_model.call((provider.to_owned(), model_id.to_owned()));
                        }
                    },
                    if models_empty {
                        option { value: "", "Default model" }
                    }
                    for model in models {
                        option { value: model.key(), "{model.name} · {model.provider}" }
                    }
                }
                select {
                    class: "h-8 rounded-md border border-input bg-background px-2 text-[11px] text-foreground max-[430px]:w-17",
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
                IconButton {
                    label: "New chat",
                    icon: AppIcon::NewChat,
                    disabled: new_session_disabled,
                    onclick: move |_| on_new_session.call(()),
                }
            }
        }
    }
}

#[component]
pub(super) fn AgentSessionSidebar(
    sessions: Vec<AgentSessionSummary>,
    selected_id: Option<String>,
    connected: bool,
    on_select: EventHandler<String>,
    on_new: EventHandler<()>,
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
                            {
                                let active = selected_id.as_deref() == Some(session.id.as_str());
                                let id = session.id.clone();
                                let status_tone = match session.status {
                                    AgentStatus::Working | AgentStatus::Compacting => "bg-primary animate-pulse",
                                    AgentStatus::Ready => "bg-success",
                                    AgentStatus::Starting => "bg-warning",
                                    AgentStatus::Failed => "bg-destructive",
                                    AgentStatus::Stopped => "bg-muted-foreground/50",
                                };
                                rsx! {
                                    li { key: "{session.id}",
                                        button {
                                            class: if active { "w-full rounded-lg border border-primary/25 bg-primary/10 px-2.5 py-2.5 text-left" } else { "w-full rounded-lg border border-transparent px-2.5 py-2.5 text-left hover:bg-accent" },
                                            aria_current: active.then_some("page"),
                                            onclick: move |_| on_select.call(id.clone()),
                                            div { class: "flex items-center gap-2",
                                                span { class: "size-1.5 shrink-0 rounded-full {status_tone}" }
                                                strong { class: "min-w-0 flex-1 truncate text-[11px] font-medium", "{session.title}" }
                                            }
                                            div { class: "mt-1 flex items-center gap-2 pl-3.5 text-[9px] text-muted-foreground",
                                                span { class: "min-w-0 flex-1 truncate", "{session.status_message}" }
                                                time { class: "shrink-0", {session_age(session.updated_at_ms)} }
                                            }
                                        }
                                    }
                                }
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
        ChatItem::User { text, .. } => rsx! {
            article { class: "ml-auto max-w-[88%] rounded-xl rounded-br-sm bg-primary px-3.5 py-2.5 text-[13px] leading-relaxed whitespace-pre-wrap text-primary-foreground shadow-sm",
                "{text}"
            }
        },
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
    connected: bool,
    working: bool,
    pending_messages: usize,
    on_send: EventHandler<String>,
    on_abort: EventHandler<()>,
) -> Element {
    let can_send = connected && !draft().trim().is_empty();
    let submit = move || {
        if can_send {
            on_send.call(draft());
        }
    };
    rsx! {
        footer { class: "border-t border-border bg-background px-2.5 pt-2 pb-[max(0.5rem,env(safe-area-inset-bottom))]",
            div { class: "mx-auto max-w-3xl rounded-xl border border-input bg-card shadow-sm focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/20",
                textarea {
                    class: "block max-h-38 min-h-12 w-full resize-none bg-transparent px-3.5 pt-3 pb-2 text-[13px] leading-relaxed text-foreground outline-none placeholder:text-muted-foreground/70",
                    rows: 2,
                    value: draft(),
                    disabled: !connected,
                    placeholder: if working { "Steer Pi while it works…" } else { "Ask Pi to change or inspect this project…" },
                    aria_label: "Message Pi",
                    oninput: move |event| draft.set(event.value()),
                    onkeydown: move |event: KeyboardEvent| {
                        if event.key() == Key::Enter && !event.modifiers().contains(Modifiers::SHIFT) {
                            event.prevent_default();
                            submit();
                        }
                    },
                }
                div { class: "flex min-h-9 items-center gap-2 px-2 pb-2",
                    span { class: "min-w-0 flex-1 truncate px-1 text-[10px] text-muted-foreground",
                        if working {
                            if pending_messages > 0 {
                                "Steer queued · {pending_messages} pending"
                            } else {
                                "Enter steers · Shift+Enter adds a line"
                            }
                        } else {
                            "Enter sends · Shift+Enter adds a line"
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
        }
    }
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
