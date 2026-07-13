use dioxus::prelude::*;
use dioxus_code::{code, Code, Theme};
use dioxus_primitives::dropdown_menu::{
    DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger,
};

use crate::ui::{AppIcon, Button, ButtonKind, Drawer, Icon, IconButton, MenuTrigger, Modal, Toast};

const FILES: [(&str, &str, &str); 8] = [
    ("src", "folder", ""),
    ("app.rs", "file nested", "M"),
    ("files.rs", "file nested", "M"),
    ("git.rs", "file nested", ""),
    ("assets", "folder", ""),
    ("app.css", "file nested", "A"),
    ("README.md", "file", "M"),
    ("Cargo.toml", "file", ""),
];

#[derive(Clone, Copy, PartialEq, Eq)]
struct OpenFile {
    label: &'static str,
    dirty: bool,
}

const INITIAL_OPEN_FILES: [OpenFile; 5] = [
    OpenFile {
        label: "app.rs",
        dirty: true,
    },
    OpenFile {
        label: "README.md",
        dirty: true,
    },
    OpenFile {
        label: "logo.svg",
        dirty: false,
    },
    OpenFile {
        label: "hero.png",
        dirty: false,
    },
    OpenFile {
        label: "archive.bin",
        dirty: false,
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum FileDialog {
    None,
    CreateFile,
    CreateFolder,
    Move,
    Duplicate,
    Delete,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditorAction {
    GoToLine,
    ToggleWrap,
    ToggleLineNumbers,
    SaveAll,
    CloseAll,
    CloseOthers,
    OpenDiff,
    ToggleStage,
    CopyPath,
    Revert,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExplorerBranchAction {
    Main,
    Workspace,
    Runtime,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExplorerBranchDialog {
    None,
    Create,
    Rename,
}

#[component]
pub fn Files(slug: String) -> Element {
    let _ = slug;
    let mut selected = use_signal(|| "app.rs");
    let mut open_files = use_signal(|| INITIAL_OPEN_FILES.to_vec());
    let mut active_view = use_signal(|| "code");
    let mut drawer = use_signal(|| false);
    let mut sidebar_open = use_signal(|| true);
    let search = use_signal(|| false);
    let git_filter = use_signal(|| false);
    let explorer_menu = use_signal(|| false);
    let branch_menu = use_signal(|| false);
    let mut editor_menu = use_signal(|| false);
    let mut mobile_tabs_open = use_signal(|| false);
    let mut wrap = use_signal(|| false);
    let mut line_numbers = use_signal(|| true);
    let mut diff = use_signal(|| false);
    let mut staged = use_signal(|| false);
    let src_expanded = use_signal(|| true);
    let assets_expanded = use_signal(|| true);
    let mut current_branch = use_signal(|| "main".to_string());
    let mut branch_dialog = use_signal(|| ExplorerBranchDialog::None);
    let mut dialog = use_signal(|| FileDialog::None);
    let mut toast = use_signal(|| None::<String>);

    rsx! {
        div { class: if sidebar_open() { "files-layout" } else { "files-layout sidebar-closed" },
            if sidebar_open() {
                aside { class: "explorer desktop-explorer",
                    Explorer {
                        selected,
                        search,
                        git_filter,
                        menu: explorer_menu,
                        branch_menu,
                        current_branch,
                        src_expanded,
                        assets_expanded,
                        on_branch_dialog: move |next| branch_dialog.set(next),
                        on_action: move |action: FileDialog| dialog.set(action),
                        on_select: move |name: &'static str| {
                            if !open_files.read().iter().any(|file| file.label == name) {
                                open_files
                                    .write()
                                    .push(OpenFile {
                                        label: name,
                                        dirty: false,
                                    });
                            }
                            selected.set(name);
                            active_view.set(view_for_file(name));
                        },
                        on_notice: move |message: String| toast.set(Some(message)),
                    }
                }
            }
            if drawer() {
                Drawer {
                    title: "Explorer",
                    label: "Workspace file explorer",
                    content_class: "explorer drawer",
                    restore_focus: "button[aria-label='Open explorer']",
                    on_close: move |()| drawer.set(false),
                    Explorer {
                        selected,
                        search,
                        git_filter,
                        menu: explorer_menu,
                        branch_menu,
                        current_branch,
                        src_expanded,
                        assets_expanded,
                        on_branch_dialog: move |next| branch_dialog.set(next),
                        on_action: move |action: FileDialog| dialog.set(action),
                        on_select: move |name: &'static str| {
                            if !open_files.read().iter().any(|file| file.label == name) {
                                open_files
                                    .write()
                                    .push(OpenFile {
                                        label: name,
                                        dirty: false,
                                    });
                            }
                            selected.set(name);
                            active_view.set(view_for_file(name));
                            drawer.set(false);
                        },
                        on_notice: move |message: String| toast.set(Some(message)),
                    }
                }
            }
            section { class: "editor-panel",
                div { class: "editor-header",
                    div { class: "desktop-sidebar-toggle",
                        IconButton {
                            label: if sidebar_open() { "Hide file browser" } else { "Show file browser" },
                            icon: AppIcon::Explorer,
                            pressed: sidebar_open(),
                            onclick: move |_| sidebar_open.toggle(),
                        }
                    }
                    div { class: "mobile-sidebar-toggle",
                        IconButton {
                            label: "Open explorer",
                            icon: AppIcon::Explorer,
                            onclick: move |_| drawer.set(true),
                        }
                    }
                    div { class: "editor-tabs", role: "tablist",
                        for file in open_files() {
                            EditorTab {
                                label: file.label,
                                dirty: file.dirty,
                                active: selected() == file.label,
                                onclick: move |_| {
                                    selected.set(file.label);
                                    active_view.set(view_for_file(file.label));
                                },
                                onclose: move |()| close_file(file.label, open_files, selected, active_view, toast),
                            }
                        }
                    }
                    DropdownMenu {
                        class: "mobile-tabs",
                        open: mobile_tabs_open(),
                        on_open_change: move |open: bool| mobile_tabs_open.set(open),
                        DropdownMenuTrigger {
                            class: "mobile-tabs-trigger",
                            "aria-label": "Open file tabs",
                            "aria-expanded": mobile_tabs_open(),
                            span { class: "mobile-tabs-value",
                                if let Some(file) = open_files.read().iter().find(|file| file.label == selected()) {
                                    span { "{file.label}" }
                                    if file.dirty {
                                        span { class: "mobile-tab-dirty", "*" }
                                    }
                                } else {
                                    "No file open"
                                }
                            }
                            span { class: "mobile-tabs-chevron", "⌄" }
                        }
                        DropdownMenuContent { class: "dropdown mobile-tabs-dropdown",
                            if open_files.read().is_empty() {
                                div { class: "mobile-tabs-empty", "No file open" }
                            }
                            for (index, file) in open_files().into_iter().enumerate() {
                                div { class: if selected() == file.label { "mobile-tab-option active" } else { "mobile-tab-option" },
                                    DropdownMenuItem::<String> {
                                        class: "mobile-tab-select",
                                        value: file.label.to_string(),
                                        index,
                                        on_select: move |path| {
                                            if let Some(file) = open_files
                                                .read()
                                                .iter()
                                                .find(|file| file.label == path)
                                            {
                                                selected.set(file.label);
                                                active_view.set(view_for_file(file.label));
                                                mobile_tabs_open.set(false);
                                            }
                                        },
                                        span { class: "mobile-tab-label", "{file.label}" }
                                        if file.dirty {
                                            span { class: "mobile-tab-dirty", "*" }
                                        }
                                    }
                                    button {
                                        class: "mobile-tab-close",
                                        "aria-label": "Close {file.label}",
                                        title: "Close {file.label}",
                                        onclick: move |event| {
                                            event.stop_propagation();
                                            close_file(file.label, open_files, selected, active_view, toast);
                                            mobile_tabs_open.set(false);
                                        },
                                        crate::ui::Icon { icon: AppIcon::Close, size: 15 }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "toolbar-actions",
                        IconButton {
                            label: "Find in file",
                            icon: AppIcon::Search,
                            disabled: active_view() != "code",
                            onclick: move |_| toast.set(Some("Find opened".into())),
                        }
                        DropdownMenu {
                            class: "menu-anchor",
                            open: editor_menu(),
                            on_open_change: move |open: bool| editor_menu.set(open),
                            MenuTrigger {
                                label: "Editor actions",
                                icon: AppIcon::Menu,
                                open: editor_menu(),
                            }
                            DropdownMenuContent { class: "dropdown align-right",
                                DropdownMenuItem::<EditorAction> {
                                    value: EditorAction::GoToLine,
                                    index: 0_usize,
                                    on_select: move |_| toast.set(Some("Go to line opened".into())),
                                    span { "Go to Line" }
                                    kbd { "Mod G" }
                                }
                                DropdownMenuItem::<EditorAction> {
                                    value: EditorAction::ToggleWrap,
                                    index: 1_usize,
                                    on_select: move |_| wrap.toggle(),
                                    span {
                                        if wrap() {
                                            "✓ Word Wrap"
                                        } else {
                                            "Word Wrap"
                                        }
                                    }
                                }
                                DropdownMenuItem::<EditorAction> {
                                    value: EditorAction::ToggleLineNumbers,
                                    index: 2_usize,
                                    on_select: move |_| line_numbers.toggle(),
                                    span {
                                        if line_numbers() {
                                            "✓ Line Numbers"
                                        } else {
                                            "Line Numbers"
                                        }
                                    }
                                }
                                hr {}
                                DropdownMenuItem::<EditorAction> {
                                    value: EditorAction::SaveAll,
                                    index: 3_usize,
                                    on_select: move |_| toast.set(Some("All files saved".into())),
                                    span { "Save All" }
                                    kbd { "Mod Shift S" }
                                }
                                DropdownMenuItem::<EditorAction> {
                                    value: EditorAction::CloseAll,
                                    index: 4_usize,
                                    on_select: move |_| {
                                        open_files.write().clear();
                                        selected.set("");
                                        active_view.set("empty");
                                    },
                                    "Close All"
                                }
                                DropdownMenuItem::<EditorAction> {
                                    value: EditorAction::CloseOthers,
                                    index: 5_usize,
                                    on_select: move |_| {
                                        let active = selected();
                                        open_files.write().retain(|file| file.label == active);
                                    },
                                    "Close Others"
                                }
                                hr {}
                                DropdownMenuItem::<EditorAction> {
                                    value: EditorAction::OpenDiff,
                                    index: 6_usize,
                                    on_select: move |_| diff.toggle(),
                                    if diff() {
                                        "Hide Changes"
                                    } else {
                                        "View Changes"
                                    }
                                }
                                DropdownMenuItem::<EditorAction> {
                                    value: EditorAction::ToggleStage,
                                    index: 7_usize,
                                    on_select: move |_| staged.toggle(),
                                    if staged() {
                                        "Unstage File"
                                    } else {
                                        "Stage File"
                                    }
                                }
                                DropdownMenuItem::<EditorAction> {
                                    value: EditorAction::CopyPath,
                                    index: 8_usize,
                                    on_select: move |_| toast.set(Some("File path copied".into())),
                                    "Copy relative path"
                                }
                                hr {}
                                DropdownMenuItem::<EditorAction> {
                                    value: EditorAction::Revert,
                                    index: 9_usize,
                                    class: "destructive-text",
                                    on_select: move |_| toast.set(Some("Unsaved changes reverted".into())),
                                    "Revert Unsaved Changes"
                                }
                            }
                        }
                        IconButton {
                            label: "Save file",
                            icon: AppIcon::Save,
                            disabled: selected().is_empty(),
                            onclick: move |_| toast.set(Some(format!("{} saved", selected()))),
                        }
                    }
                }
                div { class: "editor-content",
                    if active_view() == "empty" {
                        div { class: "empty-state",
                            h2 { "No open files" }
                            p { "Choose a file from the explorer to open it." }
                        }
                    } else if diff() && active_view() == "code" {
                        DiffEditor { on_close: move |()| diff.set(false) }
                    } else if active_view() == "markdown" {
                        MarkdownPreview {}
                    } else if active_view() == "svg" {
                        SvgPreview {}
                    } else if active_view() == "image" {
                        ImagePreview {}
                    } else if active_view() == "unsupported" {
                        UnsupportedPreview {}
                    } else {
                        CodePlaceholder { wrap: wrap(), line_numbers: line_numbers() }
                    }
                }
                footer { class: "editor-statusbar",
                    div {
                        span { class: "status-light" }
                        "Mock buffer"
                    }
                    div {
                        span { "Ln 18, Col 24" }
                        span { "Spaces: 4" }
                        span { "UTF-8" }
                        span { "Rust" }
                    }
                }
            }
        }

        if dialog() != FileDialog::None {
            FileActionDialog {
                action: dialog(),
                on_close: move |()| dialog.set(FileDialog::None),
                on_submit: move |message| {
                    dialog.set(FileDialog::None);
                    toast.set(Some(message));
                },
            }
        }
        if branch_dialog() != ExplorerBranchDialog::None {
            ExplorerBranchActionDialog {
                action: branch_dialog(),
                current_branch: current_branch(),
                on_close: move |()| branch_dialog.set(ExplorerBranchDialog::None),
                on_submit: move |branch: String| {
                    let action = branch_dialog();
                    current_branch.set(branch.clone());
                    branch_dialog.set(ExplorerBranchDialog::None);
                    toast
                        .set(
                            Some(
                                match action {
                                    ExplorerBranchDialog::Create => {
                                        format!("Created and switched to {branch}")
                                    }
                                    ExplorerBranchDialog::Rename => {
                                        format!("Renamed branch to {branch}")
                                    }
                                    ExplorerBranchDialog::None => format!("Switched to {branch}"),
                                },
                            ),
                        );
                },
            }
        }
        if let Some(message) = toast() {
            Toast { message, on_close: move |()| toast.set(None) }
        }
    }
}

fn view_for_file(file: &str) -> &'static str {
    match file {
        "README.md" => "markdown",
        "logo.svg" => "svg",
        "hero.png" => "image",
        "archive.bin" => "unsupported",
        _ => "code",
    }
}

fn next_active_index_after_close(
    item_count: usize,
    active_index: Option<usize>,
    closing_index: usize,
) -> Option<usize> {
    if item_count <= 1 {
        return None;
    }

    match active_index {
        Some(active) if active == closing_index => Some(closing_index.min(item_count - 2)),
        Some(active) if closing_index < active => Some(active - 1),
        Some(active) => Some(active),
        None => None,
    }
}

fn close_file(
    label: &'static str,
    mut open_files: Signal<Vec<OpenFile>>,
    mut selected: Signal<&'static str>,
    mut active_view: Signal<&'static str>,
    mut toast: Signal<Option<String>>,
) {
    let files = open_files();
    let Some(closing_index) = files.iter().position(|open| open.label == label) else {
        return;
    };
    let active_index = files.iter().position(|open| open.label == selected());
    let next_index = next_active_index_after_close(files.len(), active_index, closing_index);
    open_files.write().remove(closing_index);
    if selected() == label {
        if let Some(index) = next_index {
            let next = open_files.read()[index].label;
            selected.set(next);
            active_view.set(view_for_file(next));
        } else {
            selected.set("");
            active_view.set("empty");
        }
    }
    toast.set(Some(format!("{label} closed")));
}

#[component]
fn Explorer(
    selected: Signal<&'static str>,
    mut search: Signal<bool>,
    mut git_filter: Signal<bool>,
    mut menu: Signal<bool>,
    mut branch_menu: Signal<bool>,
    mut current_branch: Signal<String>,
    mut src_expanded: Signal<bool>,
    mut assets_expanded: Signal<bool>,
    on_branch_dialog: EventHandler<ExplorerBranchDialog>,
    on_action: EventHandler<FileDialog>,
    on_select: EventHandler<&'static str>,
    on_notice: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "explorer-inner",
            div { class: "explorer-toolbar",
                DropdownMenu {
                    class: "menu-anchor",
                    open: menu(),
                    on_open_change: move |open: bool| menu.set(open),
                    MenuTrigger {
                        label: "File actions",
                        icon: AppIcon::Menu,
                        open: menu(),
                    }
                    DropdownMenuContent { class: "dropdown",
                        DropdownMenuItem::<FileDialog> {
                            value: FileDialog::CreateFile,
                            index: 0_usize,
                            on_select: move |action| on_action.call(action),
                            "New file"
                        }
                        DropdownMenuItem::<FileDialog> {
                            value: FileDialog::CreateFolder,
                            index: 1_usize,
                            on_select: move |action| on_action.call(action),
                            "New folder"
                        }
                        hr {}
                        DropdownMenuItem::<FileDialog> {
                            value: FileDialog::Move,
                            index: 2_usize,
                            on_select: move |action| on_action.call(action),
                            "Move selected"
                        }
                        DropdownMenuItem::<FileDialog> {
                            value: FileDialog::Duplicate,
                            index: 3_usize,
                            on_select: move |action| on_action.call(action),
                            "Duplicate selected"
                        }
                        DropdownMenuItem::<FileDialog> {
                            value: FileDialog::Delete,
                            index: 4_usize,
                            class: "destructive-text",
                            on_select: move |action| on_action.call(action),
                            "Delete selected"
                        }
                    }
                }
                IconButton {
                    label: "Search files",
                    icon: AppIcon::Search,
                    pressed: search(),
                    onclick: move |_| search.toggle(),
                }
                span { class: "toolbar-spacer" }
                IconButton {
                    label: "Refresh files",
                    icon: AppIcon::Refresh,
                    onclick: move |_| on_notice.call("Explorer refreshed".into()),
                }
            }
            if search() {
                div { class: "explorer-search",
                    input {
                        placeholder: "Search files…",
                        "aria-label": "Search files",
                        autofocus: true,
                    }
                }
            }
            div { class: "explorer-label",
                if git_filter() {
                    "GIT CHANGES"
                } else {
                    "FILES"
                }
            }
            div {
                class: "file-tree",
                role: "tree",
                "aria-label": "Workspace files",
                for (name, kind, status) in FILES {
                    if (!git_filter() || !status.is_empty())
                        && (kind != "file nested"
                            || ((matches!(name, "app.rs" | "files.rs" | "git.rs") && src_expanded())
                                || (name == "app.css" && assets_expanded())))
                    {
                        button {
                            class: if selected() == name { "tree-row selected {kind}" } else { "tree-row {kind}" },
                            role: "treeitem",
                            "aria-selected": selected() == name,
                            "aria-expanded": if kind == "folder" { Some(if name == "src" { src_expanded() } else { assets_expanded() }) } else { None },
                            onclick: move |_| {
                                if name == "src" {
                                    src_expanded.toggle();
                                } else if name == "assets" {
                                    assets_expanded.toggle();
                                } else {
                                    on_select.call(name);
                                }
                            },
                            span { class: "chevron",
                                if kind == "folder" {
                                    if (name == "src" && src_expanded()) || (name == "assets" && assets_expanded()) {
                                        "▾"
                                    } else {
                                        "▸"
                                    }
                                } else {
                                    ""
                                }
                            }
                            span { class: "tree-icon",
                                if kind == "folder" {
                                    "▣"
                                } else {
                                    "◇"
                                }
                            }
                            span { class: "tree-name", {name} }
                            if !status.is_empty() {
                                span { class: "tree-status", {status} }
                            }
                        }
                    }
                }
            }
            div { class: "explorer-footer",
                DropdownMenu {
                    class: "footer-branch menu-anchor",
                    open: branch_menu(),
                    on_open_change: move |open: bool| branch_menu.set(open),
                    DropdownMenuTrigger {
                        class: "branch-footer-button",
                        "aria-label": "Current branch: {current_branch}",
                        title: "Switch or manage branch",
                        Icon { icon: AppIcon::GitBranch, size: 11 }
                        span { "{current_branch}" }
                    }
                    DropdownMenuContent { class: "dropdown explorer-branch-dropdown",
                        DropdownMenuItem::<ExplorerBranchAction> {
                            value: ExplorerBranchAction::Main,
                            index: 0_usize,
                            class: if current_branch() == "main" { "selected-menu-item" } else { "" },
                            on_select: move |_| {
                                current_branch.set("main".into());
                                on_notice.call("Switched to main".into());
                            },
                            span { "main" }
                            if current_branch() == "main" {
                                Icon { icon: AppIcon::Check, size: 14 }
                            }
                        }
                        DropdownMenuItem::<ExplorerBranchAction> {
                            value: ExplorerBranchAction::Workspace,
                            index: 1_usize,
                            class: if current_branch() == "feature/workspace-ui" { "selected-menu-item" } else { "" },
                            on_select: move |_| {
                                current_branch.set("feature/workspace-ui".into());
                                on_notice.call("Switched to feature/workspace-ui".into());
                            },
                            span { "feature/workspace-ui" }
                            if current_branch() == "feature/workspace-ui" {
                                Icon { icon: AppIcon::Check, size: 14 }
                            }
                        }
                        DropdownMenuItem::<ExplorerBranchAction> {
                            value: ExplorerBranchAction::Runtime,
                            index: 2_usize,
                            class: if current_branch() == "fix/runtime-status" { "selected-menu-item" } else { "" },
                            on_select: move |_| {
                                current_branch.set("fix/runtime-status".into());
                                on_notice.call("Switched to fix/runtime-status".into());
                            },
                            span { "fix/runtime-status" }
                            if current_branch() == "fix/runtime-status" {
                                Icon { icon: AppIcon::Check, size: 14 }
                            }
                        }
                        hr {}
                        DropdownMenuItem::<ExplorerBranchDialog> {
                            value: ExplorerBranchDialog::Create,
                            index: 3_usize,
                            on_select: move |next| on_branch_dialog.call(next),
                            "Create branch…"
                        }
                        DropdownMenuItem::<ExplorerBranchDialog> {
                            value: ExplorerBranchDialog::Rename,
                            index: 4_usize,
                            on_select: move |next| on_branch_dialog.call(next),
                            "Rename current branch…"
                        }
                    }
                }
                button {
                    class: if git_filter() { "changes-filter active" } else { "changes-filter" },
                    "aria-label": if git_filter() { "Show all files" } else { "Show Git changed files" },
                    "aria-pressed": git_filter(),
                    title: if git_filter() { "Show all files" } else { "Show Git changed files" },
                    onclick: move |_| git_filter.toggle(),
                    "6 changes"
                }
            }
        }
    }
}

#[component]
fn ExplorerBranchActionDialog(
    action: ExplorerBranchDialog,
    current_branch: String,
    on_close: EventHandler<()>,
    on_submit: EventHandler<String>,
) -> Element {
    let is_create = action == ExplorerBranchDialog::Create;
    let mut value = use_signal(|| {
        if is_create {
            "feature/new-branch".to_string()
        } else {
            current_branch.clone()
        }
    });
    rsx! {
        Modal {
            title: if is_create { "Create branch" } else { "Rename branch" },
            description: if is_create { "Create and switch to a new local branch." } else { "Rename the current branch {current_branch}." },
            on_close: move |()| on_close.call(()),
            div { class: "form-stack",
                label { r#for: "explorer-branch-name", "Branch name" }
                input {
                    id: "explorer-branch-name",
                    value,
                    autofocus: true,
                    oninput: move |event| value.set(event.value()),
                }
                div { class: "modal-actions",
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if is_create { "Create branch" } else { "Rename" },
                        kind: ButtonKind::Primary,
                        disabled: value().trim().is_empty(),
                        onclick: move |_| on_submit.call(value().trim().to_string()),
                    }
                }
            }
        }
    }
}

#[component]
fn EditorTab(
    label: &'static str,
    dirty: bool,
    active: bool,
    onclick: EventHandler<MouseEvent>,
    onclose: EventHandler<()>,
) -> Element {
    let glyph = match std::path::Path::new(label)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
    {
        Some(extension) if extension.eq_ignore_ascii_case("rs") => "R",
        Some(extension) if extension.eq_ignore_ascii_case("md") => "M",
        _ => "◇",
    };
    rsx! {
        div { class: if active { "editor-tab active" } else { "editor-tab" },
            button {
                class: "tab-select",
                role: "tab",
                "aria-selected": active,
                onclick: move |event| onclick.call(event),
                span { class: "file-glyph", {glyph} }
                span { {label} }
            }
            if dirty {
                span {
                    class: "dirty-dot",
                    title: "Unsaved changes",
                    "aria-label": "Unsaved changes",
                }
            }
            button {
                class: "tab-close",
                "aria-label": "Close {label}",
                title: "Close {label}",
                onclick: move |_| onclose.call(()),
                crate::ui::Icon { icon: AppIcon::Close, size: 12 }
            }
        }
    }
}

#[component]
fn CodePlaceholder(wrap: bool, line_numbers: bool) -> Element {
    let class = match (wrap, line_numbers) {
        (true, true) => "code-editor wrap",
        (true, false) => "code-editor wrap no-line-numbers",
        (false, true) => "code-editor",
        (false, false) => "code-editor no-line-numbers",
    };
    rsx! {
        div { class, "aria-label": "Read-only code editor preview",
            Code { src: code!("/src/app.rs"), theme: Theme::TOKYO_NIGHT }
        }
    }
}

#[component]
fn DiffEditor(on_close: EventHandler<()>) -> Element {
    rsx! {
        div { class: "diff-editor",
            header {
                span { "Working tree changes" }
                button { onclick: move |_| on_close.call(()), "Close diff ×" }
            }
            div { class: "diff-meta", "@@ -12,7 +12,11 @@ pub fn App() -> Element {{" }
            div { class: "diff-line context",
                span { "12" }
                code { "    rsx! {{" }
            }
            div { class: "diff-line removed",
                span { "13" }
                code { "-       div {{ \"Starter app\" }}" }
            }
            div { class: "diff-line added",
                span { "13" }
                code { "+       document::Stylesheet {{ href: APP_CSS }}" }
            }
            div { class: "diff-line added",
                span { "14" }
                code { "+       Router::<Route> {{}}" }
            }
            div { class: "diff-line context",
                span { "15" }
                code { "    }}" }
            }
        }
    }
}

#[component]
fn MarkdownPreview() -> Element {
    rsx! {
        article { class: "preview markdown-preview",
            p { class: "preview-label", "MARKDOWN PREVIEW" }
            h1 { "Syntaxis" }
            p { "A local-first workspace for focused software development." }
            h2 { "Getting started" }
            p { "Run the responsive Dioxus interface with:" }
            pre {
                code { "dx serve" }
            }
            ul {
                li { "Explore project files" }
                li { "Run terminal sessions" }
                li { "Review and commit Git changes" }
            }
        }
    }
}

#[component]
fn SvgPreview() -> Element {
    rsx! {
        div { class: "preview media-preview",
            p { class: "preview-label", "SVG PREVIEW · 256 × 256" }
            div { class: "checkerboard",
                div { class: "mock-logo",
                    span { "S" }
                }
            }
        }
    }
}

#[component]
fn ImagePreview() -> Element {
    rsx! {
        div { class: "preview media-preview",
            p { class: "preview-label", "IMAGE PREVIEW · 1600 × 900 · 184 KB" }
            div { class: "mock-image",
                div { class: "mock-image-glow" }
                strong { "Syntaxis" }
                small { "Build without leaving your flow." }
            }
        }
    }
}

#[component]
fn UnsupportedPreview() -> Element {
    rsx! {
        div { class: "preview unsupported-preview",
            div { class: "empty-icon", "?" }
            h2 { "Preview unavailable" }
            p { "archive.bin is a binary file and cannot be displayed in the editor." }
            div { class: "file-facts",
                span { "4.8 MB" }
                span { "application/octet-stream" }
            }
        }
    }
}

#[component]
fn FileActionDialog(
    action: FileDialog,
    on_close: EventHandler<()>,
    on_submit: EventHandler<String>,
) -> Element {
    let (title, description, label, dangerous) = match action {
        FileDialog::CreateFile => (
            "New file",
            "Create a file in the selected folder.",
            "Create file",
            false,
        ),
        FileDialog::CreateFolder => (
            "New folder",
            "Create a folder in the selected location.",
            "Create folder",
            false,
        ),
        FileDialog::Move => (
            "Move app.rs",
            "Choose a new workspace-relative path.",
            "Move",
            false,
        ),
        FileDialog::Duplicate => (
            "Duplicate app.rs",
            "Choose a name for the copied file.",
            "Duplicate",
            false,
        ),
        FileDialog::Delete => (
            "Delete app.rs?",
            "This mock action cannot be undone.",
            "Delete file",
            true,
        ),
        FileDialog::None => ("File action", "", "Continue", false),
    };
    rsx! {
        Modal {
            title,
            description,
            on_close: move |()| on_close.call(()),
            div { class: "form-stack",
                if !dangerous {
                    label { r#for: "file-action-input", "Name or path" }
                    input {
                        id: "file-action-input",
                        value: if action == FileDialog::Duplicate { "app-copy.rs" } else { "" },
                        placeholder: "src/new_file.rs",
                        autofocus: true,
                    }
                } else {
                    p { class: "danger-note", "The file will be removed from this workspace." }
                }
                div { class: "modal-actions",
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label,
                        kind: if dangerous { ButtonKind::Danger } else { ButtonKind::Primary },
                        onclick: move |_| on_submit.call("File action completed (mock)".into()),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::next_active_index_after_close;

    #[test]
    fn closing_active_file_prefers_the_item_now_at_the_same_index() {
        assert_eq!(next_active_index_after_close(4, Some(1), 1), Some(1));
    }

    #[test]
    fn closing_last_active_file_selects_the_previous_item() {
        assert_eq!(next_active_index_after_close(4, Some(3), 3), Some(2));
    }

    #[test]
    fn closing_inactive_file_preserves_the_active_item() {
        assert_eq!(next_active_index_after_close(4, Some(2), 0), Some(1));
        assert_eq!(next_active_index_after_close(4, Some(1), 3), Some(1));
    }

    #[test]
    fn closing_the_final_file_has_no_next_selection() {
        assert_eq!(next_active_index_after_close(1, Some(0), 0), None);
    }
}
