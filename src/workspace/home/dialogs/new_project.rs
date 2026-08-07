use dioxus::prelude::*;
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, DialogActions, DialogForm, Field, Icon, Modal, TemplateIcon,
    TextInput,
};
use syntaxis_workspace::WorkspaceRecord;

use crate::{app::Route, terminal::ProjectInitializerTerminal};

use super::RequestState;
use crate::workspace::{api, home::HomeDialog};

const PROJECT_PATH_ERROR: &str =
    "Use a project name or subpath without empty, dot, or parent folders.";

mod catalog;

use catalog::{ProjectTemplate, TemplateCategory, TemplateDefinition, CATEGORIES, TEMPLATES};

#[component]
pub(super) fn NewProjectDialog(
    mut dialog: Signal<HomeDialog>,
    on_notice: EventHandler<String>,
    on_changed: EventHandler<()>,
) -> Element {
    let mut project_path = use_signal(String::new);
    let mut selected = use_signal(ProjectTemplate::default);
    let mut template_filter = use_signal(String::new);
    let mut request = use_signal(|| RequestState::Idle);
    let mut workspace = use_signal(|| None::<WorkspaceRecord>);
    let mut setup_result = use_signal(|| None::<bool>);
    let navigator = use_navigator();
    let pending = request() == RequestState::Pending;
    let definition = template_definition(selected());
    let filter = template_filter();
    let path_error = (!project_path().trim().is_empty())
        .then(|| validate_project_path(&project_path()))
        .flatten();

    rsx! {
        Modal {
            title: if workspace.read().is_some() { "Building new project" } else { "New project" },
            description: if let Some(workspace) = workspace.read().as_ref() { format!("{} · {}", workspace.root, definition.label) } else { "Create inside an exposed workspace root, then scaffold it in a live terminal."
                .to_owned() },
            content_class: "max-w-225",
            on_close: move |()| {
                if !pending {
                    if workspace.read().is_some() && setup_result().is_none() {
                        on_notice.call("Project setup continues in its terminal session".into());
                    }
                    dialog.set(HomeDialog::None);
                }
            },
            if let Some(created) = workspace() {
                div { class: "px-5 pt-3 pb-5",
                    if let Some(command) = definition.command {
                        div { class: "h-[min(34rem,calc(100svh-13rem))] min-h-72 overflow-hidden rounded-lg border border-border bg-background",
                            ProjectInitializerTerminal {
                                workspace_id: created.id.0.clone(),
                                workspace_slug: created.slug.clone(),
                                command: command.to_owned(),
                                label: format!("Initialize {}", definition.label),
                                on_finished: {
                                    let workspace_id = created.id.0.clone();
                                    move |success| {
                                        setup_result.set(Some(success));
                                        let workspace_id = workspace_id.clone();
                                        spawn(async move {
                                            if let Ok(refreshed) = api::refresh_workspace(workspace_id).await {
                                                workspace.set(Some(refreshed));
                                            }
                                            on_changed.call(());
                                            if success {
                                                on_notice.call(format!("{} project is ready", definition.label));
                                            }
                                        });
                                    }
                                },
                            }
                        }
                        p { class: "mt-2.5 flex items-center gap-2 text-xs text-muted-foreground",
                            span { class: if setup_result() == Some(true) { "size-2 rounded-full bg-success" } else if setup_result() == Some(false) { "size-2 rounded-full bg-destructive" } else { "size-2 animate-pulse rounded-full bg-primary" } }
                            if setup_result() == Some(true) {
                                "Setup finished successfully."
                            } else if setup_result() == Some(false) {
                                "Setup exited with an error. The terminal remains available for repairs."
                            } else {
                                "You can interact with the initializer or leave it running in this terminal session."
                            }
                        }
                    } else {
                        div { class: "grid min-h-52 place-items-center rounded-lg border border-dashed border-border bg-muted/20 text-center",
                            div { class: "max-w-sm px-6",
                                span { class: "mx-auto mb-3 grid size-11 place-items-center rounded-full bg-success/12 text-success",
                                    Icon { icon: AppIcon::Check, size: 22 }
                                }
                                h3 { class: "font-semibold text-foreground", "Empty project created" }
                                p { class: "mt-1 text-sm text-muted-foreground",
                                    "The workspace is registered and ready for files."
                                }
                            }
                        }
                    }
                    div { class: "mt-4 flex justify-end gap-2",
                        Button {
                            label: "Back to home",
                            kind: ButtonKind::Ghost,
                            onclick: move |_| {
                                if setup_result().is_none() {
                                    on_notice.call("Project setup continues in its terminal session".into());
                                }
                                dialog.set(HomeDialog::None);
                            },
                        }
                        Button {
                            label: "Open project",
                            kind: ButtonKind::Primary,
                            onclick: {
                                let slug = created.slug.clone();
                                move |_| {
                                    navigator
                                        .push(Route::Files {
                                            slug: slug.clone(),
                                            query: crate::files::FilesQuery::default(),
                                        });
                                }
                            },
                        }
                    }
                }
            } else {
                DialogForm {
                    Field {
                        control_id: "new-project-path",
                        label: "Project name",
                        error: match request() {
                            RequestState::Error(message) => Some(message.to_owned()),
                            _ => path_error,
                        },
                        TextInput {
                            value: project_path(),
                            placeholder: "MyAwesomeIdea",
                            autofocus: true,
                            disabled: pending,
                            oninput: move |event: FormEvent| {
                                project_path.set(event.value());
                                request.set(RequestState::Idle);
                            },
                        }
                    }
                    fieldset { disabled: pending,
                        legend { class: "mb-2 w-full",
                            span { class: "flex items-end justify-between gap-3",
                                span { class: "text-[11px] font-semibold tracking-wide text-muted-foreground uppercase",
                                    "Start from"
                                }
                                span { class: "text-[10px] text-muted-foreground",
                                    "{TEMPLATES.len()} starters"
                                }
                            }
                        }
                        TextInput {
                            value: filter.clone(),
                            placeholder: "Filter frameworks and runtimes…",
                            disabled: pending,
                            oninput: move |event: FormEvent| template_filter.set(event.value()),
                        }
                        div { class: "mt-3 max-h-[min(25rem,44svh)] space-y-4 overflow-y-auto pr-1",
                            for category in CATEGORIES {
                                if category_has_matches(category, &filter) {
                                    section {
                                        h3 { class: "mb-1.5 text-[10px] font-semibold tracking-wide text-muted-foreground uppercase",
                                            {category.label()}
                                        }
                                        div { class: "grid grid-cols-4 gap-2 max-md:grid-cols-2",
                                            for template in TEMPLATES {
                                                if template.category == category && template_matches(&template, &filter) {
                                                    TemplateButton {
                                                        template,
                                                        active: selected() == template.template,
                                                        on_select: move |template| {
                                                            selected.set(template);
                                                            request.set(RequestState::Idle);
                                                        },
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    DialogActions {
                        Button {
                            label: "Cancel",
                            kind: ButtonKind::Ghost,
                            disabled: pending,
                            onclick: move |_| dialog.set(HomeDialog::None),
                        }
                        Button {
                            label: if pending { "Creating…" } else { "Create project" },
                            kind: ButtonKind::Primary,
                            disabled: pending || project_path().trim().is_empty()
                                || validate_project_path(&project_path()).is_some(),
                            onclick: move |_| {
                                let path = project_path().trim().to_owned();
                                if validate_project_path(&path).is_some() {
                                    request.set(RequestState::Error(PROJECT_PATH_ERROR));
                                    return;
                                }
                                request.set(RequestState::Pending);
                                spawn(async move {
                                    match api::create_project(path).await {
                                        Ok(created) => {
                                            if definition.command.is_none() {
                                                setup_result.set(Some(true));
                                            }
                                            workspace.set(Some(created));
                                            request.set(RequestState::Idle);
                                            on_changed.call(());
                                        }
                                        Err(error) => {
                                            request.set(RequestState::Error(project_error_message(&error)));
                                        }
                                    }
                                });
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TemplateButton(
    template: TemplateDefinition,
    active: bool,
    on_select: EventHandler<ProjectTemplate>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: if active { "flex min-w-0 items-center gap-2.5 rounded-lg border border-primary bg-primary/8 p-3 text-left ring-1 ring-primary/20" } else { "flex min-w-0 items-center gap-2.5 rounded-lg border border-border bg-card p-3 text-left transition-colors hover:border-primary/50 hover:bg-accent" },
            "aria-pressed": active,
            onclick: move |_| on_select.call(template.template),
            span { class: "grid size-8 shrink-0 place-items-center",
                TemplateIcon { icon: template.icon }
            }
            span { class: "min-w-0",
                strong { class: "block truncate text-xs text-foreground", {template.label} }
                small { class: "block truncate text-[10px] text-muted-foreground",
                    {template.description}
                }
            }
        }
    }
}

fn template_definition(template: ProjectTemplate) -> TemplateDefinition {
    TEMPLATES
        .into_iter()
        .find(|definition| definition.template == template)
        .unwrap_or(TEMPLATES[0])
}

fn template_matches(template: &TemplateDefinition, query: &str) -> bool {
    let query = query.trim().to_ascii_lowercase();
    query.is_empty()
        || template.label.to_ascii_lowercase().contains(&query)
        || template.description.to_ascii_lowercase().contains(&query)
        || template
            .category
            .label()
            .to_ascii_lowercase()
            .contains(&query)
}

fn category_has_matches(category: TemplateCategory, query: &str) -> bool {
    TEMPLATES
        .iter()
        .any(|template| template.category == category && template_matches(template, query))
}

fn validate_project_path(path: &str) -> Option<String> {
    let path = path.trim();
    let relative = path.strip_prefix('/').unwrap_or(path);
    (relative.is_empty()
        || path.starts_with("//")
        || path.contains('\\')
        || relative.split('/').any(|component| {
            component.is_empty()
                || matches!(component, "." | "..")
                || component.len() > 255
                || component.chars().any(char::is_control)
        }))
    .then(|| PROJECT_PATH_ERROR.to_owned())
}

fn project_error_message(error: &ServerFnError) -> &'static str {
    match error {
        ServerFnError::ServerError { code: 409, .. } => {
            "A file or folder already exists at that project path."
        }
        ServerFnError::ServerError { code: 403, .. } => {
            "The runtime cannot create a project in that location."
        }
        ServerFnError::ServerError { code: 400, .. } => PROJECT_PATH_ERROR,
        _ => "The project could not be created. Check the runtime and try again.",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{template_definition, validate_project_path, ProjectTemplate, TEMPLATES};

    #[test]
    fn project_names_accept_subpaths_but_not_escape_components() {
        assert!(validate_project_path("testing/MyAwesomeIdea").is_none());
        assert!(validate_project_path("/testing/MyAwesomeIdea").is_none());
        assert!(validate_project_path("../outside").is_some());
        assert!(validate_project_path("testing//idea").is_some());
        assert!(validate_project_path("testing\\idea").is_some());
    }

    #[test]
    fn starter_catalog_is_unique_and_includes_interactive_generators() {
        assert_eq!(TEMPLATES.len(), 30);
        assert_eq!(
            TEMPLATES
                .iter()
                .map(|template| template.label)
                .collect::<HashSet<_>>()
                .len(),
            TEMPLATES.len()
        );

        let vite_plus = template_definition(ProjectTemplate::VitePlus)
            .command
            .expect("Vite+ should have an initializer");
        let tanstack = template_definition(ProjectTemplate::TanstackStart)
            .command
            .expect("TanStack Start should have an initializer");
        let cloudflare = template_definition(ProjectTemplate::Cloudflare)
            .command
            .expect("Cloudflare should have an initializer");
        let shadcn = template_definition(ProjectTemplate::Shadcn)
            .command
            .expect("shadcn/ui should have an initializer");
        let expo = template_definition(ProjectTemplate::Expo)
            .command
            .expect("React Native should have an initializer");
        let dioxus = template_definition(ProjectTemplate::Dioxus)
            .command
            .expect("Dioxus should have an initializer");
        let asp_net = template_definition(ProjectTemplate::AspNetApi)
            .command
            .expect("ASP.NET Core should have an initializer");
        let aspire = template_definition(ProjectTemplate::Aspire)
            .command
            .expect("Aspire should have an initializer");

        assert!(vite_plus.contains("vp create --directory ."));
        assert!(tanstack.contains("@tanstack/cli@latest create ."));
        assert!(cloudflare.contains("npm create cloudflare@latest -- ."));
        assert!(shadcn.contains("shadcn@latest init"));
        assert!(shadcn.contains("--name \"$project_name\""));
        assert!(shadcn.contains("cp -a -- \"$project_name\"/. ."));
        assert!(expo.contains("create-expo-app@latest ."));
        assert!(!dioxus.contains("dx new . --yes"));
        assert!(asp_net.contains("dotnet new webapi --output ."));
        assert!(aspire.contains("aspire new aspire-starter"));
    }
}
