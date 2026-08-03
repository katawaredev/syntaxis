use super::*;

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum PricingFilter {
    #[default]
    All,
    Free,
    Metered,
}
impl PricingFilter {
    const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Free => "free",
            Self::Metered => "metered",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "free" => Self::Free,
            "metered" => Self::Metered,
            _ => Self::All,
        }
    }
}
#[derive(Clone, Copy, Default)]
struct ModelFilters {
    pricing: PricingFilter,
    reasoning_only: bool,
    vision_only: bool,
    min_context: u64,
}

#[component]
pub(super) fn ModelPicker(
    selected: Option<ModelSummary>,
    models: Vec<ModelSummary>,
    disabled: bool,
    on_select: EventHandler<(String, String)>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut query = use_signal(String::new);
    let mut pricing = use_signal(PricingFilter::default);
    let mut reasoning_only = use_signal(|| false);
    let mut vision_only = use_signal(|| false);
    let mut min_context = use_signal(|| 0_u64);
    let selected_key = selected.as_ref().map(ModelSummary::key);
    let selected_name = selected
        .as_ref()
        .map_or_else(|| "Default model".to_owned(), |model| model.name.clone());
    let selected_provider = selected
        .as_ref()
        .map_or_else(|| "Pi".to_owned(), |model| model.provider.clone());
    let groups = group_models(
        models.clone(),
        &query(),
        ModelFilters {
            pricing: pricing(),
            reasoning_only: reasoning_only(),
            vision_only: vision_only(),
            min_context: min_context(),
        },
    );
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
                div { class: "space-y-2 border-b border-border bg-secondary/20 px-3 py-2",
                    div { class: "flex flex-wrap items-center gap-1.5",
                        select {
                            class: "h-7 rounded-md border border-input bg-background px-2 text-[10px] text-foreground",
                            aria_label: "Filter models by catalog price",
                            value: pricing().as_str(),
                            onchange: move |event| pricing.set(PricingFilter::from_str(&event.value())),
                            option { value: "all", "Any price" }
                            option { value: "free", "Free ($0 rates)" }
                            option { value: "metered", "Metered" }
                        }
                        select {
                            class: "h-7 rounded-md border border-input bg-background px-2 text-[10px] text-foreground",
                            aria_label: "Minimum model context window",
                            value: min_context().to_string(),
                            onchange: move |event| {
                                min_context.set(event.value().parse().unwrap_or_default());
                            },
                            option { value: "0", "Any context" }
                            option { value: "128000", "128K+ context" }
                            option { value: "200000", "200K+ context" }
                            option { value: "1000000", "1M+ context" }
                        }
                        label { class: "flex h-7 cursor-pointer items-center gap-1 rounded-md border border-input bg-background px-2 text-[10px]",
                            input {
                                r#type: "checkbox",
                                checked: reasoning_only(),
                                onchange: move |event| reasoning_only.set(event.checked()),
                            }
                            "Reasoning"
                        }
                        label { class: "flex h-7 cursor-pointer items-center gap-1 rounded-md border border-input bg-background px-2 text-[10px]",
                            input {
                                r#type: "checkbox",
                                checked: vision_only(),
                                onchange: move |event| vision_only.set(event.checked()),
                            }
                            "Vision"
                        }
                    }
                    p { class: "text-[9px] leading-relaxed text-muted-foreground",
                        "Pi only lists models available through configured credentials. Prices are catalog token rates; subscription models may still show metered rates."
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
pub(super) fn ThinkingPicker(
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
            span { class: "min-w-0 text-right",
                strong {
                    class: if model.cost.is_free() { "block text-[9px] font-medium text-success" } else { "block text-[9px] font-medium text-muted-foreground" },
                    title: "Input / output catalog price per million tokens",
                    {format_model_price(&model)}
                }
                small { class: "block truncate text-[8px] text-muted-foreground",
                    {format_context_window(model.context_window)}
                    if model.reasoning {
                        " · reasoning"
                    }
                    if model.supports_images {
                        " · vision"
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
            BrandMark { icon: BrandIcon::OpenAi, size }
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

fn group_models(
    models: Vec<ModelSummary>,
    query: &str,
    filters: ModelFilters,
) -> Vec<(String, Vec<ModelSummary>)> {
    let query = query.trim().to_ascii_lowercase();
    let mut groups = BTreeMap::<String, Vec<ModelSummary>>::new();
    for model in models {
        let searchable =
            format!("{} {} {}", model.provider, model.name, model.id).to_ascii_lowercase();
        let pricing_matches = match filters.pricing {
            PricingFilter::All => true,
            PricingFilter::Free => model.cost.is_free(),
            PricingFilter::Metered => !model.cost.is_free(),
        };
        if (query.is_empty() || searchable.contains(&query))
            && pricing_matches
            && (!filters.reasoning_only || model.reasoning)
            && (!filters.vision_only || model.supports_images)
            && model.context_window >= filters.min_context
        {
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
