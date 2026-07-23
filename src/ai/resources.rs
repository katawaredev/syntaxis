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

#[component]
pub(super) fn PromptTemplatesPanel(
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
pub(super) fn SkillsPanel(
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
        let Some((resource_query, resource_view, Ok(page))) = search() else {
            return;
        };
        if resource_query != submitted_query() || resource_view != catalog_view() {
            return;
        }
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
                                    label: if searching { "Loading…" } else { "Load more" },
                                    kind: ButtonKind::Ghost,
                                    disabled: searching,
                                    onclick: move |_| offset.set(next_offset()),
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

#[component]
fn SkillDiscoveryControls(
    catalog_enabled: bool,
    searching: bool,
    mut query: Signal<String>,
    mut submitted_query: Signal<String>,
    mut catalog_view: Signal<SkillCatalogView>,
    mut results: Signal<Vec<SkillSearchResult>>,
    mut offset: Signal<usize>,
    mut has_more: Signal<bool>,
    mut search_revision: Signal<u64>,
) -> Element {
    let query_length = query().trim().len();
    let search_disabled = searching || query_length == 1 || (!catalog_enabled && query_length == 0);
    rsx! {
        div { class: "grid grid-cols-[minmax(12rem,1fr)_10rem_auto] gap-2 max-sm:grid-cols-1",
            TextInput {
                value: query(),
                placeholder: "Search skills (for example: Rust)",
                oninput: move |event: FormEvent| query.set(event.value()),
            }
            select {
                aria_label: "Skills catalog view",
                class: "h-9 rounded-lg border border-input bg-background px-3 text-xs",
                value: catalog_view_value(catalog_view()),
                disabled: searching || !catalog_enabled,
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
                    catalog_view.set(next);
                },
                option { value: "all-time", "All time" }
                option { value: "trending", "Trending" }
                option { value: "hot", "Hot" }
            }
            Button {
                label: if searching { "Searching…" } else { "Search" },
                kind: ButtonKind::Secondary,
                disabled: search_disabled,
                onclick: move |_| {
                    results.set(Vec::new());
                    offset.set(0);
                    has_more.set(false);
                    submitted_query.set(query().trim().to_owned());
                    search_revision.with_mut(|value| *value += 1);
                },
            }
        }
        if !catalog_enabled {
            p { class: "mt-2 text-[9px] text-muted-foreground",
                "Set VERCEL_OIDC_TOKEN on the Syntaxis server to enable the All time, Trending, and Hot skills.sh leaderboards."
            }
        }
    }
}

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
                                        toast.set(Some(("Prompt template saved".into(), Tone::Success)));
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
                                        toast.set(Some(("Prompt template deleted".into(), Tone::Success)));
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

#[component]
fn SkillSearchCard(
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

const fn scope_label(scope: PiResourceScope) -> &'static str {
    match scope {
        PiResourceScope::Global => "Global",
        PiResourceScope::Project => "Project",
    }
}

fn skill_draft_valid(name: &str, description: &str, content: &str) -> bool {
    !name.trim().is_empty() && !description.trim().is_empty() && !content.trim().is_empty()
}

const PROMPT_EDITOR_DESCRIPTION: &str =
    "The filename becomes the /command name. Use $1, $@, and ${1:-default} for arguments.";
