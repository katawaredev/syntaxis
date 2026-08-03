use super::*;

#[component]
pub(super) fn UsageMenu(stats: Option<SessionStats>) -> Element {
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
