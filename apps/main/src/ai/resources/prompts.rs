use super::*;

const PROMPT_EDITOR_DESCRIPTION: &str =
    "The filename becomes the /command name. Use $1, $@, and ${1:-default} for arguments.";

#[component]
pub(crate) fn PromptTemplatesPanel(
    workspace_id: String,
    mut revision: Signal<u64>,
    mut toast: Signal<Option<(String, Tone)>>,
    sidebar_open: bool,
    on_toggle_sidebar: EventHandler<()>,
    on_open_sidebar: EventHandler<()>,
) -> Element {
    let load_workspace = workspace_id.clone();
    let templates = use_resource(move || {
        let workspace_id = load_workspace.clone();
        let _ = revision();
        async move { api::prompt_templates(workspace_id).await }
    });
    let mut editing = use_signal(|| None::<(Option<String>, PromptTemplate)>);
    let mut deleting = use_signal(|| None::<PromptTemplate>);
    rsx! {
        ResourceHeader {
            title: "Prompt templates",
            subtitle: "Reusable /commands for Pi",
            action: "New template",
            sidebar_open,
            on_toggle_sidebar,
            on_open_sidebar,
            on_action: move |()| {
                editing
                    .set(
                        Some((
                            None,
                            PromptTemplate {
                                name: String::new(),
                                description: String::new(),
                                argument_hint: String::new(),
                                content: String::new(),
                                scope: PiResourceScope::Project,
                            },
                        )),
                    );
            },
        }
        div { class: "min-h-0 flex-1 overflow-y-auto p-5",
            div { class: "mx-auto max-w-3xl",
                p { class: "mb-4 text-xs leading-relaxed text-muted-foreground",
                    "Templates are Markdown snippets invoked as /name. Project templates live in .pi/prompts; global templates are shared by all workspaces."
                }
                match templates() {
                    None => rsx! {
                        p { class: "text-xs text-muted-foreground", "Loading templates…" }
                    },
                    Some(Err(error)) => rsx! {
                        p { class: "text-xs text-destructive", "{error}" }
                    },
                    Some(Ok(items)) if items.is_empty() => rsx! {
                        EmptyResource { message: "No prompt templates yet." }
                    },
                    Some(Ok(items)) => rsx! {
                        div { class: "grid gap-2",
                            for template in items {
                                ResourceCard {
                                    key: "{template.scope:?}-{template.name}",
                                    name: format!("/{}", template.name),
                                    description: template.description.clone(),
                                    scope: template.scope,
                                    on_edit: {
                                        let template = template.clone();
                                        move |()| editing.set(Some((Some(template.name.clone()), template.clone())))
                                    },
                                    on_delete: move |()| deleting.set(Some(template.clone())),
                                }
                            }
                        }
                    },
                }
            }
        }
        if let Some((original_name, template)) = editing() {
            PromptEditor {
                workspace_id: workspace_id.clone(),
                original_name,
                template,
                editing,
                revision,
                toast,
            }
        }
        if let Some(template) = deleting() {
            DeletePromptDialog {
                workspace_id: workspace_id.clone(),
                template,
                deleting,
                revision,
                toast,
            }
        }
    }
}

#[component]
fn PromptEditor(
    workspace_id: String,
    original_name: Option<String>,
    template: PromptTemplate,
    mut editing: Signal<Option<(Option<String>, PromptTemplate)>>,
    mut revision: Signal<u64>,
    mut toast: Signal<Option<(String, Tone)>>,
) -> Element {
    let mut name = use_signal(|| template.name.clone());
    let mut description = use_signal(|| template.description.clone());
    let mut argument_hint = use_signal(|| template.argument_hint.clone());
    let mut content = use_signal(|| template.content.clone());
    let scope = use_signal(|| template.scope);
    let mut saving = use_signal(|| false);
    rsx! {
        Modal {
            title: if original_name.is_some() { "Edit prompt template" } else { "New prompt template" },
            description: PROMPT_EDITOR_DESCRIPTION,
            on_close: move |()| {
                if !saving() {
                    editing.set(None);
                }
            },
            DialogForm {
                Field { control_id: "prompt-name", label: "Name",
                    TextInput {
                        value: name(),
                        placeholder: "review",
                        disabled: saving(),
                        oninput: move |event: FormEvent| name.set(event.value()),
                    }
                }
                Field { control_id: "prompt-description", label: "Description",
                    TextInput {
                        value: description(),
                        disabled: saving(),
                        oninput: move |event: FormEvent| description.set(event.value()),
                    }
                }
                Field {
                    control_id: "prompt-argument-hint",
                    label: "Argument hint",
                    TextInput {
                        value: argument_hint(),
                        placeholder: "<PR-URL> [focus]",
                        disabled: saving(),
                        oninput: move |event: FormEvent| argument_hint.set(event.value()),
                    }
                }
                ScopeSelect { scope, disabled: saving() || original_name.is_some() }
                Field { control_id: "prompt-content", label: "Prompt",
                    TextArea {
                        value: content(),
                        rows: 12,
                        disabled: saving(),
                        oninput: move |event: FormEvent| content.set(event.value()),
                    }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        onclick: move |_| editing.set(None),
                    }
                    Button {
                        label: if saving() { "Saving…" } else { "Save template" },
                        kind: ButtonKind::Primary,
                        disabled: saving() || name().trim().is_empty() || content().trim().is_empty(),
                        onclick: move |_| {
                            saving.set(true);
                            let template = PromptTemplate {
                                name: name().trim().to_owned(),
                                description: description().trim().to_owned(),
                                argument_hint: argument_hint().trim().to_owned(),
                                content: content(),
                                scope: scope(),
                            };
                            let workspace_id = workspace_id.clone();
                            let original_name = original_name.clone();
                            spawn(async move {
                                match api::save_prompt_template(workspace_id, original_name, template).await
                                {
                                    Ok(()) => {
                                        editing.set(None);
                                        revision.with_mut(|value| *value += 1);
                                    }
                                    Err(error) => toast.set(Some((error.to_string(), Tone::Destructive))),
                                }
                                saving.set(false);
                            });
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn DeletePromptDialog(
    workspace_id: String,
    template: PromptTemplate,
    mut deleting: Signal<Option<PromptTemplate>>,
    mut revision: Signal<u64>,
    mut toast: Signal<Option<(String, Tone)>>,
) -> Element {
    let name = template.name.clone();
    rsx! {
        Modal {
            title: "Delete /{name}?",
            description: "This permanently removes the prompt template file.",
            on_close: move |()| deleting.set(None),
            DialogForm {
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        onclick: move |_| deleting.set(None),
                    }
                    Button {
                        label: "Delete",
                        kind: ButtonKind::Danger,
                        onclick: move |_| {
                            deleting.set(None);
                            let workspace_id = workspace_id.clone();
                            let name = name.clone();
                            spawn(async move {
                                match api::delete_prompt_template(workspace_id, name, template.scope).await {
                                    Ok(()) => {
                                        revision.with_mut(|value| *value += 1);
                                    }
                                    Err(error) => toast.set(Some((error.to_string(), Tone::Destructive))),
                                }
                            });
                        },
                    }
                }
            }
        }
    }
}
