use dioxus::prelude::*;
use syntaxis_agent::{AgentStatus, ChatItem, ItemStatus};
use syntaxis_ui::prelude::{AppIcon, Icon};

use crate::files::preview::render_markdown;

#[component]
pub(crate) fn AgentTimeline(
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
