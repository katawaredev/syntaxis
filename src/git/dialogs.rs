#[allow(
    unused_imports,
    reason = "Dioxus expands the parent glob for RSX hot-reload analysis"
)]
use super::{
    api, component, dioxus_core, dioxus_elements, dioxus_signals, display_remote_url,
    remote_request, rsx, short_oid, spawn, use_signal, AExtension, ActionCallback, AnyStorage,
    AreaExtension, BaseExtension, BranchComparison, BranchInfo, Button, ButtonExtension,
    ButtonKind, Checkbox, CommitInfo, CommitRequest, ControlSize, DataExtension, DialogActions,
    DialogForm, Element, EventHandler, Field, FieldsetExtension, FormEvent, FormExtension,
    GitDialog, GlobalAttributesExtension, HasFormData, HasKeyboardData, History, IframeExtension,
    InputExtension, Key, KeyboardEvent, LiExtension, LinkExtension, MapExtension, MetaExtension,
    MeterExtension, Modal, ObjectExtension, OptgroupExtension, OptionExtension, OutputExtension,
    ParamExtension, ProgressExtension, Props, RawPatch, ReadableExt, ReadableHashMapExt,
    ReadableHashSetExt, ReadableOptionExt, ReadableResultExt, ReadableStrExt, ReadableVecExt,
    RemoteInfo, RemoteRequest, SelectExtension, SlotExtension, Storage, StyleExtension,
    SvgAttributesExtension, TagInfo, TagRequest, TextArea, TextInput, TextInputType,
    TextareaExtension, TrackExtension, WritableExt,
};

#[component]
pub(super) fn RemoteDialog(
    remote: Option<RemoteInfo>,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_submit: EventHandler<RemoteRequest>,
) -> Element {
    let editing = remote.is_some();
    let initial_name = remote
        .as_ref()
        .map_or_else(|| "origin".into(), |remote| remote.name.clone());
    let initial_fetch_url = remote
        .as_ref()
        .map(|remote| remote.fetch_url.clone())
        .unwrap_or_default();
    let initial_push_url = remote.as_ref().map_or_else(String::new, |remote| {
        if remote.push_url == remote.fetch_url {
            String::new()
        } else {
            remote.push_url.clone()
        }
    });
    let mut name = use_signal(|| initial_name);
    let mut fetch_url = use_signal(|| initial_fetch_url);
    let mut push_url = use_signal(|| initial_push_url);
    rsx! {
        Modal {
            title: if editing { "Edit remote" } else { "Add remote" },
            description: if editing { "Rename the remote or update its fetch and push URLs." } else { "Add a named Git remote. The push URL defaults to the fetch URL." },
            on_close,
            DialogForm {
                Field { control_id: "remote-name", label: "Name",
                    TextInput {
                        value: name(),
                        autofocus: true,
                        disabled: pending,
                        placeholder: "origin",
                        oninput: move |event: FormEvent| name.set(event.value()),
                    }
                }
                Field { control_id: "remote-fetch-url", label: "Fetch URL",
                    TextInput {
                        value: fetch_url(),
                        disabled: pending,
                        placeholder: "https://example.com/owner/repository.git",
                        oninput: move |event: FormEvent| fetch_url.set(event.value()),
                    }
                }
                Field {
                    control_id: "remote-push-url",
                    label: "Push URL (optional)",
                    TextInput {
                        value: push_url(),
                        disabled: pending,
                        placeholder: "Uses the fetch URL when empty",
                        oninput: move |event: FormEvent| push_url.set(event.value()),
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
                        label: if pending { "Saving…" } else if editing { "Save remote" } else { "Add remote" },
                        kind: ButtonKind::Primary,
                        disabled: pending || name().trim().is_empty() || fetch_url().trim().is_empty(),
                        onclick: move |_| {
                            let push = push_url();
                            on_submit
                                .call(
                                    remote_request(
                                        name().trim().to_owned(),
                                        fetch_url().trim().to_owned(),
                                        (!push.trim().is_empty()).then(|| push.trim().to_owned()),
                                    ),
                                );
                        },
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn RemoveRemoteDialog(
    remote: RemoteInfo,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Remove remote?",
            description: "This removes the remote configuration and its remote-tracking branches. It does not delete the remote repository.",
            on_close,
            DialogForm {
                div { class: "rounded-md border border-border bg-secondary/50 p-3",
                    strong { class: "block text-xs", "{remote.name}" }
                    small { class: "mt-1 block truncate text-[10px] text-muted-foreground",
                        {display_remote_url(&remote.fetch_url)}
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
                        label: if pending { "Removing…" } else { "Remove remote" },
                        kind: ButtonKind::Danger,
                        disabled: pending,
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn BranchDialog(
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
pub(super) fn TagDialog(
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

#[component]
pub(super) fn CompareMergeDialog(
    current_branch: String,
    branches: Vec<BranchInfo>,
    initial_target: Option<String>,
    comparison: Option<BranchComparison>,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_compare: EventHandler<String>,
    on_merge: EventHandler<String>,
) -> Element {
    let default_target = initial_target
        .filter(|name| branches.iter().any(|branch| branch.name == *name))
        .or_else(|| {
            branches
                .iter()
                .find(|branch| branch.name != current_branch)
                .map(|branch| branch.name.clone())
        })
        .unwrap_or_default();
    let mut target = use_signal(|| default_target);
    let comparison_matches = comparison
        .as_ref()
        .is_some_and(|value| value.base == current_branch && value.head == target());
    rsx! {
        Modal {
            title: "Compare and merge branch",
            description: "Review commits and the Git-generated three-dot diff before merging into the current branch.",
            on_close,
            DialogForm {
                div { class: "grid grid-cols-[minmax(0,1fr)_auto] items-end gap-2",
                    Field {
                        control_id: "compare-branch",
                        label: "Compare with {current_branch}",
                        select {
                            id: "compare-branch",
                            class: "h-9 w-full rounded-md border border-input bg-background px-2 text-xs",
                            value: target(),
                            disabled: pending,
                            onchange: move |event| target.set(event.value()),
                            for branch in branches {
                                if branch.name != current_branch {
                                    option { value: branch.name.clone(), "{branch.name}" }
                                }
                            }
                        }
                    }
                    Button {
                        label: if pending { "Loading…" } else { "Compare" },
                        kind: ButtonKind::Ghost,
                        disabled: pending || target().is_empty(),
                        onclick: move |_| on_compare.call(target()),
                    }
                }
                if let Some(value) = comparison {
                    div { class: "space-y-2 rounded-md border border-border",
                        div { class: "flex flex-wrap gap-3 border-b border-border px-3 py-2 text-[10px] text-muted-foreground",
                            span { "{value.base_only_commits} only on {value.base}" }
                            span { "{value.head_only_commits} only on {value.head}" }
                            span { "{value.files_changed} files" }
                            span { class: "text-success", "+{value.additions}" }
                            span { class: "text-destructive", "−{value.deletions}" }
                        }
                        if !value.commits.is_empty() {
                            div { class: "max-h-24 overflow-y-auto px-3 text-[10px]",
                                for commit in value.commits {
                                    p { class: "truncate py-1",
                                        code { class: "mr-2 text-primary", "{commit.short_oid}" }
                                        "{commit.subject}"
                                    }
                                }
                            }
                        }
                        if value.patch.is_empty() {
                            p { class: "px-3 pb-3 text-xs text-muted-foreground",
                                "No file differences to display."
                            }
                        } else {
                            div { class: "max-h-64 overflow-auto border-t border-border",
                                RawPatch { patch: value.patch }
                            }
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
                        label: if pending { "Working…" } else { "Merge into current branch" },
                        kind: ButtonKind::Primary,
                        disabled: pending || !comparison_matches,
                        onclick: move |_| on_merge.call(target()),
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn AbortMergeDialog(
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Abort merge?",
            description: "Restore the index and working tree to their state before the current merge began.",
            on_close,
            DialogForm {
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
                        label: if pending { "Aborting…" } else { "Abort merge" },
                        kind: ButtonKind::Danger,
                        disabled: pending,
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn DiscardAllDialog(
    changed_files: usize,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Discard all changes?",
            description: "Restore every staged and unstaged file to HEAD and remove untracked files. This cannot be undone.",
            on_close,
            DialogForm {
                p { class: "rounded-md border border-destructive/30 bg-destructive/10 p-3 text-xs text-destructive",
                    "{changed_files} changed file(s) will be discarded."
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
                        label: if pending { "Discarding…" } else { "Discard all changes" },
                        kind: ButtonKind::Danger,
                        disabled: pending || changed_files == 0,
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn CommitHistoryActionDialog(
    action: GitDialog,
    commit: CommitInfo,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    let checkout = action == GitDialog::CheckoutCommit;
    rsx! {
        Modal {
            title: if checkout { "Checkout commit?" } else { "Revert commit?" },
            description: if checkout { "Checkout switches to this snapshot in detached HEAD mode. Create a branch before committing new work." } else { "Revert creates a new commit that reverses this commit. Git may stop for conflict resolution." },
            on_close,
            DialogForm {
                div { class: "rounded-md border border-border bg-secondary/50 p-3",
                    p { class: "truncate text-xs font-medium", "{commit.subject}" }
                    p { class: "mt-1 font-mono text-[9px] text-muted-foreground",
                        "{commit.short_oid}"
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
                        label: if pending { "Working…" } else if checkout { "Checkout commit" } else { "Create revert commit" },
                        kind: if checkout { ButtonKind::Primary } else { ButtonKind::Danger },
                        disabled: pending,
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn ForcePushDialog(
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    let mut acknowledged = use_signal(|| false);
    rsx! {
        Modal {
            title: "Force push with lease?",
            description: "The remote rejected the normal push as non-fast-forward. The lease prevents replacing commits you have not fetched.",
            on_close,
            DialogForm {
                label { class: "flex items-center gap-2.5 text-xs",
                    Checkbox {
                        checked: acknowledged(),
                        aria_label: "Confirm force push with lease",
                        disabled: pending,
                        on_checked_change: move |checked| acknowledged.set(checked),
                    }
                    span { "I understand that remote commits may be replaced." }
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
                        label: if pending { "Pushing…" } else { "Force push with lease" },
                        kind: ButtonKind::Danger,
                        disabled: pending || !acknowledged(),
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn CommitDialog(
    workspace_slug: String,
    initial_message: String,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_submit: EventHandler<CommitRequest>,
) -> Element {
    let mut message = use_signal(|| initial_message);
    let mut amend = use_signal(|| false);
    rsx! {
        Modal {
            title: if amend() { "Amend previous commit" } else { "Commit staged changes" },
            description: "Git will use the configured identity, hooks, and signing settings.",
            on_close,
            DialogForm {
                Field { control_id: "commit-message", label: "Commit message",
                    TextArea {
                        rows: 4,
                        value: message(),
                        placeholder: "Describe your changes",
                        autofocus: true,
                        disabled: pending,
                        oninput: move |event: FormEvent| message.set(event.value()),
                    }
                }
                label { class: "compact flex items-center gap-2.5 py-1.75",
                    Checkbox {
                        checked: amend(),
                        aria_label: "Amend previous commit",
                        disabled: pending,
                        on_checked_change: move |checked| {
                            amend.set(checked);
                            if checked && message().trim().is_empty() {
                                let slug = workspace_slug.clone();
                                spawn(async move {
                                    let previous_message =
                                        api::commit_message(slug, "HEAD".into()).await;
                                    if let Ok(previous_message) = previous_message {
                                        if amend() && message().trim().is_empty() {
                                            message.set(previous_message);
                                        }
                                    }
                                });
                            }
                        },
                    }
                    span { "Amend previous commit" }
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
                        label: if pending { "Committing…" } else { "Commit" },
                        kind: ButtonKind::Primary,
                        disabled: pending || message().trim().is_empty(),
                        onclick: move |_| {
                            on_submit
                                .call(CommitRequest {
                                    message: message(),
                                    amend: amend(),
                                    signing_passphrase: None,
                                });
                        },
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn SigningDialog(
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_submit: EventHandler<String>,
) -> Element {
    let mut passphrase = use_signal(String::new);
    let submit = EventHandler::new(move |()| {
        if pending || passphrase().is_empty() {
            return;
        }
        let secret = std::mem::take(&mut *passphrase.write());
        on_submit.call(secret);
    });
    rsx! {
        Modal {
            title: "Signing passphrase required",
            description: "The passphrase is sent only for this commit retry and is not stored.",
            on_close,
            DialogForm {
                Field {
                    control_id: "signing-passphrase",
                    label: "Signing passphrase",
                    TextInput {
                        input_type: TextInputType::Password,
                        value: passphrase(),
                        autofocus: true,
                        disabled: pending,
                        oninput: move |event: FormEvent| passphrase.set(event.value()),
                        onkeydown: move |event: KeyboardEvent| {
                            if event.key() == Key::Enter {
                                event.prevent_default();
                                submit.call(());
                            }
                        },
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
                        label: if pending { "Signing…" } else { "Retry signed commit" },
                        kind: ButtonKind::Primary,
                        disabled: pending || passphrase().is_empty(),
                        onclick: move |_| submit.call(()),
                    }
                }
            }
        }
    }
}
