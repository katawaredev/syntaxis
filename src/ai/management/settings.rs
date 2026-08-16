use super::*;

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
    rsx! {
        div { class: "mx-auto max-w-3xl",
            if let Some(message) = snapshot.compatibility_message.clone() {
                p { class: "mb-5 rounded-lg bg-warning/10 p-3 text-xs text-warning",
                    "{message}"
                }
            }
            if let Some(message) = error() {
                p { class: "mb-5 rounded-lg bg-destructive/10 p-3 text-xs text-destructive",
                    "{message}"
                }
            }
            if selected_section() == AiSettingsSection::General {
                div { class: "space-y-5",
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
                                        disabled: !snapshot.compatible || saving().is_some(),
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
                        "Update Pi packages and refresh tracked skills."
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
            }
        }
    }
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
