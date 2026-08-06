use super::*;

#[component]
pub(super) fn UsageMenu(stats: Option<SessionStats>) -> Element {
    let mut open = use_signal(|| false);
    let percent = stats.as_ref().map_or(0, context_percent);
    let remaining = 100_u8.saturating_sub(percent);
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
                span { class: "relative grid size-6 place-items-center",
                    svg {
                        key: "usage-ring-{percent}",
                        class: "absolute inset-0 size-6 -rotate-90",
                        view_box: "0 0 24 24",
                        fill: "none",
                        "aria-hidden": "true",
                        circle {
                            class: "stroke-muted",
                            cx: "12",
                            cy: "12",
                            r: "9",
                            path_length: "100",
                            stroke_width: "3",
                        }
                        circle {
                            class: usage_ring_class(percent),
                            cx: "12",
                            cy: "12",
                            r: "9",
                            path_length: "100",
                            stroke_width: "3",
                            stroke_linecap: "round",
                            stroke_dasharray: "{percent} {remaining}",
                        }
                    }
                    span { class: "relative grid size-4.5 place-items-center rounded-full bg-background",
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
    let percent = context_percent(&stats);
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

fn context_percent(stats: &SessionStats) -> u8 {
    stats
        .context_percent
        .or_else(|| {
            let tokens = stats.context_tokens?;
            let window = stats.context_window?.max(1);
            Some(u8::try_from((tokens.saturating_mul(100) / window).min(100)).unwrap_or(100))
        })
        .unwrap_or_default()
        .min(100)
}

fn usage_ring_class(percent: u8) -> &'static str {
    match percent {
        85.. => "stroke-destructive",
        65.. => "stroke-warning",
        _ => "stroke-primary",
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
