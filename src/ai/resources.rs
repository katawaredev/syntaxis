use dioxus::prelude::*;
use syntaxis_ui::prelude::{
    Button, ButtonKind, DialogActions, DialogForm, Field, Modal, TextArea, TextInput, Tone,
};

use super::{
    api::{
        self, PiResourceScope, PiSkill, PromptTemplate, SkillCatalogView, SkillSearchPage,
        SkillSearchResult,
    },
    management::ManagementSidebarButton,
};

mod prompts;
mod skills;

pub(super) use prompts::PromptTemplatesPanel;
pub(super) use skills::SkillsPanel;

#[component]
fn ResourceHeader(
    title: &'static str,
    subtitle: &'static str,
    action: &'static str,
    sidebar_open: bool,
    on_toggle_sidebar: EventHandler<()>,
    on_open_sidebar: EventHandler<()>,
    on_action: EventHandler<()>,
) -> Element {
    rsx! {
        header { class: "flex min-h-12 items-center gap-3 border-b border-border bg-background px-4",
            ManagementSidebarButton {
                sidebar_open,
                on_toggle_sidebar,
                on_open_sidebar,
            }
            div { class: "min-w-0 flex-1",
                strong { class: "block text-xs", "{title}" }
                small { class: "text-[9px] text-muted-foreground", "{subtitle}" }
            }
            Button {
                label: action,
                kind: ButtonKind::Primary,
                onclick: move |_| on_action.call(()),
            }
        }
    }
}

#[component]
fn EmptyResource(message: &'static str) -> Element {
    rsx! {
        p { class: "rounded-xl border border-dashed border-border p-6 text-center text-xs text-muted-foreground",
            "{message}"
        }
    }
}

#[component]
fn ResourceCard(
    name: String,
    description: String,
    scope: PiResourceScope,
    on_edit: EventHandler<()>,
    on_delete: EventHandler<()>,
) -> Element {
    rsx! {
        article { class: "flex items-center gap-3 rounded-xl border border-border bg-background p-3",
            div { class: "min-w-0 flex-1",
                strong { class: "block truncate text-xs", "{name}" }
                p { class: "mt-0.5 line-clamp-2 text-[10px] leading-relaxed text-muted-foreground",
                    "{description}"
                }
                small { class: "text-[9px] text-primary", "{scope_label(scope)}" }
            }
            Button {
                label: "Edit",
                kind: ButtonKind::Ghost,
                onclick: move |_| on_edit.call(()),
            }
            Button {
                label: "Delete",
                kind: ButtonKind::Danger,
                onclick: move |_| on_delete.call(()),
            }
        }
    }
}

#[component]
fn ScopeSelect(mut scope: Signal<PiResourceScope>, disabled: bool) -> Element {
    rsx! {
        Field { control_id: "resource-scope", label: "Scope",
            select {
                id: "resource-scope",
                class: "h-9 w-full rounded-lg border border-input bg-background px-3 text-xs",
                disabled,
                value: match scope() {
                    PiResourceScope::Global => "global",
                    PiResourceScope::Project => "project",
                },
                onchange: move |event| {
                    scope
                        .set(
                            if event.value() == "global" {
                                PiResourceScope::Global
                            } else {
                                PiResourceScope::Project
                            },
                        );
                },
                option { value: "project", "Project (.pi)" }
                option { value: "global", "Global (~/.pi/agent)" }
            }
        }
    }
}

fn scope_label(scope: PiResourceScope) -> &'static str {
    match scope {
        PiResourceScope::Global => "Global",
        PiResourceScope::Project => "Project",
    }
}
