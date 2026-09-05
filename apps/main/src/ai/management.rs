use dioxus::prelude::*;
use serde_json::{Value, json};
use syntaxis_ui::prelude::{
    AiSidebarTabs as SharedAiSidebarTabs, AppIcon, Button, ButtonKind, DialogActions, DialogForm,
    IconButton, Modal, ProviderIcon, Tone,
};

use super::{
    AiSettingsSection,
    api::{
        self, PiAdvancedSettingsSnapshot, PiAuthFlow, PiAuthPrompt, PiAuthType, PiResourceScope,
        PiSettingsSnapshot,
    },
    generated_settings::{PI_SETTING_DEFINITIONS, PiSettingDefinition, PiSettingKind},
};

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
        SharedAiSidebarTabs {
            settings_active: panel() == AiPanel::Settings,
            on_chat: move |()| {
                panel.set(AiPanel::Chat);
                on_change.call(AiPanel::Chat);
            },
            on_settings: move |()| {
                panel.set(AiPanel::Settings);
                on_change.call(AiPanel::Settings);
            },
        }
    }
}

#[component]
pub(super) fn SettingsSidebar(
    selected: AiSettingsSection,
    on_selected: EventHandler<AiSettingsSection>,
) -> Element {
    rsx! {
        nav {
            class: "min-h-0 flex-1 overflow-y-auto p-2",
            aria_label: "Pi settings sections",
            for section in AiSettingsSection::ALL {
                button {
                    r#type: "button",
                    class: if selected == section { "mb-1 w-full rounded-lg bg-primary/10 px-3 py-2 text-left text-[11px] font-medium text-primary" } else { "mb-1 w-full rounded-lg px-3 py-2 text-left text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground" },
                    aria_current: if selected == section { "page" },
                    onclick: move |_| {
                        if selected != section {
                            on_selected.call(section);
                        }
                    },
                    "{section.label()}"
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
    selected_section: ReadSignal<AiSettingsSection>,
    sidebar_open: bool,
    on_toggle_sidebar: EventHandler<()>,
    on_open_sidebar: EventHandler<()>,
    on_provider_accounts_changed: EventHandler<()>,
) -> Element {
    let settings_workspace_id = workspace_id.clone();
    let settings = use_resource(move || {
        let workspace_id = settings_workspace_id.clone();
        let _ = revision();
        async move { api::pi_settings(workspace_id).await }
    });
    rsx! {
        section { class: "flex h-full min-h-0 flex-col bg-card",
            header { class: "flex min-h-12 items-center gap-3 border-b border-border bg-background px-4",
                ManagementSidebarButton {
                    sidebar_open,
                    on_toggle_sidebar,
                    on_open_sidebar,
                }
                div { class: "min-w-0 flex-1",
                    strong { class: "block text-xs", "{selected_section().label()}" }
                    small { class: "text-[9px] text-muted-foreground", "Pi settings" }
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
                            selected_section,
                            revision,
                            toast,
                            on_provider_accounts_changed,
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

mod providers;
mod settings;

use providers::ProviderAccounts;
use settings::SettingsForm;
