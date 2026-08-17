use super::*;

#[component]
pub(super) fn ModelPicker(
    selected: Option<ModelSummary>,
    models: Vec<ModelSummary>,
    thinking_level: ThinkingLevel,
    disabled: bool,
    on_select: EventHandler<(String, String, ThinkingLevel)>,
    on_thinking: EventHandler<ThinkingLevel>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut choosing_model = use_signal(|| false);
    let mut query = use_signal(String::new);
    let selected_key = selected.as_ref().map(ModelSummary::key);
    let selected_name = selected
        .as_ref()
        .map_or_else(|| "Default model".to_owned(), |model| model.name.clone());
    let selected_provider = selected
        .as_ref()
        .map_or_else(|| "Agent".to_owned(), |model| model.provider.clone());
    let selected_detail = selected.as_ref().map_or_else(
        || "Agent".to_owned(),
        |model| {
            if model.reasoning {
                format!(
                    "{} · {} effort",
                    model.provider,
                    model.effective_thinking_level(thinking_level).label(),
                )
            } else {
                model.provider.clone()
            }
        },
    );
    let visible_models = filter_models(models.clone(), &query())
        .into_iter()
        .filter(|model| selected_key.as_deref() != Some(model.key().as_str()))
        .collect::<Vec<_>>();
    let model_groups = group_models_by_provider(visible_models);
    rsx! {
        PopoverRoot {
            class: "relative min-w-0",
            is_modal: false,
            open: open(),
            on_open_change: move |next| {
                let was_open = open();
                open.set(next);
                if next && !was_open {
                    choosing_model.set(false);
                    query.set(String::new());
                }
            },
            PopoverTrigger {
                class: if open() { "flex h-8 min-w-0 max-w-58 items-center gap-2 rounded-lg border border-primary/30 bg-accent px-2.5 text-left shadow-sm max-[590px]:max-w-34 max-[520px]:size-10 max-[520px]:max-w-none max-[520px]:justify-center max-[520px]:p-0" } else { "flex h-8 min-w-0 max-w-58 items-center gap-2 rounded-lg border border-input bg-background/80 px-2.5 text-left shadow-xs transition-colors hover:bg-accent max-[590px]:max-w-34 max-[520px]:size-10 max-[520px]:max-w-none max-[520px]:justify-center max-[520px]:p-0" },
                aria_label: "Choose agent model",
                aria_expanded: open(),
                disabled: disabled || models.is_empty(),
                span { class: "grid size-5 shrink-0 place-items-center rounded-md bg-primary/10 text-primary",
                    ProviderIcon { provider: selected_provider.clone(), size: 12 }
                }
                span { class: "min-w-0 flex-1 max-[520px]:hidden",
                    strong { class: "block truncate text-[11px] font-medium", "{selected_name}" }
                    small { class: "block truncate text-[9px] text-muted-foreground",
                        "{selected_detail}"
                    }
                }
                span { class: "max-[520px]:hidden",
                    Icon { icon: AppIcon::ChevronDown, size: 13 }
                }
            }
            PopoverContent { class: "touch-popover absolute top-[calc(100%+6px)] right-0 z-80 w-[min(430px,calc(100vw-1rem))] overflow-hidden rounded-xl border border-border bg-popover shadow-2xl",
                if let Some(model) = selected.clone() {
                    div { class: "border-b border-border px-3 py-3",
                        div { class: "flex items-start gap-3",
                            div { class: "min-w-0 flex-1",
                                strong { class: "block truncate text-sm font-semibold",
                                    "{model.name}"
                                }
                                p { class: "mt-1 text-[10px] text-muted-foreground",
                                    {format_context_window(model.context_window)}
                                    if model.reasoning {
                                        " · Reasoning"
                                    }
                                    if model.supports_images {
                                        " · Vision"
                                    }
                                }
                                p {
                                    class: "mt-1 text-[10px] text-muted-foreground",
                                    title: "Input / output catalog price per million tokens",
                                    {format_model_price_long(&model)}
                                }
                            }
                            Icon { icon: AppIcon::Check, size: 15 }
                        }
                    }
                }
                ReasoningEffort {
                    model: selected.clone(),
                    selected: thinking_level,
                    disabled,
                    on_select: on_thinking,
                }
                Collapsible {
                    open: choosing_model(),
                    on_open_change: move |next| {
                        choosing_model.set(next);
                        if !next {
                            query.set(String::new());
                        }
                    },
                    CollapsibleTrigger { class: "flex h-11 w-full items-center px-3 text-left text-xs font-medium transition-colors hover:bg-accent",
                        "Choose model"
                        span { class: "ml-auto grid size-7 place-items-center text-muted-foreground",
                            if choosing_model() {
                                Icon { icon: AppIcon::Close, size: 14 }
                            } else {
                                Icon { icon: AppIcon::ChevronDown, size: 14 }
                            }
                        }
                    }
                    CollapsibleContent {
                        div { class: "border-y border-border p-3",
                            div { class: "flex h-9 items-center gap-2 rounded-lg border border-input bg-background px-3",
                                Icon { icon: AppIcon::Search, size: 14 }
                                input {
                                    class: "min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-muted-foreground",
                                    value: query(),
                                    placeholder: "Search models…",
                                    aria_label: "Search models",
                                    oninput: move |event| query.set(event.value()),
                                }
                            }
                        }
                        div { class: "max-h-[min(360px,55vh)] overflow-y-auto p-1.5",
                            if model_groups.is_empty() {
                                p { class: "px-3 py-8 text-center text-xs text-muted-foreground",
                                    "No matching models"
                                }
                            }
                            for (provider, provider_models) in model_groups {
                                div {
                                    key: "{provider}",
                                    class: "not-first:mt-1.5",
                                    div { class: "sticky top-0 z-1 flex h-8 items-center gap-2 bg-popover/95 px-2.5 text-[9px] font-semibold tracking-wide text-muted-foreground uppercase backdrop-blur-sm",
                                        span { class: "grid size-5 place-items-center rounded-md bg-primary/10 text-primary",
                                            ProviderIcon {
                                                provider: provider.clone(),
                                                size: 11,
                                            }
                                        }
                                        span { class: "truncate", "{provider}" }
                                    }
                                    for model in provider_models {
                                        ModelRow {
                                            key: "{model.key()}",
                                            model,
                                            on_select: move |model: ModelSummary| {
                                                let effective_level = model.effective_thinking_level(thinking_level);
                                                on_select.call((model.provider, model.id, effective_level));
                                                open.set(false);
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ReasoningEffort(
    model: Option<ModelSummary>,
    selected: ThinkingLevel,
    disabled: bool,
    on_select: EventHandler<ThinkingLevel>,
) -> Element {
    let Some(model) = model else {
        return rsx! {};
    };
    let levels = model.supported_thinking_levels().to_vec();
    let effective = model.effective_thinking_level(selected);
    let selected_index = levels
        .iter()
        .position(|level| *level == effective)
        .unwrap_or_default();
    if !model.reasoning {
        return rsx! {
            div { class: "flex items-center gap-2 border-b border-border bg-secondary/10 px-3 py-2 text-[10px] text-muted-foreground",
                Icon { icon: AppIcon::BrainCog, size: 13 }
                span { "Reasoning effort is unavailable for this model." }
            }
        };
    }
    rsx! {
        div { class: "border-b border-border bg-secondary/10 px-3 py-2.5",
            div { class: "flex items-center gap-2",
                Icon { icon: AppIcon::BrainCog, size: 14 }
                label {
                    class: "text-[10px] font-medium text-foreground",
                    r#for: "model-reasoning-effort",
                    "Reasoning effort"
                }
                output {
                    class: "ml-auto rounded-md bg-primary/10 px-2 py-0.5 text-[9px] font-semibold text-primary",
                    "for": "model-reasoning-effort",
                    "{effective.label()}"
                }
            }
            input {
                id: "model-reasoning-effort",
                class: "mt-2 h-7 w-full cursor-pointer accent-primary disabled:cursor-not-allowed disabled:opacity-50",
                r#type: "range",
                min: "0",
                max: "{levels.len().saturating_sub(1)}",
                step: "1",
                value: "{selected_index}",
                disabled: disabled || levels.len() < 2,
                aria_label: "Reasoning effort for {model.name}",
                oninput: move |event| {
                    if let Ok(index) = event.value().parse::<usize>()
                        && let Some(level) = levels.get(index)
                    {
                        on_select.call(*level);
                    }
                },
            }
            div { class: "flex justify-between text-[8px] text-muted-foreground",
                span { "{levels.first().copied().unwrap_or(ThinkingLevel::Off).label()}" }
                if levels.len() > 1 {
                    span { "{levels.last().copied().unwrap_or(ThinkingLevel::Off).label()}" }
                }
            }
        }
    }
}

#[component]
fn ModelRow(model: ModelSummary, on_select: EventHandler<ModelSummary>) -> Element {
    let selected_model = model.clone();
    rsx! {
        button {
            class: "grid min-h-12 w-full grid-cols-[minmax(0,1fr)_7rem] items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-xs text-muted-foreground hover:bg-accent hover:text-foreground",
            r#type: "button",
            onclick: move |_| on_select.call(selected_model.clone()),
            span { class: "min-w-0",
                strong { class: "block truncate font-medium", "{model.name}" }
                small { class: "block truncate text-[9px] text-muted-foreground",
                    {format_context_window(model.context_window)}
                    if model.reasoning {
                        " · Reasoning"
                    }
                    if model.supports_images {
                        " · Vision"
                    }
                }
            }
            span { class: "min-w-0 text-right",
                strong {
                    class: if model.cost.is_free() { "block text-[9px] font-medium text-success" } else { "block text-[9px] font-medium text-muted-foreground" },
                    title: "Input / output catalog price per million tokens",
                    {format_model_price(&model)}
                }
            }
        }
    }
}

#[component]
fn filter_models(models: Vec<ModelSummary>, query: &str) -> Vec<ModelSummary> {
    let query = query.trim().to_ascii_lowercase();
    let free_query = query == "free";
    let mut models = models
        .into_iter()
        .filter(|model| {
            let searchable =
                format!("{} {} {}", model.provider, model.name, model.id).to_ascii_lowercase();
            query.is_empty() || searchable.contains(&query) || (free_query && model.cost.is_free())
        })
        .collect::<Vec<_>>();
    models.sort_by_key(|model| (model.provider.to_lowercase(), model.name.to_lowercase()));
    models
}

fn group_models_by_provider(models: Vec<ModelSummary>) -> Vec<(String, Vec<ModelSummary>)> {
    let mut groups = Vec::<(String, Vec<ModelSummary>)>::new();
    for model in models {
        if let Some((_, provider_models)) = groups
            .last_mut()
            .filter(|(provider, _)| provider == &model.provider)
        {
            provider_models.push(model);
        } else {
            groups.push((model.provider.clone(), vec![model]));
        }
    }
    groups
}

fn format_model_price(model: &ModelSummary) -> String {
    if model.cost.is_free() {
        return "Free".to_owned();
    }
    let tiers = if model.cost.has_paid_tier { "+" } else { "" };
    format!(
        "{} in · {} out{tiers}",
        format_model_rate(model.cost.input),
        format_model_rate(model.cost.output),
    )
}

fn format_model_price_long(model: &ModelSummary) -> String {
    if model.cost.is_free() {
        return "Free".to_owned();
    }
    let tiers = if model.cost.has_paid_tier { "+" } else { "" };
    format!(
        "{} input · {} output{tiers} / 1M tokens",
        format_model_rate(model.cost.input),
        format_model_rate(model.cost.output),
    )
}

fn format_model_rate(microusd: u64) -> String {
    let mut rate = format!(
        "${}.{:04}",
        microusd / 1_000_000,
        (microusd % 1_000_000) / 100
    );
    while rate.ends_with('0') {
        rate.pop();
    }
    if rate.ends_with('.') {
        rate.pop();
    }
    rate
}

fn format_context_window(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        let whole = tokens / 1_000_000;
        let tenth = (tokens % 1_000_000) / 100_000;
        if tenth == 0 {
            format!("{whole}M context")
        } else {
            format!("{whole}.{tenth}M context")
        }
    } else {
        format!("{}K context", tokens / 1_000)
    }
}
