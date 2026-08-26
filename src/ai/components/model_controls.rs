use super::*;

#[component]
pub(super) fn ModelPicker(
    workspace_id: String,
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
    let mut preferences = use_signal(crate::ai::api::ModelPreferences::default);
    let model_keys = models.iter().map(ModelSummary::key).collect::<Vec<_>>();
    let preferences_workspace_id = workspace_id.clone();
    let selected_for_preferences = selected.clone();
    let restore_thinking = on_thinking;
    use_effect(use_reactive((&model_keys,), move |(available_models,)| {
        if available_models.is_empty() {
            return;
        }
        let workspace_id = preferences_workspace_id.clone();
        let selected = selected_for_preferences.clone();
        let restore_thinking = restore_thinking;
        let before_sync = preferences.peek().clone();
        spawn(async move {
            if let Ok(synced) =
                crate::ai::api::sync_model_preferences(workspace_id, available_models).await
                && preferences.peek().eq(&before_sync)
            {
                let restored_effort = selected.as_ref().and_then(|model| {
                    synced
                        .efforts
                        .get(&model.key())
                        .copied()
                        .map(|effort| model.effective_thinking_level(effort))
                });
                preferences.set(synced);
                if let Some(restored_effort) = restored_effort
                    && restored_effort != thinking_level
                {
                    restore_thinking.call(restored_effort);
                }
            }
        });
    }));
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
    let filtered_models = filter_models(models.clone(), &query());
    let favourite_keys = preferences().favourites;
    let favourite_models = favourite_keys
        .iter()
        .filter_map(|key| {
            filtered_models
                .iter()
                .find(|model| model.key() == *key)
                .cloned()
        })
        .filter(|model| selected_key.as_deref() != Some(model.key().as_str()))
        .collect::<Vec<_>>();
    let visible_models = filtered_models
        .into_iter()
        .filter(|model| selected_key.as_deref() != Some(model.key().as_str()))
        .filter(|model| !favourite_keys.contains(&model.key()))
        .collect::<Vec<_>>();
    let model_groups = group_models_by_provider(visible_models);
    let selected_is_favourite = selected_key
        .as_ref()
        .is_some_and(|key| favourite_keys.contains(key));
    let reasoning_workspace_id = workspace_id.clone();
    rsx! {
        InteractivePopover {
            id: "ai-model-picker",
            label: "Choose agent model",
            class: "min-w-0",
            open: open(),
            on_open_change: move |next| {
                let was_open = open();
                open.set(next);
                if next && !was_open {
                    choosing_model.set(false);
                    query.set(String::new());
                }
            },
            disabled: disabled || models.is_empty(),
            trigger_class: if open() { "flex h-8 min-w-0 max-w-58 items-center gap-2 rounded-lg border border-primary/30 bg-accent px-2.5 text-left shadow-sm max-[590px]:max-w-34 max-[520px]:size-10 max-[520px]:max-w-none max-[520px]:justify-center max-[520px]:p-0" } else { "flex h-8 min-w-0 max-w-58 items-center gap-2 rounded-lg border border-input bg-background/80 px-2.5 text-left shadow-xs transition-colors hover:bg-accent max-[590px]:max-w-34 max-[520px]:size-10 max-[520px]:max-w-none max-[520px]:justify-center max-[520px]:p-0" },
            content_class: "absolute top-[calc(100%+6px)] right-0 z-80 w-[min(430px,calc(100vw-1rem))] overflow-hidden rounded-xl border border-border bg-popover shadow-2xl",
            trigger: rsx! {
                span { class: "grid size-5 shrink-0 place-items-center rounded-md bg-primary/10 text-primary",
                    ProviderIcon { provider: selected_provider.clone(), size: 12 }
                }
                span { class: "min-w-0 flex-1 max-[520px]:hidden",
                    strong { class: "block truncate text-[11px] font-medium", "{selected_name}" }
                    small { class: "block truncate text-[9px] text-muted-foreground", "{selected_detail}" }
                }
                span { class: "max-[520px]:hidden",
                    Icon { icon: AppIcon::ChevronDown, size: 13 }
                }
            },
            if let Some(model) = selected.clone() {
                div { class: "border-b border-border px-3 py-3",
                    div { class: "flex items-start gap-3",
                        div { class: "min-w-0 flex-1",
                            strong { class: "block truncate text-sm font-semibold", "{model.name}" }
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
                        button {
                            class: if selected_is_favourite { "grid size-8 shrink-0 place-items-center rounded-md text-primary hover:bg-accent" } else { "grid size-8 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" },
                            r#type: "button",
                            title: if selected_is_favourite { "Remove from favourites" } else { "Add to favourites" },
                            aria_label: if selected_is_favourite { "Remove {model.name} from favourites" } else { "Add {model.name} to favourites" },
                            onclick: {
                                let key = model.key();
                                let workspace_id = workspace_id.clone();
                                move |_| update_favourite(
                                    preferences,
                                    workspace_id.clone(),
                                    key.clone(),
                                    !selected_is_favourite,
                                )
                            },
                            Icon {
                                icon: if selected_is_favourite { AppIcon::FavouriteFilled } else { AppIcon::Favourite },
                                size: 15,
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
                on_select: move |level| {
                    if let Some(model) = selected.as_ref() {
                        remember_effort(
                            preferences,
                            reasoning_workspace_id.clone(),
                            model.key(),
                            level,
                        );
                    }
                    on_thinking.call(level);
                },
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
                        if favourite_models.is_empty() && model_groups.is_empty() {
                            p { class: "px-3 py-8 text-center text-xs text-muted-foreground",
                                "No matching models"
                            }
                        }
                        if !favourite_models.is_empty() {
                            div { class: "mb-1.5",
                                div { class: "sticky top-0 z-1 flex h-8 items-center gap-2 bg-popover/95 px-2.5 text-[9px] font-semibold tracking-wide text-muted-foreground uppercase backdrop-blur-sm",
                                    Icon { icon: AppIcon::Favourite, size: 11 }
                                    "Favourites"
                                }
                                for model in favourite_models {
                                    ModelRow {
                                        key: "{model.key()}",
                                        model,
                                        favourite: true,
                                        on_favourite: {
                                            let workspace_id = workspace_id.clone();
                                            move |model: ModelSummary| {
                                                update_favourite(preferences, workspace_id.clone(), model.key(), false);
                                            }
                                        },
                                        on_select: move |model: ModelSummary| {
                                            let requested_level = preferences()
                                                .efforts
                                                .get(&model.key())
                                                .copied()
                                                .unwrap_or(thinking_level);
                                            let effective_level = model.effective_thinking_level(requested_level);
                                            on_select.call((model.provider, model.id, effective_level));
                                            open.set(false);
                                        },
                                    }
                                }
                            }
                        }
                        for (provider, provider_models) in model_groups {
                            div { key: "{provider}", class: "not-first:mt-1.5",
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
                                        favourite: false,
                                        on_favourite: {
                                            let workspace_id = workspace_id.clone();
                                            move |model: ModelSummary| {
                                                update_favourite(preferences, workspace_id.clone(), model.key(), true);
                                            }
                                        },
                                        on_select: move |model: ModelSummary| {
                                            let requested_level = preferences()
                                                .efforts
                                                .get(&model.key())
                                                .copied()
                                                .unwrap_or(thinking_level);
                                            let effective_level = model.effective_thinking_level(requested_level);
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
fn ModelRow(
    model: ModelSummary,
    favourite: bool,
    on_favourite: EventHandler<ModelSummary>,
    on_select: EventHandler<ModelSummary>,
) -> Element {
    let selected_model = model.clone();
    let favourite_model = model.clone();
    rsx! {
        div { class: "flex min-h-12 w-full items-center rounded-lg text-xs text-muted-foreground hover:bg-accent hover:text-foreground",
            button {
                class: "grid min-h-12 min-w-0 flex-1 grid-cols-[minmax(0,1fr)_7rem] items-center gap-2 px-2.5 py-1.5 text-left",
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
            button {
                class: if favourite { "mr-1 grid size-9 shrink-0 place-items-center rounded-md text-primary hover:bg-background/70" } else { "mr-1 grid size-9 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-background/70 hover:text-foreground" },
                r#type: "button",
                title: if favourite { "Remove from favourites" } else { "Add to favourites" },
                aria_label: if favourite { "Remove {model.name} from favourites" } else { "Add {model.name} to favourites" },
                onclick: move |_| on_favourite.call(favourite_model.clone()),
                Icon {
                    icon: if favourite { AppIcon::FavouriteFilled } else { AppIcon::Favourite },
                    size: 14,
                }
            }
        }
    }
}

fn update_favourite(
    mut preferences: Signal<crate::ai::api::ModelPreferences>,
    workspace_id: String,
    model_key: String,
    favourite: bool,
) {
    preferences.with_mut(|current| {
        current.favourites.retain(|key| key != &model_key);
        if favourite {
            current.favourites.insert(0, model_key.clone());
        }
    });
    spawn(async move {
        let _ = crate::ai::api::set_favourite_model(workspace_id, model_key, favourite).await;
    });
}

fn remember_effort(
    mut preferences: Signal<crate::ai::api::ModelPreferences>,
    workspace_id: String,
    model_key: String,
    effort: ThinkingLevel,
) {
    preferences
        .write()
        .efforts
        .insert(model_key.clone(), effort);
    spawn(async move {
        let _ = crate::ai::api::set_model_effort(workspace_id, model_key, effort).await;
    });
}

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
