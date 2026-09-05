use super::*;

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum SettingsView {
    #[default]
    Essentials,
    Advanced,
}

#[component]
pub(super) fn SettingsForm(
    workspace_id: String,
    snapshot: PiSettingsSnapshot,
    selected_section: ReadSignal<AiSettingsSection>,
    revision: Signal<u64>,
    toast: Signal<Option<(String, Tone)>>,
    on_provider_accounts_changed: EventHandler<()>,
) -> Element {
    let saving = use_signal(|| None::<String>);
    let error = use_signal(|| None::<String>);
    let mut view = use_signal(SettingsView::default);
    rsx! {
        div { class: "mx-auto max-w-3xl",
            if let Some(message) = error() {
                p { class: "mb-5 rounded-lg bg-destructive/10 p-3 text-xs text-destructive",
                    "{message}"
                }
            }
            if selected_section() == AiSettingsSection::General {
                div { class: "space-y-5",
                    div { class: "flex items-center justify-between gap-3",
                        div { class: "inline-flex rounded-lg border border-border bg-background p-1",
                            button {
                                r#type: "button",
                                class: if view() == SettingsView::Essentials { "rounded-md bg-muted px-3 py-1.5 text-[10px] font-medium text-foreground" } else { "rounded-md px-3 py-1.5 text-[10px] text-muted-foreground hover:text-foreground" },
                                onclick: move |_| view.set(SettingsView::Essentials),
                                "Essentials"
                            }
                            button {
                                r#type: "button",
                                class: if view() == SettingsView::Advanced { "rounded-md bg-muted px-3 py-1.5 text-[10px] font-medium text-foreground" } else { "rounded-md px-3 py-1.5 text-[10px] text-muted-foreground hover:text-foreground" },
                                onclick: move |_| view.set(SettingsView::Advanced),
                                "Advanced JSON"
                            }
                        }
                        small { class: "text-[9px] text-muted-foreground", "Pi {snapshot.pi_version}" }
                    }
                    if view() == SettingsView::Essentials {
                        PiUpdate {
                            workspace_id: workspace_id.clone(),
                            revision,
                            toast,
                        }
                        for section in definition_sections() {
                            section {
                                h3 { class: "mb-2 px-1 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground",
                                    "{section}"
                                }
                                div { class: "divide-y divide-border overflow-hidden rounded-xl border border-border bg-background",
                                    for definition in PI_SETTING_DEFINITIONS
                                        .iter()
                                        .copied()
                                        .filter(|definition| definition.section == section)
                                    {
                                        SettingRow {
                                            key: "{definition.path}",
                                            definition,
                                            current: setting_value(&snapshot.values, definition),
                                            disabled: saving().is_some()
                                                || !snapshot.available_setters.iter().any(|setter| setter == definition.setter),
                                            unavailable: !snapshot.available_setters.iter().any(|setter| setter == definition.setter),
                                            saving: saving().as_deref() == Some(definition.path),
                                            workspace_id: workspace_id.clone(),
                                            saving_state: saving,
                                            error,
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if view() == SettingsView::Advanced {
                        AdvancedSettings {
                            workspace_id: workspace_id.clone(),
                            revision,
                            toast,
                        }
                    }
                }
            }
            if selected_section() == AiSettingsSection::ProviderAccounts {
                ProviderAccounts {
                    workspace_id: workspace_id.clone(),
                    on_accounts_changed: on_provider_accounts_changed,
                }
            }
        }
    }
}

#[component]
pub(super) fn PiUpdate(
    workspace_id: String,
    mut revision: Signal<u64>,
    mut toast: Signal<Option<(String, Tone)>>,
) -> Element {
    let mut pending = use_signal(|| false);
    rsx! {
        section {
            h3 { class: "mb-2 px-1 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground",
                "Updates"
            }
            div { class: "flex items-center gap-4 rounded-xl border border-border bg-background px-4 py-3",
                div { class: "min-w-0 flex-1",
                    strong { class: "block text-xs font-medium", "Pi and tracked skills" }
                    p { class: "mt-0.5 text-[10px] leading-relaxed text-muted-foreground",
                        "Install the latest Pi release and refresh tracked skills."
                    }
                }
                Button {
                    label: if pending() { "Updating…" } else { "Update" },
                    kind: ButtonKind::Primary,
                    disabled: pending(),
                    onclick: move |_| {
                        pending.set(true);
                        let workspace_id = workspace_id.clone();
                        spawn(async move {
                            match api::update_pi(workspace_id).await {
                                Ok(result) => toast.set(Some((result.message, Tone::Success))),
                                Err(error) => {
                                    toast.set(Some((error.to_string(), Tone::Destructive)));
                                }
                            }
                            pending.set(false);
                            revision.with_mut(|revision| *revision += 1);
                        });
                    },
                }
            }
        }
    }
}

#[component]
fn SettingRow(
    definition: PiSettingDefinition,
    current: String,
    disabled: bool,
    unavailable: bool,
    saving: bool,
    workspace_id: String,
    mut saving_state: Signal<Option<String>>,
    mut error: Signal<Option<String>>,
) -> Element {
    let mut draft = use_signal(|| current.clone());
    use_effect(use_reactive((&current,), move |(current,)| {
        draft.set(current);
    }));
    let previous_value = current.clone();
    let save = EventHandler::new(move |value: Value| {
        saving_state.set(Some(definition.path.into()));
        error.set(None);
        let workspace_id = workspace_id.clone();
        let rollback_value = previous_value.clone();
        spawn(async move {
            match api::update_pi_setting(workspace_id, definition.path.into(), value).await {
                Ok(snapshot) => draft.set(setting_value(&snapshot.values, definition)),
                Err(update_error) => {
                    draft.set(rollback_value);
                    error.set(Some(update_error.to_string()));
                }
            }
            saving_state.set(None);
        });
    });
    rsx! {
        div { class: "grid grid-cols-[minmax(0,1fr)_minmax(9rem,14rem)] items-center gap-4 px-4 py-3 max-sm:grid-cols-1",
            div { class: "min-w-0",
                strong { class: "block text-xs font-medium", "{definition.label}" }
                p { class: "mt-0.5 text-[10px] leading-relaxed text-muted-foreground",
                    "{definition.description}"
                }
            }
            div { class: "min-w-0",
                match definition.kind {
                    PiSettingKind::Toggle => rsx! {
                        select {
                            class: "h-8 w-full rounded-lg border border-input bg-background px-2 text-xs",
                            disabled,
                            value: draft(),
                            onchange: move |event| {
                                let value = event.value();
                                draft.set(value.clone());
                                save.call(json!(value == "true"));
                            },
                            option { value: "true", "On" }
                            option { value: "false", "Off" }
                        }
                    },
                    PiSettingKind::Select(options) => rsx! {
                        select {
                            class: "h-8 w-full rounded-lg border border-input bg-background px-2 text-xs",
                            disabled,
                            value: draft(),
                            onchange: move |event| {
                                let value = event.value();
                                draft.set(value.clone());
                                save.call(json!(value));
                            },
                            if definition.default_value.is_empty() {
                                option { value: "", "Not set" }
                            }
                            for option in options {
                                option { value: option, "{option}" }
                            }
                        }
                    },
                    PiSettingKind::Number | PiSettingKind::Text | PiSettingKind::StringArray => {
                        rsx! {
                            input {
                                class: "h-8 w-full rounded-lg border border-input bg-background px-2 text-xs",
                                r#type: if definition.kind == PiSettingKind::Number { "number" } else { "text" },
                                placeholder: if definition.kind == PiSettingKind::StringArray { "Comma-separated values" } else { "" },
                                disabled,
                                value: draft(),
                                oninput: move |event| draft.set(event.value()),
                                onblur: move |_| {
                                    let value = draft();
                                    if value != current
                                        && let Some(value) = draft_setting_value(definition.kind, &value)
                                    {
                                        save.call(value);
                                    }
                                },
                            }
                        }
                    }
                }
                if saving {
                    small { class: "text-[9px] text-muted-foreground", "Saving…" }
                }
                if unavailable {
                    small { class: "text-[9px] text-warning", "Unavailable in this Pi version" }
                }
            }
        }
    }
}

#[component]
fn AdvancedSettings(
    workspace_id: String,
    revision: Signal<u64>,
    toast: Signal<Option<(String, Tone)>>,
) -> Element {
    let mut scope = use_signal(|| PiResourceScope::Global);
    let reload = use_signal(|| 0_u64);
    let resource_workspace_id = workspace_id.clone();
    let settings = use_resource(move || {
        let workspace_id = resource_workspace_id.clone();
        let scope = scope();
        let _ = reload();
        async move { api::pi_advanced_settings(workspace_id, scope).await }
    });
    rsx! {
        section { class: "space-y-3",
            div { class: "flex flex-wrap items-center gap-3",
                label { class: "text-[10px] font-medium text-muted-foreground", "Scope" }
                select {
                    class: "h-8 rounded-lg border border-input bg-background px-2 text-xs",
                    value: match scope() {
                        PiResourceScope::Global => "global",
                        PiResourceScope::Project => "project",
                    },
                    onchange: move |event| {
                        scope
                            .set(
                                if event.value() == "project" {
                                    PiResourceScope::Project
                                } else {
                                    PiResourceScope::Global
                                },
                            );
                    },
                    option { value: "global", "Global" }
                    option { value: "project", "Project" }
                }
                p { class: "min-w-0 flex-1 text-[10px] text-muted-foreground",
                    "Edit every setting supported by the installed Pi version. JSON syntax is required."
                }
            }
            match settings() {
                None => rsx! {
                    p { class: "text-xs text-muted-foreground", "Loading advanced settings…" }
                },
                Some(Err(load_error)) => rsx! {
                    div { class: "rounded-lg bg-destructive/10 p-3 text-xs text-destructive", "{load_error}" }
                },
                Some(Ok(snapshot)) => rsx! {
                    AdvancedSettingsEditor {
                        workspace_id: workspace_id.clone(),
                        snapshot,
                        revision,
                        reload,
                        toast,
                    }
                },
            }
        }
    }
}

#[component]
fn AdvancedSettingsEditor(
    workspace_id: String,
    snapshot: PiAdvancedSettingsSnapshot,
    mut revision: Signal<u64>,
    mut reload: Signal<u64>,
    mut toast: Signal<Option<(String, Tone)>>,
) -> Element {
    let mut draft = use_signal(|| snapshot.content.clone());
    let mut saved_content = use_signal(|| snapshot.content.clone());
    let mut current_revision = use_signal(|| snapshot.revision.clone());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut docs_query = use_signal(String::new);
    use_effect(use_reactive(
        (&snapshot.content, &snapshot.revision),
        move |(content, snapshot_revision)| {
            draft.set(content.clone());
            saved_content.set(content);
            current_revision.set(snapshot_revision);
            error.set(None);
        },
    ));
    let filtered_docs = filter_documentation(&snapshot.documentation, &docs_query());
    rsx! {
        div { class: "space-y-3",
            div { class: "rounded-xl border border-border bg-background",
                div { class: "flex items-center gap-3 border-b border-border px-3 py-2",
                    code { class: "min-w-0 flex-1 truncate text-[9px] text-muted-foreground",
                        "{snapshot.path}"
                    }
                    Button {
                        label: if saving() { "Saving…" } else { "Save" },
                        kind: ButtonKind::Primary,
                        disabled: saving() || draft() == saved_content(),
                        onclick: move |_| {
                            saving.set(true);
                            error.set(None);
                            let workspace_id = workspace_id.clone();
                            let content = draft();
                            let expected_revision = current_revision();
                            let scope = snapshot.scope;
                            spawn(async move {
                                match api::save_pi_advanced_settings(
                                        workspace_id,
                                        scope,
                                        content,
                                        expected_revision,
                                    )
                                    .await
                                {
                                    Ok(saved) => {
                                        draft.set(saved.content.clone());
                                        saved_content.set(saved.content);
                                        current_revision.set(saved.revision);
                                        toast
                                            .set(
                                                Some((
                                                    "Pi settings saved. One rolling backup was kept.".into(),
                                                    Tone::Success,
                                                )),
                                            );
                                        revision.with_mut(|value| *value += 1);
                                    }
                                    Err(save_error) => error.set(Some(save_error.to_string())),
                                }
                                saving.set(false);
                            });
                        },
                    }
                    Button {
                        label: "Reload",
                        kind: ButtonKind::Secondary,
                        disabled: saving(),
                        onclick: move |_| reload.with_mut(|value| *value += 1),
                    }
                }
                textarea {
                    class: "block min-h-80 w-full resize-y bg-transparent p-4 font-mono text-[11px] leading-relaxed text-foreground outline-none",
                    spellcheck: "false",
                    value: draft(),
                    oninput: move |event| draft.set(event.value()),
                }
            }
            if let Some(message) = error() {
                p { class: "rounded-lg bg-destructive/10 p-3 text-xs text-destructive",
                    "{message}"
                }
            }
            details { class: "rounded-xl border border-border bg-background",
                summary { class: "cursor-pointer px-4 py-3 text-xs font-medium",
                    "Installed Pi settings reference"
                }
                div { class: "space-y-3 border-t border-border p-4",
                    input {
                        class: "h-8 w-full rounded-lg border border-input bg-background px-3 text-xs",
                        placeholder: "Search installed Pi documentation",
                        value: docs_query(),
                        oninput: move |event| docs_query.set(event.value()),
                    }
                    pre { class: "max-h-96 overflow-auto whitespace-pre-wrap font-mono text-[10px] leading-relaxed text-muted-foreground",
                        "{filtered_docs}"
                    }
                }
            }
        }
    }
}

fn filter_documentation(documentation: &str, query: &str) -> String {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return documentation.to_owned();
    }
    documentation
        .lines()
        .filter(|line| line.to_lowercase().contains(&query))
        .collect::<Vec<_>>()
        .join("\n")
}

fn draft_setting_value(kind: PiSettingKind, value: &str) -> Option<Value> {
    match kind {
        PiSettingKind::Number => value.parse::<u64>().ok().map(|number| json!(number)),
        PiSettingKind::StringArray => Some(json!(
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
        )),
        PiSettingKind::Text => Some(json!(value)),
        PiSettingKind::Toggle | PiSettingKind::Select(_) => None,
    }
}

fn setting_value(values: &Value, definition: PiSettingDefinition) -> String {
    let value = definition
        .path
        .split('.')
        .try_fold(values, |value, segment| value.get(segment));
    match value {
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::String(value)) => value.clone(),
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        _ => definition.default_value.into(),
    }
}

fn definition_sections() -> Vec<&'static str> {
    let mut sections = Vec::new();
    for definition in PI_SETTING_DEFINITIONS {
        if !sections.contains(&definition.section) {
            sections.push(definition.section);
        }
    }
    sections
}
