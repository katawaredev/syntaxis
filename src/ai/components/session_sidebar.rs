use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use syntaxis_agent::{
    AgentSessionSummary, AgentStatus, ConversationMatchRole, ConversationSearchResult,
};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, Icon, IconButton, MenuButtonTrigger, MenuContent,
};

use crate::ai::api;

#[component]
pub(crate) fn AgentSessionSidebar(
    workspace_id: String,
    sessions: Vec<AgentSessionSummary>,
    selected_id: Option<String>,
    connected: bool,
    on_select: EventHandler<String>,
    on_new: EventHandler<()>,
    on_delete: EventHandler<String>,
) -> Element {
    let mut query = use_signal(String::new);
    let search_workspace_id = workspace_id.clone();
    let search_results = use_resource(move || {
        let workspace_id = search_workspace_id.clone();
        let resource_query = query().trim().to_owned();
        async move {
            if resource_query.chars().count() < 2 {
                return (resource_query, Ok(Vec::new()));
            }
            dioxus_sdk_time::sleep(std::time::Duration::from_millis(300)).await;
            let result = api::search_conversations(workspace_id, resource_query.clone()).await;
            (resource_query, result)
        }
    });
    let active_query = query().trim().to_owned();
    rsx! {
        nav {
            class: "flex h-full min-h-0 flex-col bg-sidebar",
            aria_label: "Pi chats",
            div { class: "flex min-h-12 items-center gap-1 border-b border-border px-2",
                div { class: "flex min-w-0 flex-1 items-center gap-2 rounded-md border border-input bg-background/70 px-2 focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/35",
                    Icon { icon: AppIcon::Search, size: 14 }
                    input {
                        class: "h-8 min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-muted-foreground",
                        r#type: "search",
                        value: query(),
                        placeholder: "Search conversations…",
                        aria_label: "Search conversations",
                        maxlength: 200,
                        oninput: move |event| query.set(event.value()),
                        onkeydown: move |event| {
                            if event.key() == Key::Escape {
                                query.set(String::new());
                            }
                        },
                    }
                    if !query().is_empty() {
                        button {
                            class: "grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground",
                            r#type: "button",
                            aria_label: "Clear conversation search",
                            title: "Clear search",
                            onclick: move |_| query.set(String::new()),
                            Icon { icon: AppIcon::Close, size: 12 }
                        }
                    }
                }
                IconButton {
                    label: "New chat",
                    icon: AppIcon::NewChat,
                    disabled: !connected,
                    onclick: move |_| on_new.call(()),
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto p-2",
                if active_query.chars().count() >= 2 {
                    match search_results() {
                        None => rsx! {
                            ConversationSearchState { message: "Searching…" }
                        },
                        Some((resource_query, _)) if resource_query != active_query => rsx! {
                            ConversationSearchState { message: "Searching…" }
                        },
                        Some((_, Err(error))) => rsx! {
                            ConversationSearchState { destructive: true, message: format!("Search failed: {error}") }
                        },
                        Some((_, Ok(results))) if results.is_empty() => rsx! {
                            ConversationSearchState { message: "No conversations match." }
                        },
                        Some((_, Ok(results))) => rsx! {
                            div { class: "mb-1 px-2 py-1 text-[9px] font-semibold tracking-wider text-muted-foreground uppercase",
                                "Message matches"
                            }
                            ul { class: "space-y-1",
                                for result in results {
                                    ConversationSearchRow {
                                        key: "{result.session_id}",
                                        result,
                                        query: active_query.clone(),
                                        connected,
                                        on_select: move |session_id| {
                                            query.set(String::new());
                                            on_select.call(session_id);
                                        },
                                    }
                                }
                            }
                        },
                    }
                } else if sessions.is_empty() {
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
        }
    }
}

#[component]
fn ConversationSearchState(message: String, #[props(default)] destructive: bool) -> Element {
    rsx! {
        div { class: if destructive { "px-3 py-8 text-center text-[11px] leading-relaxed text-destructive" } else { "px-3 py-8 text-center text-[11px] leading-relaxed text-muted-foreground" },
            "{message}"
        }
    }
}

#[component]
fn ConversationSearchRow(
    result: ConversationSearchResult,
    query: String,
    connected: bool,
    on_select: EventHandler<String>,
) -> Element {
    let session_id = result.session_id.clone();
    let role = match result.role {
        ConversationMatchRole::User => "You",
        ConversationMatchRole::Assistant => "Pi",
    };
    let matches = if result.match_count == 1 {
        "1 match".to_owned()
    } else {
        format!("{} matches", result.match_count)
    };
    rsx! {
        li {
            button {
                class: "w-full rounded-lg border border-transparent px-2.5 py-2.5 text-left hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50",
                disabled: !connected,
                onclick: move |_| on_select.call(session_id.clone()),
                strong { class: "block truncate text-[11px] font-medium", "{result.title}" }
                p { class: "mt-1 line-clamp-2 text-[10px] leading-relaxed text-muted-foreground",
                    span { class: "font-medium text-foreground/80", "{role}: " }
                    HighlightedConversationSnippet { text: result.snippet, query }
                }
                div { class: "mt-1 flex items-center gap-2 text-[9px] text-muted-foreground",
                    span { class: "min-w-0 flex-1", "{matches}" }
                    time { class: "shrink-0", {session_age(result.updated_at_ms)} }
                }
            }
        }
    }
}

#[component]
fn HighlightedConversationSnippet(text: String, query: String) -> Element {
    let parts = highlighted_parts(&text, &query);
    rsx! {
        for (index, (part, matched)) in parts.into_iter().enumerate() {
            if matched {
                mark {
                    key: "search-match-{index}",
                    class: "rounded-sm bg-warning/25 px-0.5 text-foreground",
                    "{part}"
                }
            } else {
                span { key: "search-text-{index}", "{part}" }
            }
        }
    }
}

fn highlighted_parts(text: &str, query: &str) -> Vec<(String, bool)> {
    let Ok(pattern) = regex::RegexBuilder::new(&regex::escape(query))
        .case_insensitive(true)
        .build()
    else {
        return vec![(text.to_owned(), false)];
    };
    let mut parts = Vec::new();
    let mut previous = 0;
    for found in pattern.find_iter(text) {
        if found.start() > previous {
            parts.push((text[previous..found.start()].to_owned(), false));
        }
        parts.push((found.as_str().to_owned(), true));
        previous = found.end();
    }
    if previous < text.len() {
        parts.push((text[previous..].to_owned(), false));
    }
    if parts.is_empty() {
        parts.push((text.to_owned(), false));
    }
    parts
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
                MenuButtonTrigger {
                    class: if menu_open() { "grid size-7 place-items-center rounded-md bg-accent text-foreground" } else { "grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-background/70 hover:text-foreground" },
                    label: "Chat actions for {session.title}",
                    title: "Chat actions",
                    on_toggle: move |()| menu_open.toggle(),
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
