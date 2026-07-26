use dioxus::prelude::*;
use syntaxis_agent::{AgentStatus, ChatItem, ItemStatus};
use syntaxis_ui::prelude::{AppIcon, Icon};

use crate::files::preview::render_markdown;

const INITIAL_RENDER_ITEMS: usize = 150;
const RENDER_PAGE_ITEMS: usize = 100;

#[component]
pub(crate) fn AgentTimeline(
    items: Vec<ChatItem>,
    status: AgentStatus,
    on_suggestion: EventHandler<String>,
) -> Element {
    let is_empty = items.is_empty();
    let mut visible_count = use_signal(|| INITIAL_RENDER_ITEMS);
    let hidden_count = items.len().saturating_sub(visible_count());
    let visible_items = items.into_iter().skip(hidden_count).collect::<Vec<_>>();
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
                    if hidden_count > 0 {
                        button {
                            class: "mx-auto rounded-lg border border-border bg-background px-3 py-2 text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground",
                            r#type: "button",
                            onclick: move |_| {
                                *visible_count.write() += RENDER_PAGE_ITEMS;
                            },
                            "Show {hidden_count.min(RENDER_PAGE_ITEMS)} earlier items"
                        }
                    }
                    for item in visible_items {
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
            args,
            details,
            args_truncated,
            details_truncated,
            status,
            ..
        } => {
            let tone = match status {
                ItemStatus::Failed => "text-destructive",
                ItemStatus::Running | ItemStatus::Streaming => "text-primary",
                ItemStatus::Complete | ItemStatus::Stopped => "text-success",
            };
            let rendered_output = matches!(status, ItemStatus::Complete | ItemStatus::Stopped)
                .then(|| render_markdown(&output));
            let line_changes = tool_line_changes(&output);
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
                        if let Some((added, removed)) = line_changes {
                            span { class: "shrink-0 font-mono text-[9px]",
                                if added > 0 {
                                    span { class: "text-success", "+{added}" }
                                }
                                if removed > 0 {
                                    span { class: "ml-1 text-destructive", "-{removed}" }
                                }
                            }
                        }
                        small { class: "shrink-0 text-[9px] capitalize text-muted-foreground",
                            "{status:?}"
                        }
                    }
                    if !output.is_empty() {
                        div { class: "max-h-80 overflow-auto border-t border-border bg-background px-3 py-2 text-[10px] leading-relaxed text-muted-foreground",
                            if let Some(rendered) = rendered_output {
                                div {
                                    class: "ai-markdown ai-tool-markdown",
                                    dangerous_inner_html: rendered,
                                }
                            } else {
                                pre { class: "font-mono whitespace-pre-wrap", "{output}" }
                            }
                        }
                    }
                    if let Some(args) = args {
                        details { class: "border-t border-border px-3 py-2",
                            summary { class: "cursor-pointer font-medium text-foreground",
                                if args_truncated {
                                    "Arguments (truncated)"
                                } else {
                                    "Arguments"
                                }
                            }
                            pre { class: "mt-2 max-h-60 overflow-auto font-mono text-[10px] whitespace-pre-wrap text-muted-foreground",
                                {pretty_json(&args)}
                            }
                        }
                    }
                    if let Some(structured_details) = details {
                        details { class: "border-t border-border px-3 py-2",
                            summary { class: "cursor-pointer font-medium text-foreground",
                                if details_truncated {
                                    "Details (truncated)"
                                } else {
                                    "Details"
                                }
                            }
                            pre { class: "mt-2 max-h-60 overflow-auto font-mono text-[10px] whitespace-pre-wrap text-muted-foreground",
                                {pretty_json(&structured_details)}
                            }
                        }
                    }
                }
            }
        }
        ChatItem::Custom {
            label,
            text,
            details,
            details_truncated,
            ..
        } => {
            let rendered = render_markdown(&text);
            rsx! {
                article { class: "rounded-lg border border-primary/20 bg-primary/5 px-3 py-2.5 text-[11px]",
                    header { class: "mb-1 font-mono text-[9px] font-semibold tracking-wide text-primary uppercase",
                        "{label}"
                    }
                    if !text.is_empty() {
                        div {
                            class: "ai-markdown",
                            dangerous_inner_html: rendered,
                        }
                    }
                    if let Some(structured_details) = details {
                        details { class: "mt-2 border-t border-border pt-2",
                            summary { class: "cursor-pointer text-muted-foreground",
                                if details_truncated {
                                    "Details (truncated)"
                                } else {
                                    "Details"
                                }
                            }
                            pre { class: "mt-2 max-h-72 overflow-auto font-mono text-[10px] whitespace-pre-wrap text-muted-foreground",
                                {pretty_json(&structured_details)}
                            }
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

fn pretty_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn tool_line_changes(output: &str) -> Option<(usize, usize)> {
    let mut added = 0;
    let mut removed = 0;
    for line in output.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            added += 1;
        } else if line.starts_with('-') {
            removed += 1;
        }
    }
    (added > 0 || removed > 0).then_some((added, removed))
}
