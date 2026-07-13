use dioxus::prelude::*;
use dioxus_code::{code, Code, Theme};
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem, DropdownMenuTrigger};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, ControlSize, DangerNote, DialogActions, DialogForm, Drawer, Field,
    Icon, IconButton, MenuContent, MenuTrigger, Modal, PanelHeader, PanelTab, PanelTabIndicator,
    PanelTabList, PanelTabWidth, TextInput, TextInputType, Toast,
};

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
        div { class: if sidebar_open() { "grid size-full min-h-0 min-w-0 grid-cols-[248px_minmax(0,1fr)] overflow-hidden max-md:block" } else { "grid size-full min-h-0 min-w-0 grid-cols-[minmax(0,1fr)] overflow-hidden max-md:block" },
            if sidebar_open() {
                aside { class: "min-h-0 min-w-0 border-r border-border bg-background max-md:hidden",
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
                    content_class: "h-full w-[min(330px,88vw)] justify-self-start border-0 border-r border-border bg-background shadow-[15px_0_50px_#0008]",
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
            section { class: "flex min-h-0 min-w-0 flex-col overflow-hidden max-md:h-full",
                PanelHeader {
                    div { class: "shrink-0 max-md:hidden",
                        IconButton {
                            label: if sidebar_open() { "Hide file browser" } else { "Show file browser" },
                            icon: AppIcon::Explorer,
                            pressed: sidebar_open(),
                            onclick: move |_| sidebar_open.toggle(),
                        }
                    }
                    div { class: "hidden shrink-0 max-md:block",
                        IconButton {
                            label: "Open explorer",
                            icon: AppIcon::Explorer,
                            onclick: move |_| drawer.set(true),
                        }
                    }
                    PanelTabList {
                        for file in open_files() {
                            PanelTab {
                                label: file.label.to_string(),
                                dirty: file.dirty,
                                active: selected() == file.label,
                                width: PanelTabWidth::Content,
                                indicator: PanelTabIndicator::Glyph(file_glyph(file.label).into()),
                                on_select: move |_| {
                                    selected.set(file.label);
                                    active_view.set(view_for_file(file.label));
                                },
                                on_close: move |()| close_file(file.label, open_files, selected, active_view, toast),
                            }
                        }
                    }
                    DropdownMenu {
                        class: "relative hidden min-w-0 flex-1 max-md:block",
                        open: mobile_tabs_open(),
                        on_open_change: move |open: bool| mobile_tabs_open.set(open),
                        DropdownMenuTrigger {
                            class: "flex h-10 w-full items-center justify-between gap-2 rounded-md border border-input bg-background px-3 text-left text-xs text-foreground hover:bg-accent data-[state=open]:bg-accent",
                            "aria-label": "Open file tabs",
                            "aria-expanded": mobile_tabs_open(),
                            span { class: "flex min-w-0 items-center gap-1 overflow-hidden [&>span:first-child]:truncate",
                                if let Some(file) = open_files.read().iter().find(|file| file.label == selected()) {
                                    span { "{file.label}" }
                                    if file.dirty {
                                        span { class: "shrink-0 text-primary", "*" }
                                    }
                                } else {
                                    "No file open"
                                }
                            }
                            span { class: "shrink-0 text-muted-foreground", "⌄" }
                        }
                        MenuContent { class: "!top-[calc(100%+4px)] right-2 left-2 w-auto !p-0.75",
                            if open_files.read().is_empty() {
                                div { class: "p-2.5 text-xs text-muted-foreground",
                                    "No file open"
                                }
                            }
                            for (index, file) in open_files().into_iter().enumerate() {
                                div { class: if selected() == file.label { "flex h-11 min-w-0 items-stretch rounded-md border border-border bg-accent text-foreground not-first:mt-0.5" } else { "flex h-11 min-w-0 items-stretch rounded-md border border-border bg-background text-muted-foreground not-first:mt-0.5" },
                                    DropdownMenuItem::<String> {
                                        class: "min-h-10.5 min-w-0 flex-1 justify-start gap-1 rounded-r-none px-2 text-inherit",
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
                                        span { class: "truncate", "{file.label}" }
                                        if file.dirty {
                                            span { class: "shrink-0 text-primary", "*" }
                                        }
                                    }
                                    button {
                                        class: "min-h-10.5 w-10.5 min-w-10.5 justify-center rounded-l-none p-0 text-muted-foreground",
                                        "aria-label": "Close {file.label}",
                                        title: "Close {file.label}",
                                        onclick: move |event| {
                                            event.stop_propagation();
                                            close_file(file.label, open_files, selected, active_view, toast);
                                            mobile_tabs_open.set(false);
                                        },
                                        Icon { icon: AppIcon::Close, size: 15 }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "flex items-center gap-1",
                        IconButton {
                            label: "Find in file",
                            icon: AppIcon::Search,
                            disabled: active_view() != "code",
                            onclick: move |_| toast.set(Some("Find opened".into())),
                        }
                        DropdownMenu {
                            class: "relative",
                            open: editor_menu(),
                            on_open_change: move |open: bool| editor_menu.set(open),
                            MenuTrigger {
                                label: "Editor actions",
                                icon: AppIcon::Menu,
                                open: editor_menu(),
                            }
                            MenuContent { class: "right-0 w-47.5",
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
                                    class: "!text-destructive",
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
                div { class: "relative min-h-0 min-w-0 flex-1 overflow-auto bg-card",
                    if active_view() == "empty" {
                        div { class: "flex size-full flex-col items-center justify-center p-7 text-center",
                            h2 { class: "text-lg text-foreground", "No open files" }
                            p { class: "mt-1.75 max-w-97.5 leading-normal text-muted-foreground",
                                "Choose a file from the explorer to open it."
                            }
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
                footer { class: "flex h-6.25 min-h-6.25 items-center justify-between border-t border-border bg-background px-2.5 text-[9px] text-muted-foreground",
                    div { class: "flex items-center gap-3.25",
                        span { class: "size-2 rounded-full bg-success" }
                        "Mock buffer"
                    }
                    div { class: "flex items-center gap-3.25",
                        span { class: "max-md:hidden", "Ln 18, Col 24" }
                        span { class: "max-md:hidden", "Spaces: 4" }
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
        div { class: "flex h-full min-h-0 flex-col",
            div { class: "flex h-10.5 min-h-10.5 items-center border-b border-border px-1.25",
                DropdownMenu {
                    class: "relative",
                    open: menu(),
                    on_open_change: move |open: bool| menu.set(open),
                    MenuTrigger {
                        label: "File actions",
                        icon: AppIcon::Menu,
                        open: menu(),
                    }
                    MenuContent { class: "left-0 w-47.5",
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
                            class: "!text-destructive",
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
                span { class: "flex-1" }
                IconButton {
                    label: "Refresh files",
                    icon: AppIcon::Refresh,
                    onclick: move |_| on_notice.call("Explorer refreshed".into()),
                }
            }
            if search() {
                div { class: "border-b border-border p-1.75",
                    TextInput {
                        size: ControlSize::Small,
                        input_type: TextInputType::Search,
                        placeholder: "Search files…",
                        aria_label: "Search files",
                        autofocus: true,
                    }
                }
            }
            div { class: "flex h-7.75 min-h-7.75 items-center justify-between px-2.75 text-[10px] font-bold tracking-[0.08em] text-muted-foreground",
                if git_filter() {
                    "GIT CHANGES"
                } else {
                    "FILES"
                }
            }
            div {
                class: "min-h-0 flex-1 overflow-y-auto px-1.25",
                role: "tree",
                "aria-label": "Workspace files",
                for (name, kind, status) in FILES {
                    if (!git_filter() || !status.is_empty())
                        && (kind != "file nested"
                            || ((matches!(name, "app.rs" | "files.rs" | "git.rs") && src_expanded())
                                || (name == "app.css" && assets_expanded())))
                    {
                        button {
                            class: if selected() == name { "flex h-7.25 w-full items-center gap-1.5 rounded-sm bg-accent px-1.5 text-left text-xs text-foreground [&.nested]:pl-4.75 {kind}" } else { "flex h-7.25 w-full items-center gap-1.5 rounded-sm bg-transparent px-1.5 text-left text-xs text-foreground/90 hover:bg-accent/65 [&.nested]:pl-4.75 {kind}" },
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
                            span { class: "w-2.25 text-[9px] text-muted-foreground",
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
                            span { class: if kind == "folder" { "w-3.25 text-warning" } else { "w-3.25 text-primary" },
                                if kind == "folder" {
                                    "▣"
                                } else {
                                    "◇"
                                }
                            }
                            span { class: "flex-1 truncate", {name} }
                            if !status.is_empty() {
                                span { class: "text-[10px] font-bold text-warning", {status} }
                            }
                        }
                    }
                }
            }
            div { class: "flex h-7.25 min-h-7.25 items-center justify-between border-t border-border px-2.5 text-[10px] text-muted-foreground",
                DropdownMenu {
                    class: "relative min-w-0",
                    open: branch_menu(),
                    on_open_change: move |open: bool| branch_menu.set(open),
                    DropdownMenuTrigger {
                        class: "flex h-5.75 max-w-33.75 items-center gap-1.25 rounded-sm bg-transparent px-1.25 text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground data-[state=open]:bg-accent data-[state=open]:text-foreground [&_span]:truncate",
                        "aria-label": "Current branch: {current_branch}",
                        title: "Switch or manage branch",
                        Icon { icon: AppIcon::GitBranch, size: 11 }
                        span { "{current_branch}" }
                    }
                    MenuContent { class: "!top-auto bottom-[calc(100%+5px)] left-0 w-58.75",
                        DropdownMenuItem::<ExplorerBranchAction> {
                            value: ExplorerBranchAction::Main,
                            index: 0_usize,
                            class: if current_branch() == "main" { "text-primary" } else { "" },
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
                            class: if current_branch() == "feature/workspace-ui" { "text-primary" } else { "" },
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
                            class: if current_branch() == "fix/runtime-status" { "text-primary" } else { "" },
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
                    class: if git_filter() { "rounded-sm bg-accent px-1.25 py-0.75 text-[10px] text-foreground" } else { "rounded-sm bg-transparent px-1.25 py-0.75 text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground" },
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
            DialogForm {
                Field { control_id: "explorer-branch-name", label: "Branch name",
                    TextInput {
                        value: value(),
                        autofocus: true,
                        oninput: move |event: FormEvent| value.set(event.value()),
                    }
                }
                DialogActions {
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

fn file_glyph(label: &str) -> &'static str {
    match std::path::Path::new(label)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
    {
        Some(extension) if extension.eq_ignore_ascii_case("rs") => "R",
        Some(extension) if extension.eq_ignore_ascii_case("md") => "M",
        _ => "◇",
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
            p { "A connected workspace for focused software development." }
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
            DialogForm {
                if !dangerous {
                    Field {
                        control_id: "file-action-input",
                        label: "Name or path",
                        TextInput {
                            value: if action == FileDialog::Duplicate { "app-copy.rs" } else { "" },
                            placeholder: "src/new_file.rs",
                            autofocus: true,
                        }
                    }
                } else {
                    DangerNote { message: "The file will be removed from this workspace." }
                }
                DialogActions {
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
