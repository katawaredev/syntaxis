use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem, DropdownMenuTrigger};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, Checkbox, ControlSize, DangerNote, DialogActions, DialogForm,
    Drawer, Field, Icon, IconButton, MenuContent, MenuTrigger, Modal, PanelHeader, PanelHeaderKind,
    Select, StatusBadge, TextArea, TextAreaResize, TextInput, TextInputType, Toast, Tone,
};

use crate::mock::{MockChange, CHANGES, CONFLICTS, STAGED};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarView {
    Changes,
    History,
    Compare,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GitDialog {
    None,
    Commit,
    CreateBranch,
    RenameBranch,
    DeleteBranch,
    CreateTag,
    CheckoutCommit,
    RevertCommit,
    SigningRetry,
    ForcePush,
    DiscardAll,
    AbortMerge,
    MergeBranch,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BranchAction {
    SwitchMain,
    SwitchWorkspace,
    SwitchRuntime,
    Compare,
    Push,
}

#[component]
pub fn Git(slug: String) -> Element {
    let _ = slug;
    let mut view = use_signal(|| SidebarView::Changes);
    let mut selected = use_signal(|| "src/workspace.rs");
    let selected_commit = use_signal(|| "d8f07a1");
    let mut drawer = use_signal(|| false);
    let mut branch_menu = use_signal(|| false);
    let mut action_menu = use_signal(|| false);
    let mut dialog = use_signal(|| GitDialog::None);
    let mut toast = use_signal(|| None::<String>);
    let mut resolved = use_signal(|| false);
    let mut current_branch = use_signal(|| "main".to_string());
    let mut change_count = use_signal(|| 6_u8);
    let mut merge_in_progress = use_signal(|| true);

    rsx! {
        div { class: "grid size-full min-h-0 grid-cols-[minmax(0,1fr)_310px] overflow-hidden max-md:block",
            section { class: "flex min-h-0 min-w-0 flex-col overflow-hidden max-md:h-full",
                PanelHeader { kind: PanelHeaderKind::Repository,
                    div { class: "flex min-w-0 items-center gap-1.5",
                        IconButton {
                            label: "Open Git sidebar",
                            icon: AppIcon::Explorer,
                            onclick: move |_| drawer.set(true),
                        }
                        div { class: "flex min-w-0 items-center gap-1.75",
                            DropdownMenu {
                                class: "relative",
                                open: branch_menu(),
                                on_open_change: move |open: bool| branch_menu.set(open),
                                DropdownMenuTrigger {
                                    class: "flex h-7.75 items-center gap-1.75 rounded-md border border-border bg-secondary px-2 text-xs hover:bg-accent",
                                    "aria-label": "Current branch: {current_branch}",
                                    Icon { icon: AppIcon::GitBranch }
                                    span { "{current_branch}" }
                                }
                                BranchMenu {
                                    current_branch: current_branch(),
                                    on_dialog: move |next| dialog.set(next),
                                    on_action: move |action| match action {
                                        BranchAction::SwitchMain => {
                                            current_branch.set("main".into());
                                            toast.set(Some("Switched to main".into()));
                                        }
                                        BranchAction::SwitchWorkspace => {
                                            current_branch.set("feature/workspace-ui".into());
                                            toast.set(Some("Switched to feature/workspace-ui".into()));
                                        }
                                        BranchAction::SwitchRuntime => {
                                            current_branch.set("fix/runtime-status".into());
                                            toast.set(Some("Switched to fix/runtime-status".into()));
                                        }
                                        BranchAction::Compare => view.set(SidebarView::Compare),
                                        BranchAction::Push => {
                                            toast.set(Some(format!("Pushed {} to origin (mock)", current_branch())));
                                        }
                                    },
                                }
                            }
                            span { class: "text-[10px] text-muted-foreground", "origin/main" }
                        }
                        StatusBadge {
                            label: format!("{} changes", change_count()),
                            tone: Tone::Warning,
                        }
                        if merge_in_progress() {
                            StatusBadge {
                                label: "Merge in progress",
                                tone: Tone::Destructive,
                            }
                        }
                    }
                    div { class: "flex min-w-0 items-center gap-1.5",
                        Button {
                            label: "Commit",
                            kind: ButtonKind::Primary,
                            onclick: move |_| dialog.set(GitDialog::Commit),
                        }
                        IconButton {
                            label: "Fetch",
                            icon: AppIcon::Fetch,
                            onclick: move |_| toast.set(Some("Fetched origin · already up to date".into())),
                        }
                        IconButton {
                            label: "Push",
                            icon: AppIcon::Push,
                            onclick: move |_| dialog.set(GitDialog::ForcePush),
                        }
                        DropdownMenu {
                            class: "relative",
                            open: action_menu(),
                            on_open_change: move |open: bool| action_menu.set(open),
                            MenuTrigger {
                                label: "Repository actions",
                                icon: AppIcon::More,
                                open: action_menu(),
                            }
                            MenuContent { class: "right-0 w-56.25",
                                GitMenuItem {
                                    action: GitDialog::CreateTag,
                                    index: 0,
                                    label: "Create tag",
                                    on_select: move |next| dialog.set(next),
                                }
                                GitMenuItem {
                                    action: GitDialog::SigningRetry,
                                    index: 1,
                                    label: "Retry signed commit",
                                    on_select: move |next| dialog.set(next),
                                }
                                GitMenuItem {
                                    action: GitDialog::CheckoutCommit,
                                    index: 2,
                                    label: "Checkout selected commit",
                                    on_select: move |next| dialog.set(next),
                                }
                                GitMenuItem {
                                    action: GitDialog::RevertCommit,
                                    index: 3,
                                    label: "Revert selected commit",
                                    on_select: move |next| dialog.set(next),
                                }
                                hr {}
                                GitMenuItem {
                                    action: GitDialog::AbortMerge,
                                    index: 4,
                                    label: "Abort merge",
                                    destructive: true,
                                    on_select: move |next| dialog.set(next),
                                }
                                GitMenuItem {
                                    action: GitDialog::DiscardAll,
                                    index: 5,
                                    label: "Discard all changes",
                                    destructive: true,
                                    on_select: move |next| dialog.set(next),
                                }
                            }
                        }
                    }
                }
                div { class: "min-h-0 min-w-0 flex-1 overflow-auto bg-background",
                    if view() == SidebarView::History {
                        CommitDetail {
                            hash: selected_commit(),
                            on_dialog: move |next| dialog.set(next),
                            on_notice: move |message| toast.set(Some(message)),
                        }
                    } else if view() == SidebarView::Compare {
                        ComparisonDetail { on_notice: move |message| toast.set(Some(message)) }
                    } else if selected() == "src/workspace.rs" && !resolved() {
                        ConflictView {
                            on_resolve: move |message| {
                                resolved.set(true);
                                toast.set(Some(message));
                            },
                        }
                    } else {
                        WorkingDiff {
                            path: selected(),
                            on_notice: move |message| toast.set(Some(message)),
                        }
                    }
                }
            }
            aside { class: "min-h-0 min-w-0 border-l border-border bg-sidebar max-md:hidden",
                GitSidebar {
                    view,
                    selected,
                    selected_commit,
                    on_select: move |path| selected.set(path),
                    on_view: move |next| view.set(next),
                    on_commit: move |()| dialog.set(GitDialog::Commit),
                    on_notice: move |message| toast.set(Some(message)),
                }
            }
            if drawer() {
                Drawer {
                    title: "Repository",
                    label: "Git repository sidebar",
                    content_class: "h-full w-[min(330px,88vw)] justify-self-start border-0 border-r border-border bg-sidebar shadow-[15px_0_50px_#0008]",
                    restore_focus: "button[aria-label='Open Git sidebar']",
                    on_close: move |()| drawer.set(false),
                    GitSidebar {
                        view,
                        selected,
                        selected_commit,
                        on_select: move |path| {
                            selected.set(path);
                            drawer.set(false);
                        },
                        on_view: move |next| view.set(next),
                        on_commit: move |()| dialog.set(GitDialog::Commit),
                        on_notice: move |message| toast.set(Some(message)),
                    }
                }
            }
        }

        if dialog() != GitDialog::None {
            GitActionDialog {
                action: dialog(),
                selected_commit: selected_commit(),
                on_close: move |()| dialog.set(GitDialog::None),
                on_submit: move |message| {
                    match dialog() {
                        GitDialog::Commit | GitDialog::SigningRetry => change_count.set(4),
                        GitDialog::CreateBranch => {
                            current_branch.set("feature/new-workspace".into());
                        }
                        GitDialog::RenameBranch => {
                            current_branch.set("feature/renamed-workspace".into());
                        }
                        GitDialog::CheckoutCommit => {
                            current_branch.set(format!("detached@{}", selected_commit()));
                        }
                        GitDialog::RevertCommit => change_count.set(change_count().saturating_add(1)),
                        GitDialog::DiscardAll => {
                            change_count.set(0);
                            resolved.set(true);
                        }
                        GitDialog::AbortMerge => {
                            merge_in_progress.set(false);
                            resolved.set(true);
                        }
                        GitDialog::MergeBranch => {
                            merge_in_progress.set(true);
                            resolved.set(false);
                            selected.set("src/workspace.rs");
                            view.set(SidebarView::Changes);
                        }
                        GitDialog::None
                        | GitDialog::DeleteBranch
                        | GitDialog::CreateTag
                        | GitDialog::ForcePush => {}
                    }
                    dialog.set(GitDialog::None);
                    toast.set(Some(message));
                },
            }
        }
        if let Some(message) = toast() {
            Toast { message, on_close: move |()| toast.set(None) }
        }
    }
}

#[component]
fn BranchMenu(
    current_branch: String,
    on_dialog: EventHandler<GitDialog>,
    on_action: EventHandler<BranchAction>,
) -> Element {
    rsx! {
        MenuContent { class: "left-0 w-58.75 max-md:fixed max-md:top-26 max-md:left-12",
            DropdownMenuItem::<BranchAction> {
                value: BranchAction::SwitchMain,
                index: 0_usize,
                class: if current_branch == "main" { "text-primary" } else { "" },
                on_select: move |action| on_action.call(action),
                span { "main" }
                if current_branch == "main" {
                    Icon { icon: AppIcon::Check, size: 14 }
                }
            }
            DropdownMenuItem::<BranchAction> {
                value: BranchAction::SwitchWorkspace,
                index: 1_usize,
                class: if current_branch == "feature/workspace-ui" { "text-primary" } else { "" },
                on_select: move |action| on_action.call(action),
                "feature/workspace-ui"
            }
            DropdownMenuItem::<BranchAction> {
                value: BranchAction::SwitchRuntime,
                index: 2_usize,
                class: if current_branch == "fix/runtime-status" { "text-primary" } else { "" },
                on_select: move |action| on_action.call(action),
                "fix/runtime-status"
            }
            hr {}
            GitMenuItem {
                action: GitDialog::CreateBranch,
                index: 3,
                label: "Create branch…",
                on_select: move |next| on_dialog.call(next),
            }
            GitMenuItem {
                action: GitDialog::RenameBranch,
                index: 4,
                label: "Rename current branch…",
                on_select: move |next| on_dialog.call(next),
            }
            GitMenuItem {
                action: GitDialog::MergeBranch,
                index: 5,
                label: "Merge branch into current…",
                on_select: move |next| on_dialog.call(next),
            }
            DropdownMenuItem::<BranchAction> {
                value: BranchAction::Compare,
                index: 6_usize,
                on_select: move |action| on_action.call(action),
                "Compare branches"
            }
            DropdownMenuItem::<BranchAction> {
                value: BranchAction::Push,
                index: 7_usize,
                on_select: move |action| on_action.call(action),
                "Push current branch"
            }
            GitMenuItem {
                action: GitDialog::DeleteBranch,
                index: 8_usize,
                label: "Delete branch…",
                destructive: true,
                on_select: move |next| on_dialog.call(next),
            }
        }
    }
}

#[component]
fn GitMenuItem(
    action: GitDialog,
    index: usize,
    label: &'static str,
    #[props(default = false)] destructive: bool,
    on_select: EventHandler<GitDialog>,
) -> Element {
    rsx! {
        DropdownMenuItem::<GitDialog> {
            value: action,
            index,
            class: if destructive { "!text-destructive" } else { "" },
            on_select: move |next| on_select.call(next),
            {label}
        }
    }
}

#[component]
fn GitSidebar(
    view: Signal<SidebarView>,
    selected: Signal<&'static str>,
    mut selected_commit: Signal<&'static str>,
    on_select: EventHandler<&'static str>,
    on_view: EventHandler<SidebarView>,
    on_commit: EventHandler<()>,
    on_notice: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "flex h-full min-h-0 flex-col",
            div {
                class: "grid h-10.5 min-h-10.5 grid-cols-3 items-center border-b border-border px-1.25 [&_button]:flex [&_button]:h-7.75 [&_button]:items-center [&_button]:justify-center [&_button]:gap-1.25 [&_button]:rounded-md [&_button]:bg-transparent [&_button]:text-[11px] [&_button]:text-muted-foreground [&_button_span]:min-w-4.25 [&_button_span]:rounded-full [&_button_span]:bg-secondary [&_button_span]:px-1 [&_button_span]:py-px [&_button_span]:text-[9px]",
                role: "tablist",
                button {
                    class: if view() == SidebarView::Changes { "!bg-accent !text-foreground" } else { "" },
                    onclick: move |_| on_view.call(SidebarView::Changes),
                    "Changes"
                    span { "6" }
                }
                button {
                    class: if view() == SidebarView::History { "!bg-accent !text-foreground" } else { "" },
                    onclick: move |_| on_view.call(SidebarView::History),
                    "History"
                }
                button {
                    class: if view() == SidebarView::Compare { "!bg-accent !text-foreground" } else { "" },
                    onclick: move |_| on_view.call(SidebarView::Compare),
                    "Compare"
                }
            }
            if view() == SidebarView::Changes {
                div { class: "min-h-0 flex-1 overflow-y-auto",
                    ChangeSection {
                        title: "CONFLICTS",
                        count: 1,
                        action: "Resolve",
                        changes: &CONFLICTS,
                        selected,
                        on_select,
                        on_notice,
                    }
                    ChangeSection {
                        title: "STAGED CHANGES",
                        count: 2,
                        action: "Unstage all",
                        changes: &STAGED,
                        selected,
                        on_select,
                        on_notice,
                    }
                    ChangeSection {
                        title: "CHANGES",
                        count: 4,
                        action: "Stage all",
                        changes: &CHANGES,
                        selected,
                        on_select,
                        on_notice,
                    }
                }
                div { class: "border-t border-border p-2.25",
                    TextArea {
                        class: "min-h-15.25",
                        size: ControlSize::Small,
                        resize: TextAreaResize::None,
                        aria_label: "Commit message",
                        placeholder: "Commit message",
                        rows: 3,
                    }
                    button {
                        class: "mt-1.5 h-7.75 w-full rounded-md bg-primary text-[11px] font-semibold text-primary-foreground hover:bg-primary/90",
                        onclick: move |_| on_commit.call(()),
                        "Commit staged changes"
                    }
                }
            } else if view() == SidebarView::History {
                div { class: "min-h-0 overflow-y-auto p-1.25",
                    CommitRow {
                        hash: "d8f07a1",
                        title: "Build responsive Phase 1 shell",
                        meta: "Alex · 18 min ago",
                        active: selected_commit() == "d8f07a1",
                        onclick: move |()| selected_commit.set("d8f07a1"),
                    }
                    CommitRow {
                        hash: "9a34dc2",
                        title: "Add workspace runtime protocol",
                        meta: "Alex · yesterday",
                        active: selected_commit() == "9a34dc2",
                        onclick: move |()| selected_commit.set("9a34dc2"),
                    }
                    CommitRow {
                        hash: "73ae118",
                        title: "Refine Git conflict parser",
                        meta: "Mina · 2 days ago",
                        active: selected_commit() == "73ae118",
                        onclick: move |()| selected_commit.set("73ae118"),
                    }
                    CommitRow {
                        hash: "14d830e",
                        title: "Initialize Dioxus project",
                        meta: "Alex · last week",
                        active: selected_commit() == "14d830e",
                        onclick: move |()| selected_commit.set("14d830e"),
                    }
                }
            } else {
                div { class: "flex flex-col gap-2 px-3 py-3.5",
                    Field { control_id: "base-branch", label: "Base",
                        Select { size: ControlSize::Small,
                            option { "main" }
                            option { "origin/main" }
                        }
                    }
                    div { class: "pt-2 text-center text-muted-foreground", "↓" }
                    Field { control_id: "compare-branch", label: "Compare",
                        Select { size: ControlSize::Small,
                            option { "feature/workspace-ui" }
                            option { "fix/runtime-status" }
                        }
                    }
                    button {
                        class: "mt-1.5 h-7.75 w-full rounded-md bg-primary text-[11px] font-semibold text-primary-foreground hover:bg-primary/90",
                        onclick: move |_| on_notice.call("Comparison refreshed".into()),
                        "Compare branches"
                    }
                    div { class: "mt-3.5 rounded-md border border-border bg-card p-2.5",
                        strong { class: "text-[11px]", "feature/workspace-ui" }
                        p { class: "mt-1 text-[10px] text-muted-foreground",
                            "+428 −96 across 12 files"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ChangeSection(
    title: &'static str,
    count: u8,
    action: &'static str,
    changes: &'static [MockChange],
    selected: Signal<&'static str>,
    on_select: EventHandler<&'static str>,
    on_notice: EventHandler<String>,
) -> Element {
    rsx! {
        section { class: "border-b border-border",
            header { class: "flex h-7.75 items-center justify-between px-2 pr-2 pl-2.75 text-[9px] font-bold tracking-[0.05em] text-muted-foreground",
                span { "{title} ({count})" }
                button {
                    class: "bg-transparent p-1 text-[9px] font-medium tracking-normal text-muted-foreground hover:text-primary",
                    onclick: move |_| on_notice.call(format!("{action} completed (mock)")),
                    {action}
                }
            }
            for change in changes {
                button {
                    class: if selected() == change.path { "grid min-h-9.75 w-full grid-cols-[16px_minmax(0,1fr)_15px] items-center gap-1.5 bg-accent px-2.25 py-1 text-left text-foreground" } else { "grid min-h-9.75 w-full grid-cols-[16px_minmax(0,1fr)_15px] items-center gap-1.5 bg-transparent px-2.25 py-1 text-left text-foreground hover:bg-accent/65" },
                    onclick: move |_| on_select.call(change.path),
                    span { class: "text-primary", "◇" }
                    span { class: "min-w-0",
                        strong { class: "block truncate text-[11px] font-medium",
                            {change.path.rsplit('/').next().unwrap_or(change.path)}
                        }
                        small { class: "mt-px block truncate text-[8px] text-muted-foreground",
                            {change.path}
                        }
                    }
                    span { class: change.kind.class(), {change.kind.label()} }
                }
            }
        }
    }
}

#[component]
fn CommitRow(
    hash: &'static str,
    title: &'static str,
    meta: &'static str,
    active: bool,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            class: if active { "relative flex w-full gap-2.25 rounded-md bg-accent py-2.25 pr-1.75 pl-6 text-left before:absolute before:top-0 before:bottom-0 before:left-3.25 before:w-px before:bg-border" } else { "relative flex w-full gap-2.25 rounded-md bg-transparent py-2.25 pr-1.75 pl-6 text-left before:absolute before:top-0 before:bottom-0 before:left-3.25 before:w-px before:bg-border hover:bg-accent" },
            onclick: move |_| onclick.call(()),
            span { class: "absolute top-4 left-2.5 size-1.75 rounded-full border-2 border-primary bg-sidebar" }
            span {
                strong { class: "block text-[11px] leading-snug", {title} }
                small { class: "mt-1 block font-mono text-[8px] text-muted-foreground",
                    "{hash} · {meta}"
                }
            }
        }
    }
}

#[component]
fn ConflictView(on_resolve: EventHandler<String>) -> Element {
    rsx! {
        div { class: "conflict-view",
            div { class: "diff-titlebar",
                div {
                    strong { "src/workspace.rs" }
                    StatusBadge { label: "Conflicted", tone: Tone::Destructive }
                }
                div {
                    Button {
                        label: "Accept incoming",
                        size: ControlSize::Small,
                        onclick: move |_| on_resolve.call("Accepted incoming changes".into()),
                    }
                    Button {
                        label: "Keep current",
                        size: ControlSize::Small,
                        onclick: move |_| on_resolve.call("Kept current changes".into()),
                    }
                    Button {
                        label: "Merge both",
                        kind: ButtonKind::Primary,
                        size: ControlSize::Small,
                        onclick: move |_| on_resolve.call("Merged both conflict blocks".into()),
                    }
                }
            }
            div { class: "conflict-columns",
                section {
                    header {
                        span { "CURRENT · main" }
                        button { onclick: move |_| on_resolve.call("Current block accepted".into()),
                            "Accept current"
                        }
                    }
                    pre {
                        code { "pub fn runtime_label() -> &'static str {{\n    \"Connected runtime\"\n}}" }
                    }
                }
                section {
                    header {
                        span { "INCOMING · feature/workspace-ui" }
                        button { onclick: move |_| on_resolve.call("Incoming block accepted".into()),
                            "Accept incoming"
                        }
                    }
                    pre {
                        code {
                            "pub fn runtime_label(ready: bool) -> String {{\n    format!(\"Runtime: {{ready}}\")\n}}"
                        }
                    }
                }
            }
            div { class: "conflict-result",
                header {
                    span { "RESULT" }
                    button { onclick: move |_| on_resolve.call("Conflict block merged".into()),
                        "Merge both"
                    }
                }
                pre {
                    code {
                        "pub fn runtime_label(ready: bool) -> String {{\n    if ready {{ \"Connected runtime\".into() }} else {{ \"Offline\".into() }}\n}}"
                    }
                }
            }
        }
    }
}

#[component]
fn WorkingDiff(path: &'static str, on_notice: EventHandler<String>) -> Element {
    rsx! {
        div { class: "working-diff",
            div { class: "diff-titlebar",
                div {
                    strong { {path} }
                    span { class: "diff-stats", "+12 −4" }
                }
                div {
                    Button {
                        label: "Discard",
                        kind: ButtonKind::Ghost,
                        size: ControlSize::Small,
                        onclick: move |_| on_notice.call("File changes discarded (mock)".into()),
                    }
                    Button {
                        label: "Stage file",
                        kind: ButtonKind::Primary,
                        size: ControlSize::Small,
                        onclick: move |_| on_notice.call("File staged".into()),
                    }
                }
            }
            div { class: "hunk-header",
                span { "@@ -42,8 +42,16 @@ fn Explorer" }
                button { onclick: move |_| on_notice.call("Hunk staged".into()), "Stage hunk" }
            }
            div { class: "unified-diff",
                div { class: "diff-line context",
                    span { "42" }
                    code { "  let mut search = use_signal(|| false);" }
                }
                div { class: "diff-line removed",
                    span { "43" }
                    code { "- let files = mock_files();" }
                }
                div { class: "diff-line added",
                    span { "43" }
                    code { "+ let mut git_filter = use_signal(|| false);" }
                }
                div { class: "diff-line added",
                    span { "44" }
                    code { "+ let mut drawer = use_signal(|| false);" }
                }
                div { class: "diff-line context",
                    span { "45" }
                    code { "  rsx! {{" }
                }
                div { class: "diff-line added",
                    span { "46" }
                    code { "+   aside {{ class: \"explorer drawer\", ... }}" }
                }
                div { class: "diff-line context",
                    span { "47" }
                    code { "  }}" }
                }
            }
        }
    }
}

#[component]
fn CommitDetail(
    hash: &'static str,
    on_dialog: EventHandler<GitDialog>,
    on_notice: EventHandler<String>,
) -> Element {
    let (title, author, age, parent, files, additions, deletions, path) = match hash {
        "9a34dc2" => (
            "Add workspace runtime protocol",
            "Alex Morgan",
            "yesterday",
            "73ae118",
            7,
            186,
            31,
            "src/mock.rs",
        ),
        "73ae118" => (
            "Refine Git conflict parser",
            "Mina Lee",
            "2 days ago",
            "14d830e",
            5,
            94,
            28,
            "src/git.rs",
        ),
        "14d830e" => (
            "Initialize Dioxus project",
            "Alex Morgan",
            "last week",
            "root",
            18,
            612,
            0,
            "Cargo.toml",
        ),
        _ => (
            "Build responsive Phase 1 shell",
            "Alex Morgan",
            "18 minutes ago",
            "9a34dc2",
            12,
            428,
            96,
            "src/workspace.rs",
        ),
    };
    rsx! {
        div { class: "commit-detail",
            header { class: "commit-detail-header",
                div {
                    p { class: "eyebrow", "COMMIT {hash}" }
                    h2 { "{title}" }
                    p { "{author} committed {age}" }
                }
                div {
                    Button {
                        label: "Checkout",
                        size: ControlSize::Small,
                        onclick: move |_| on_dialog.call(GitDialog::CheckoutCommit),
                    }
                    Button {
                        label: "Revert",
                        kind: ButtonKind::Ghost,
                        size: ControlSize::Small,
                        onclick: move |_| on_dialog.call(GitDialog::RevertCommit),
                    }
                }
            }
            div { class: "commit-metadata",
                span { "Parent {parent}" }
                span { "{files} files changed" }
                span { "+{additions} −{deletions}" }
            }
            WorkingDiff {
                path,
                on_notice: move |action| on_notice.call(format!("{action} in commit {hash}")),
            }
        }
    }
}

#[component]
fn ComparisonDetail(on_notice: EventHandler<String>) -> Element {
    rsx! {
        div { class: "comparison-detail",
            header {
                p { class: "eyebrow", "BRANCH COMPARISON" }
                h2 { "main ← feature/workspace-ui" }
                p { "4 commits ahead, 1 commit behind" }
            }
            div { class: "comparison-summary",
                span {
                    strong { "12" }
                    " files changed"
                }
                span { class: "text-success", "+428" }
                span { class: "text-destructive", "−96" }
            }
            WorkingDiff {
                path: "src/workspace.rs",
                on_notice: move |action| on_notice.call(format!("{action} in branch comparison")),
            }
        }
    }
}

#[component]
fn GitActionDialog(
    action: GitDialog,
    selected_commit: String,
    on_close: EventHandler<()>,
    on_submit: EventHandler<String>,
) -> Element {
    let mut amend = use_signal(|| false);
    let mut force_push_acknowledged = use_signal(|| false);
    let (title, description, confirm, dangerous) = match action {
        GitDialog::Commit => (
            "Commit staged changes",
            "Create a commit from 2 staged files.".into(),
            "Commit",
            false,
        ),
        GitDialog::CreateBranch => (
            "Create branch",
            "Create and switch to a new local branch.".into(),
            "Create branch",
            false,
        ),
        GitDialog::RenameBranch => (
            "Rename branch",
            "Rename the current branch main.".into(),
            "Rename",
            false,
        ),
        GitDialog::DeleteBranch => (
            "Delete branch",
            "Delete feature/old-layout from this repository.".into(),
            "Delete branch",
            true,
        ),
        GitDialog::CreateTag => (
            "Create tag",
            format!("Create a tag at {selected_commit}."),
            "Create tag",
            false,
        ),
        GitDialog::CheckoutCommit => (
            "Checkout commit?",
            format!("This checks out {selected_commit} in detached HEAD state."),
            "Checkout",
            false,
        ),
        GitDialog::RevertCommit => (
            "Revert commit?",
            format!("Create a new commit that reverses {selected_commit}."),
            "Revert commit",
            true,
        ),
        GitDialog::SigningRetry => (
            "Signing passphrase required",
            "Git could not unlock your signing key. This secret is used once and never stored."
                .into(),
            "Retry signed commit",
            false,
        ),
        GitDialog::ForcePush => (
            "Force push with lease?",
            "The remote rejected a normal push because it is not a fast-forward update.".into(),
            "Force push with lease",
            true,
        ),
        GitDialog::DiscardAll => (
            "Discard all changes?",
            "Reset staged and unstaged files and remove untracked files.".into(),
            "Discard everything",
            true,
        ),
        GitDialog::AbortMerge => (
            "Abort merge?",
            "Return the working tree to its state before this merge began.".into(),
            "Abort merge",
            true,
        ),
        GitDialog::MergeBranch => (
            "Merge branch",
            "Merge another local branch into the current branch.".into(),
            "Merge branch",
            false,
        ),
        GitDialog::None => ("Git action", String::new(), "Continue", false),
    };
    let input_label = match action {
        GitDialog::Commit => Some("Commit message"),
        GitDialog::CreateBranch | GitDialog::RenameBranch | GitDialog::MergeBranch => {
            Some("Branch name")
        }
        GitDialog::CreateTag => Some("Tag name"),
        GitDialog::SigningRetry => Some("Signing passphrase"),
        _ => None,
    };
    rsx! {
        Modal {
            title,
            description,
            on_close: move |()| on_close.call(()),
            DialogForm {
                if let Some(label) = input_label {
                    Field { control_id: "git-action-input", label,
                        if action == GitDialog::Commit {
                            TextArea {
                                rows: 4,
                                placeholder: "Describe your changes",
                                autofocus: true,
                            }
                        } else {
                            TextInput {
                                input_type: if action == GitDialog::SigningRetry { TextInputType::Password } else { TextInputType::Text },
                                placeholder: if action == GitDialog::CreateTag { "v0.1.0" } else { "feature/name" },
                                autofocus: true,
                            }
                        }
                    }
                    if action == GitDialog::Commit {
                        label { class: "compact flex items-center gap-2.5 py-1.75",
                            Checkbox {
                                checked: amend(),
                                aria_label: "Amend previous commit",
                                on_checked_change: move |checked| amend.set(checked),
                            }
                            span { "Amend previous commit" }
                        }
                    }
                }
                if dangerous {
                    DangerNote { message: "Review this destructive operation carefully before continuing." }
                }
                if action == GitDialog::ForcePush {
                    label { class: "compact flex items-center gap-2.5 py-1.75",
                        Checkbox {
                            checked: force_push_acknowledged(),
                            aria_label: "I understand remote commits may be replaced",
                            on_checked_change: move |checked| force_push_acknowledged.set(checked),
                        }
                        span { "I understand remote commits may be replaced" }
                    }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: confirm,
                        kind: if dangerous { ButtonKind::Danger } else { ButtonKind::Primary },
                        onclick: move |_| on_submit.call(format!("{confirm} completed (mock)")),
                    }
                }
            }
        }
    }
}
