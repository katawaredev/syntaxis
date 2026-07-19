use dioxus::prelude::*;
use serde_json::{json, Value};
use syntaxis_ui::prelude::{AppIcon, Button, ButtonKind, IconButton, Tone};

use super::{
    api::{self, PiSettingsSnapshot},
    generated_settings::{PiSettingDefinition, PiSettingKind, PI_SETTING_DEFINITIONS},
};

pub(super) const EXTENSIONS_SECTION: &str = "Extensions";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum AiPanel {
    #[default]
    Chat,
    Settings,
}

#[component]
pub(super) fn AiSidebarTabs(
    mut panel: Signal<AiPanel>,
    on_change: EventHandler<AiPanel>,
) -> Element {
    rsx! {
        div { class: "grid h-12 min-h-12 grid-cols-2 items-center gap-1 border-b border-border p-1.25",
            SidebarTab {
                label: "Chat",
                active: panel() == AiPanel::Chat,
                onclick: move |()| {
                    panel.set(AiPanel::Chat);
                    on_change.call(AiPanel::Chat);
                },
            }
            SidebarTab {
                label: "Settings",
                active: panel() == AiPanel::Settings,
                onclick: move |()| {
                    panel.set(AiPanel::Settings);
                    on_change.call(AiPanel::Settings);
                },
            }
        }
    }
}

#[component]
fn SidebarTab(label: &'static str, active: bool, onclick: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: if active { "file-tree-tab h-8.5 rounded-md bg-muted text-[11px] font-medium text-foreground" } else { "file-tree-tab h-8.5 rounded-md bg-transparent text-[11px] text-muted-foreground hover:bg-muted/60 hover:text-foreground" },
            onclick: move |_| onclick.call(()),
            "{label}"
        }
    }
}

pub(super) fn default_settings_section() -> String {
    EXTENSIONS_SECTION.to_owned()
}

#[component]
pub(super) fn SettingsSidebar(
    mut selected: Signal<String>,
    on_selected: EventHandler<()>,
) -> Element {
    rsx! {
        nav {
            class: "min-h-0 flex-1 overflow-y-auto p-2",
            aria_label: "Pi settings sections",
            for section in setting_sections() {
                button {
                    class: if selected() == section { "mb-1 w-full rounded-lg bg-primary/10 px-3 py-2 text-left text-[11px] font-medium text-primary" } else { "mb-1 w-full rounded-lg px-3 py-2 text-left text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground" },
                    onclick: move |_| {
                        selected.set(section.to_owned());
                        on_selected.call(());
                    },
                    "{section}"
                }
            }
        }
    }
}

#[component]
pub(super) fn SettingsPanel(
    workspace_id: String,
    mut revision: Signal<u64>,
    mut toast: Signal<Option<(String, Tone)>>,
    selected_section: ReadSignal<String>,
    sidebar_open: bool,
    on_toggle_sidebar: EventHandler<()>,
    on_open_sidebar: EventHandler<()>,
) -> Element {
    let settings_workspace_id = workspace_id.clone();
    let update_workspace_id = workspace_id.clone();
    let settings = use_resource(move || {
        let workspace_id = settings_workspace_id.clone();
        let _ = revision();
        async move { api::pi_settings(workspace_id).await }
    });
    let mut pending = use_signal(|| false);
    rsx! {
        section { class: "flex h-full min-h-0 flex-col bg-card",
            header { class: "flex min-h-12 items-center gap-3 border-b border-border bg-background px-4",
                ManagementSidebarButton {
                    sidebar_open,
                    on_toggle_sidebar,
                    on_open_sidebar,
                }
                div { class: "min-w-0 flex-1",
                    strong { class: "block text-xs", "{selected_section()}" }
                    small { class: "text-[9px] text-muted-foreground", "Pi settings" }
                }
                Button {
                    label: if pending() { "Checking…" } else { "Check for updates" },
                    kind: ButtonKind::Ghost,
                    disabled: pending(),
                    onclick: move |_| {
                        pending.set(true);
                        let workspace_id = update_workspace_id.clone();
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
            div { class: "min-h-0 flex-1 overflow-y-auto p-5",
                match settings() {
                    None => rsx! {
                        p { class: "text-xs text-muted-foreground", "Loading settings…" }
                    },
                    Some(Err(error)) => rsx! {
                        p { class: "text-xs text-destructive", "{error}" }
                    },
                    Some(Ok(snapshot)) => rsx! {
                        SettingsForm {
                            workspace_id: workspace_id.clone(),
                            snapshot,
                            revision,
                            selected_section,
                        }
                    },
                }
            }
        }
    }
}

#[component]
pub(super) fn ManagementSidebarButton(
    sidebar_open: bool,
    on_toggle_sidebar: EventHandler<()>,
    on_open_sidebar: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "shrink-0 max-md:hidden",
            IconButton {
                label: if sidebar_open { "Hide AI sidebar" } else { "Show AI sidebar" },
                icon: AppIcon::Explorer,
                pressed: sidebar_open,
                onclick: move |_| on_toggle_sidebar.call(()),
            }
        }
        div { class: "hidden shrink-0 max-md:block",
            IconButton {
                label: "Open AI sidebar",
                icon: AppIcon::Explorer,
                onclick: move |_| on_open_sidebar.call(()),
            }
        }
    }
}

#[component]
fn SettingsForm(
    workspace_id: String,
    snapshot: PiSettingsSnapshot,
    mut revision: Signal<u64>,
    selected_section: ReadSignal<String>,
) -> Element {
    let saving = use_signal(|| None::<String>);
    let error = use_signal(|| None::<String>);
    let definitions = PI_SETTING_DEFINITIONS
        .iter()
        .copied()
        .filter(|definition| definition.section == selected_section())
        .collect::<Vec<_>>();
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
            div { class: "divide-y divide-border overflow-hidden rounded-xl border border-border bg-background",
                for definition in definitions {
                    SettingRow {
                        key: "{definition.path}-{revision}",
                        definition,
                        values: snapshot.values.clone(),
                        disabled: !snapshot.compatible || saving().is_some(),
                        saving: saving().as_deref() == Some(definition.path),
                        workspace_id: workspace_id.clone(),
                        saving_state: saving,
                        error,
                        revision,
                    }
                }
            }
        }
    }
}

#[component]
fn SettingRow(
    definition: PiSettingDefinition,
    values: Value,
    disabled: bool,
    saving: bool,
    workspace_id: String,
    mut saving_state: Signal<Option<String>>,
    mut error: Signal<Option<String>>,
    mut revision: Signal<u64>,
) -> Element {
    let current = setting_value(&values, definition);
    let mut draft = use_signal(|| current.clone());
    let save = EventHandler::new(move |value: Value| {
        saving_state.set(Some(definition.path.into()));
        error.set(None);
        let workspace_id = workspace_id.clone();
        spawn(async move {
            match api::update_pi_setting(workspace_id, definition.path.into(), value).await {
                Ok(_) => revision.with_mut(|revision| *revision += 1),
                Err(update_error) => error.set(Some(update_error.to_string())),
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
                            value: current,
                            onchange: move |event| save.call(json!(event.value() == "true")),
                            option { value: "true", "On" }
                            option { value: "false", "Off" }
                        }
                    },
                    PiSettingKind::Select(options) => rsx! {
                        select {
                            class: "h-8 w-full rounded-lg border border-input bg-background px-2 text-xs",
                            disabled,
                            value: current,
                            onchange: move |event| save.call(json!(event.value())),
                            for option in options {
                                option { value: option, "{option}" }
                            }
                        }
                    },
                    PiSettingKind::Number | PiSettingKind::Text => rsx! {
                        input {
                            class: "h-8 w-full rounded-lg border border-input bg-background px-2 text-xs",
                            r#type: if definition.kind == PiSettingKind::Number { "number" } else { "text" },
                            disabled,
                            value: draft(),
                            oninput: move |event| draft.set(event.value()),
                            onblur: move |_| {
                                let value = draft();
                                if value != current {
                                    if definition.kind == PiSettingKind::Number {
                                        if let Ok(number) = value.parse::<u64>() {
                                            save.call(json!(number));
                                        }
                                    } else {
                                        save.call(json!(value));
                                    }
                                }
                            },
                        }
                    },
                }
                if saving {
                    small { class: "text-[9px] text-muted-foreground", "Saving…" }
                }
            }
        }
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
        _ => definition.default_value.into(),
    }
}

fn setting_sections() -> Vec<&'static str> {
    let mut sections = vec![EXTENSIONS_SECTION];
    for definition in PI_SETTING_DEFINITIONS {
        if !sections.contains(&definition.section) {
            sections.push(definition.section);
        }
    }
    sections
}
