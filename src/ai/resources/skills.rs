use super::*;

#[component]
pub(crate) fn SkillsPanel(
    workspace_id: String,
    mut revision: Signal<u64>,
    mut toast: Signal<Option<(String, Tone)>>,
    sidebar_open: bool,
    on_toggle_sidebar: EventHandler<()>,
    on_open_sidebar: EventHandler<()>,
) -> Element {
    let load_workspace = workspace_id.clone();
    let skills = use_resource(move || {
        let workspace_id = load_workspace.clone();
        let _ = revision();
        async move { api::pi_skills(workspace_id).await }
    });
    let catalog_access = use_resource(api::skill_catalog_available);
    let mut editing = use_signal(|| None::<(Option<String>, PiSkill)>);
    let mut deleting = use_signal(|| None::<PiSkill>);
    let query = use_signal(String::new);
    let submitted_query = use_signal(String::new);
    let search_revision = use_signal(|| 0_u64);
    let catalog_view = use_signal(|| SkillCatalogView::AllTime);
    let mut offset = use_signal(|| 0_usize);
    let mut results = use_signal(Vec::<SkillSearchResult>::new);
    let mut next_offset = use_signal(|| 0_usize);
    let mut has_more = use_signal(|| false);
    let mut loading_more = use_signal(|| false);
    let installing = use_signal(|| None::<String>);
    let search = use_resource(move || {
        let query = submitted_query();
        let view = catalog_view();
        let offset = offset();
        let catalog_available = catalog_access().and_then(Result::ok).unwrap_or(false);
        let _ = search_revision();
        async move {
            let result = if query.is_empty() {
                if catalog_available {
                    api::browse_pi_skills(view, offset).await
                } else {
                    Ok(SkillSearchPage {
                        skills: Vec::new(),
                        start_offset: 0,
                        next_offset: 0,
                        has_more: false,
                    })
                }
            } else {
                api::search_pi_skills(query.clone(), offset).await
            };
            (query, view, result)
        }
    });
    use_effect(move || {
        let Some((resource_query, resource_view, result)) = search() else {
            return;
        };
        if resource_query != submitted_query() || resource_view != catalog_view() {
            return;
        }
        if loading_more() {
            loading_more.set(false);
        }
        let Ok(page) = result else {
            return;
        };
        if page.start_offset == 0 {
            results.set(page.skills);
        } else {
            results.with_mut(|loaded| {
                for result in page.skills {
                    if !loaded.iter().any(|item| item.slug == result.slug) {
                        loaded.push(result);
                    }
                }
            });
        }
        next_offset.set(page.next_offset);
        has_more.set(page.has_more);
    });
    let search_result = search();
    let searching = search_result.is_none();
    let search_error = search_result
        .as_ref()
        .and_then(|(_, _, result)| result.as_ref().err())
        .map(ToString::to_string);
    let catalog_enabled = catalog_access().and_then(Result::ok).unwrap_or(false);
    let result_catalog_view = submitted_query().is_empty().then_some(catalog_view());
    let installed_skills = skills().and_then(Result::ok).unwrap_or_default();
    rsx! {
        ResourceHeader {
            title: "Skills",
            subtitle: "On-demand capabilities for Pi",
            action: "New skill",
            sidebar_open,
            on_toggle_sidebar,
            on_open_sidebar,
            on_action: move |()| {
                editing
                    .set(
                        Some((
                            None,
                            PiSkill {
                                name: String::new(),
                                description: String::new(),
                                content: "# Instructions\n\n".into(),
                                scope: PiResourceScope::Project,
                                storage_name: String::new(),
                                single_file: false,
                                extra_frontmatter: String::new(),
                            },
                        )),
                    );
            },
        }
        div { class: "min-h-0 flex-1 overflow-y-auto p-5",
            div { class: "mx-auto max-w-3xl space-y-6",
                section {
                    h3 { class: "mb-2 text-xs font-semibold", "Discover skills" }
                    p { class: "mb-3 text-[10px] leading-relaxed text-muted-foreground",
                        "Searches the public skills.sh catalog. Review a skill after installing: skills may include executable scripts and instructions with the server user's permissions."
                    }
                    SkillDiscoveryControls {
                        catalog_enabled,
                        searching,
                        query,
                        submitted_query,
                        catalog_view,
                        results,
                        offset,
                        has_more,
                        loading_more,
                        search_revision,
                    }
                    if let Some(ref error) = search_error {
                        p { class: "mt-3 rounded-lg bg-destructive/10 p-3 text-xs text-destructive",
                            "{error}"
                        }
                    }
                    if !results().is_empty() {
                        p { class: "py-3 text-[9px] text-muted-foreground",
                            "{results().len()} loaded results"
                        }
                        div { class: "grid grid-cols-2 gap-3 max-lg:grid-cols-1",
                            for result in results() {
                                SkillSearchCard {
                                    key: "{result.slug}",
                                    project_installed: installed_skills
                                        .iter()
                                        .any(|skill| {
                                            skill.name == result.name && skill.scope == PiResourceScope::Project
                                        }),
                                    global_installed: installed_skills
                                        .iter()
                                        .any(|skill| {
                                            skill.name == result.name && skill.scope == PiResourceScope::Global
                                        }),
                                    result,
                                    catalog_view: result_catalog_view,
                                    workspace_id: workspace_id.clone(),
                                    installing,
                                    revision,
                                    toast,
                                }
                            }
                        }
                        if has_more() {
                            div { class: "mx-auto mt-4 grid max-w-48",
                                Button {
                                    label: if loading_more() { "Loading more…" } else { "Load more" },
                                    kind: ButtonKind::Ghost,
                                    disabled: searching || loading_more(),
                                    onclick: move |_| {
                                        loading_more.set(true);
                                        offset.set(next_offset());
                                    },
                                }
                            }
                        } else {
                            p { class: "py-4 text-center text-[9px] text-muted-foreground",
                                "End of catalog results"
                            }
                        }
                    } else if !searching && search_error.is_none()
                        && (catalog_enabled || !submitted_query().is_empty())
                    {
                        if submitted_query().is_empty() {
                            EmptyResource { message: "No skills are available in this catalog view." }
                        } else {
                            EmptyResource { message: "No skills matched this search." }
                        }
                    }
                }
                section {
                    h3 { class: "mb-2 text-xs font-semibold", "Installed skills" }
                    match skills() {
                        None => rsx! {
                            p { class: "text-xs text-muted-foreground", "Loading skills…" }
                        },
                        Some(Err(error)) => rsx! {
                            p { class: "text-xs text-destructive", "{error}" }
                        },
                        Some(Ok(items)) if items.is_empty() => rsx! {
                            EmptyResource { message: "No directly managed Pi skills yet." }
                        },
                        Some(Ok(items)) => rsx! {
                            div { class: "grid gap-2",
                                for skill in items {
                                    ResourceCard {
                                        key: "{skill.scope:?}-{skill.name}",
                                        name: skill.name.clone(),
                                        description: skill.description.clone(),
                                        scope: skill.scope,
                                        on_edit: {
                                            let skill = skill.clone();
                                            move |()| editing.set(Some((Some(skill.storage_name.clone()), skill.clone())))
                                        },
                                        on_delete: move |()| deleting.set(Some(skill.clone())),
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
        if let Some((original_name, skill)) = editing() {
            SkillEditor {
                workspace_id: workspace_id.clone(),
                original_name,
                skill,
                editing,
                revision,
                toast,
            }
        }
        if let Some(skill) = deleting() {
            DeleteSkillDialog {
                workspace_id: workspace_id.clone(),
                skill,
                deleting,
                revision,
                toast,
            }
        }
    }
}

mod discovery;

use discovery::{SkillDiscoveryControls, SkillSearchCard};

#[component]
fn SkillEditor(
    workspace_id: String,
    original_name: Option<String>,
    skill: PiSkill,
    mut editing: Signal<Option<(Option<String>, PiSkill)>>,
    mut revision: Signal<u64>,
    mut toast: Signal<Option<(String, Tone)>>,
) -> Element {
    let mut name = use_signal(|| skill.name.clone());
    let mut description = use_signal(|| skill.description.clone());
    let mut content = use_signal(|| skill.content.clone());
    let scope = use_signal(|| skill.scope);
    let mut saving = use_signal(|| false);
    rsx! {
        Modal {
            title: if original_name.is_some() { "Edit skill" } else { "New skill" },
            description: "Pi loads the description at startup and reads the Markdown instructions when the skill is activated.",
            on_close: move |()| {
                if !saving() {
                    editing.set(None);
                }
            },
            DialogForm {
                Field { control_id: "skill-name", label: "Name",
                    TextInput {
                        value: name(),
                        placeholder: "code-review",
                        disabled: saving(),
                        oninput: move |event: FormEvent| name.set(event.value()),
                    }
                }
                Field { control_id: "skill-description", label: "Description",
                    TextInput {
                        value: description(),
                        placeholder: "What it does and when Pi should use it",
                        disabled: saving(),
                        oninput: move |event: FormEvent| description.set(event.value()),
                    }
                }
                ScopeSelect { scope, disabled: saving() || original_name.is_some() }
                Field {
                    control_id: "skill-content",
                    label: "Instructions (SKILL.md body)",
                    TextArea {
                        value: content(),
                        rows: 14,
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
                        label: if saving() { "Saving…" } else { "Save skill" },
                        kind: ButtonKind::Primary,
                        disabled: saving() || !skill_draft_valid(&name(), &description(), &content()),
                        onclick: move |_| {
                            saving.set(true);
                            let skill = PiSkill {
                                name: name().trim().to_owned(),
                                description: description().trim().to_owned(),
                                content: content(),
                                scope: scope(),
                                storage_name: skill.storage_name.clone(),
                                single_file: skill.single_file,
                                extra_frontmatter: skill.extra_frontmatter.clone(),
                            };
                            let workspace_id = workspace_id.clone();
                            let original_name = original_name.clone();
                            spawn(async move {
                                match api::save_pi_skill(workspace_id, original_name, skill).await {
                                    Ok(()) => {
                                        editing.set(None);
                                        revision.with_mut(|value| *value += 1);
                                        toast.set(Some(("Skill saved".into(), Tone::Success)));
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
fn DeleteSkillDialog(
    workspace_id: String,
    skill: PiSkill,
    mut deleting: Signal<Option<PiSkill>>,
    mut revision: Signal<u64>,
    mut toast: Signal<Option<(String, Tone)>>,
) -> Element {
    let name = skill.name.clone();
    rsx! {
        Modal {
            title: "Delete {name}?",
            description: "This permanently removes the skill directory, including scripts, references, and assets.",
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
                            let storage_name = skill.storage_name.clone();
                            let scope = skill.scope;
                            let single_file = skill.single_file;
                            spawn(async move {
                                match api::delete_pi_skill(workspace_id, storage_name, scope, single_file)
                                    .await
                                {
                                    Ok(()) => {
                                        revision.with_mut(|value| *value += 1);
                                        toast.set(Some(("Skill deleted".into(), Tone::Success)));
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

fn skill_draft_valid(name: &str, description: &str, content: &str) -> bool {
    !name.trim().is_empty() && !description.trim().is_empty() && !content.trim().is_empty()
}
