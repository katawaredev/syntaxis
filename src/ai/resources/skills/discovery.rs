use super::*;

#[component]
pub(super) fn SkillDiscoveryControls(
    catalog_enabled: bool,
    searching: bool,
    mut query: Signal<String>,
    mut submitted_query: Signal<String>,
    mut catalog_view: Signal<SkillCatalogView>,
    mut results: Signal<Vec<SkillSearchResult>>,
    mut offset: Signal<usize>,
    mut has_more: Signal<bool>,
    mut loading_more: Signal<bool>,
    mut search_revision: Signal<u64>,
) -> Element {
    let query_length = query().trim().len();
    let search_disabled = searching || query_length == 1 || (!catalog_enabled && query_length == 0);
    rsx! {
        div { class: if catalog_enabled { "grid grid-cols-[minmax(12rem,1fr)_10rem_auto] gap-2 max-sm:grid-cols-1" } else { "grid grid-cols-[minmax(12rem,1fr)_auto] gap-2 max-sm:grid-cols-1" },
            TextInput {
                value: query(),
                placeholder: "Search skills (for example: Rust)",
                oninput: move |event: FormEvent| query.set(event.value()),
            }
            if catalog_enabled {
                select {
                    aria_label: "Skills catalog view",
                    class: "h-9 rounded-lg border border-input bg-background px-3 text-xs",
                    value: catalog_view_value(catalog_view()),
                    disabled: searching,
                    onchange: move |event| {
                        let next = match event.value().as_str() {
                            "trending" => SkillCatalogView::Trending,
                            "hot" => SkillCatalogView::Hot,
                            _ => SkillCatalogView::AllTime,
                        };
                        query.set(String::new());
                        submitted_query.set(String::new());
                        results.set(Vec::new());
                        offset.set(0);
                        has_more.set(false);
                        loading_more.set(false);
                        catalog_view.set(next);
                    },
                    option { value: "all-time", "All time" }
                    option { value: "trending", "Trending" }
                    option { value: "hot", "Hot" }
                }
            }
            Button {
                label: if searching { "Searching…" } else { "Search" },
                kind: ButtonKind::Secondary,
                disabled: search_disabled,
                onclick: move |_| {
                    results.set(Vec::new());
                    offset.set(0);
                    has_more.set(false);
                    loading_more.set(false);
                    submitted_query.set(query().trim().to_owned());
                    search_revision.with_mut(|value| *value += 1);
                },
            }
        }
    }
}

#[component]
pub(super) fn SkillSearchCard(
    project_installed: bool,
    global_installed: bool,
    result: SkillSearchResult,
    catalog_view: Option<SkillCatalogView>,
    workspace_id: String,
    installing: Signal<Option<String>>,
    revision: Signal<u64>,
    toast: Signal<Option<(String, Tone)>>,
) -> Element {
    rsx! {
        article { class: "flex min-h-40 flex-col rounded-xl border border-border bg-background p-4",
            div { class: "flex items-start gap-3",
                div { class: "min-w-0 flex-1",
                    a {
                        href: result.page_url.clone(),
                        target: "_blank",
                        rel: "noopener noreferrer",
                        class: "break-all text-sm font-semibold text-foreground hover:text-primary hover:underline",
                        "{result.name}"
                    }
                    p { class: "mt-2 text-[10px] text-muted-foreground", "{result.source}" }
                }
                if project_installed || global_installed {
                    span { class: "shrink-0 rounded-md bg-success/12 px-2 py-1 text-[8px] font-medium text-success",
                        "Installed"
                    }
                }
            }
            div { class: "mt-auto flex flex-wrap items-end justify-between gap-3 pt-4",
                small { class: "text-[9px] text-muted-foreground",
                    "{skill_metric(result.installs, catalog_view)}"
                }
                if result.installable {
                    div { class: "flex gap-1",
                        InstallSkillButton {
                            label: "Project",
                            installed: project_installed,
                            scope: PiResourceScope::Project,
                            result: result.clone(),
                            workspace_id: workspace_id.clone(),
                            installing,
                            revision,
                            toast,
                        }
                        InstallSkillButton {
                            label: "Global",
                            installed: global_installed,
                            scope: PiResourceScope::Global,
                            result: result.clone(),
                            workspace_id,
                            installing,
                            revision,
                            toast,
                        }
                    }
                } else {
                    small { class: "text-[9px] text-muted-foreground", "External source" }
                }
            }
        }
    }
}

#[component]
fn InstallSkillButton(
    label: &'static str,
    installed: bool,
    scope: PiResourceScope,
    result: SkillSearchResult,
    workspace_id: String,
    mut installing: Signal<Option<String>>,
    mut revision: Signal<u64>,
    mut toast: Signal<Option<(String, Tone)>>,
) -> Element {
    let pending = installing().as_deref() == Some(result.slug.as_str());
    rsx! {
        Button {
            label: if pending { "Installing…" } else if installed { "Installed" } else { label },
            kind: if installed { ButtonKind::Secondary } else { ButtonKind::Primary },
            disabled: installed || installing().is_some(),
            onclick: move |_| {
                installing.set(Some(result.slug.clone()));
                let workspace_id = workspace_id.clone();
                let slug = result.slug.clone();
                spawn(async move {
                    match api::install_pi_skill(workspace_id, slug, scope).await {
                        Ok(()) => {
                            revision.with_mut(|value| *value += 1);
                            toast.set(Some(("Skill installed".into(), Tone::Success)));
                        }
                        Err(error) => toast.set(Some((error.to_string(), Tone::Destructive))),
                    }
                    installing.set(None);
                });
            },
        }
    }
}

fn format_installs(installs: u64) -> String {
    if installs >= 1_000_000 {
        format_compact_count(installs, 1_000_000, "M")
    } else if installs >= 1_000 {
        format_compact_count(installs, 1_000, "K")
    } else {
        installs.to_string()
    }
}

fn skill_metric(installs: u64, view: Option<SkillCatalogView>) -> String {
    let count = format_installs(installs);
    match view {
        Some(SkillCatalogView::Trending) => format!("{count} in 24h"),
        Some(SkillCatalogView::Hot) => format!("{count} this hour"),
        Some(SkillCatalogView::AllTime) | None => format!("{count} installs"),
    }
}

const fn catalog_view_value(view: SkillCatalogView) -> &'static str {
    match view {
        SkillCatalogView::AllTime => "all-time",
        SkillCatalogView::Trending => "trending",
        SkillCatalogView::Hot => "hot",
    }
}

fn format_compact_count(value: u64, divisor: u64, suffix: &str) -> String {
    let whole = value / divisor;
    let decimal = value % divisor * 10 / divisor;
    if decimal == 0 {
        format!("{whole}{suffix}")
    } else {
        format!("{whole}.{decimal}{suffix}")
    }
}
