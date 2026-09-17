use super::*;

#[component]
pub(crate) fn BranchDialog(
    action: GitDialog,
    current_branch: String,
    branches: Vec<BranchInfo>,
    initial_name: Option<String>,
    start_point: Option<String>,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_submit: EventHandler<String>,
) -> Element {
    let initial = if let Some(initial_name) = initial_name {
        initial_name
    } else if action == GitDialog::RenameBranch {
        current_branch.clone()
    } else if action == GitDialog::DeleteBranch {
        branches
            .iter()
            .find(|branch| !branch.current && !branch.remote)
            .map(|branch| branch.name.clone())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let mut name = use_signal(|| initial);
    let (title, description, confirm) = match action {
        GitDialog::CreateBranch => (
            "Create branch",
            "Create a branch at HEAD and switch to it.",
            "Create branch",
        ),
        GitDialog::RenameBranch => (
            "Rename branch",
            "Rename the current local branch.",
            "Rename",
        ),
        GitDialog::DeleteBranch => (
            "Delete branch",
            "Delete a fully merged local branch.",
            "Delete branch",
        ),
        _ => (
            "Branch action",
            "Update this repository branch.",
            "Continue",
        ),
    };
    let description = if action == GitDialog::CreateBranch {
        start_point.as_ref().map_or_else(
            || description.to_owned(),
            |start_point| format!("Create a branch from {start_point} and switch to it."),
        )
    } else {
        description.to_owned()
    };
    rsx! {
        Modal { title, description, on_close,
            DialogForm {
                Field { control_id: "branch-name", label: "Branch name",
                    if action == GitDialog::DeleteBranch {
                        select {
                            id: "branch-name",
                            class: "h-9 w-full rounded-md border border-input bg-background px-2 text-xs",
                            value: name(),
                            disabled: pending,
                            onchange: move |event| name.set(event.value()),
                            for branch in branches {
                                if !branch.current && !branch.remote {
                                    option { value: branch.name.clone(), "{branch.name}" }
                                }
                            }
                        }
                    } else {
                        TextInput {
                            value: name(),
                            autofocus: true,
                            disabled: pending,
                            oninput: move |event: FormEvent| name.set(event.value()),
                        }
                    }
                }
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Working…" } else { confirm },
                        kind: if action == GitDialog::DeleteBranch { ButtonKind::Danger } else { ButtonKind::Primary },
                        disabled: pending || name().trim().is_empty(),
                        onclick: move |_| on_submit.call(name()),
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn TagDialog(
    tags: Vec<TagInfo>,
    target: Option<String>,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_create: EventHandler<TagRequest>,
    on_delete: EventHandler<String>,
) -> Element {
    let mut name = use_signal(String::new);
    let mut message = use_signal(String::new);
    let mut delete_target = use_signal(|| None::<String>);
    rsx! {
        Modal {
            title: "Repository tags",
            description: target
                .as_ref()
                .map_or_else(
                    || "Create a tag at HEAD or remove an existing local tag.".to_owned(),
                    |target| format!("Create a tag at {target} or remove an existing local tag."),
                ),
            on_close,
            DialogForm {
                if !tags.is_empty() {
                    div { class: "max-h-44 space-y-1 overflow-y-auto rounded-md border border-border p-1",
                        for tag in tags {
                            div { class: "flex min-h-9 items-center justify-between gap-3 rounded px-2 text-xs hover:bg-accent/60",
                                span { class: "min-w-0",
                                    strong { class: "block truncate font-medium", "{tag.name}" }
                                    small { class: "block truncate font-mono text-[9px] text-muted-foreground",
                                        if tag.annotated {
                                            "annotated · "
                                        } else {
                                            "lightweight · "
                                        }
                                        "{short_oid(&tag.target_oid)}"
                                    }
                                }
                                if delete_target().as_deref() == Some(tag.name.as_str()) {
                                    div { class: "flex shrink-0 gap-1",
                                        Button {
                                            label: "Cancel",
                                            kind: ButtonKind::Ghost,
                                            size: ControlSize::Small,
                                            disabled: pending,
                                            onclick: move |_| delete_target.set(None),
                                        }
                                        Button {
                                            label: "Confirm delete",
                                            kind: ButtonKind::Danger,
                                            size: ControlSize::Small,
                                            disabled: pending,
                                            onclick: {
                                                let name = tag.name.clone();
                                                move |_| on_delete.call(name.clone())
                                            },
                                        }
                                    }
                                } else {
                                    Button {
                                        label: "Delete",
                                        kind: ButtonKind::Ghost,
                                        size: ControlSize::Small,
                                        disabled: pending,
                                        onclick: {
                                            let name = tag.name.clone();
                                            move |_| delete_target.set(Some(name.clone()))
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
                Field { control_id: "tag-name", label: "New tag name",
                    TextInput {
                        value: name(),
                        autofocus: true,
                        disabled: pending,
                        placeholder: "v1.0.0",
                        oninput: move |event: FormEvent| name.set(event.value()),
                    }
                }
                Field { control_id: "tag-message", label: "Annotation (optional)",
                    TextArea {
                        rows: 3,
                        value: message(),
                        disabled: pending,
                        placeholder: "Leave empty for a lightweight tag",
                        oninput: move |event: FormEvent| message.set(event.value()),
                    }
                }
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Close",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Working…" } else { "Create tag" },
                        kind: ButtonKind::Primary,
                        disabled: pending || name().trim().is_empty(),
                        onclick: move |_| {
                            let annotation = message();
                            on_create
                                .call(TagRequest {
                                    name: name(),
                                    target: target.clone(),
                                    message: (!annotation.trim().is_empty()).then_some(annotation),
                                });
                        },
                    }
                }
            }
        }
    }
}
