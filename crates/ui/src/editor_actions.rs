use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem};

use crate::{AppIcon, Icon, MenuContent, MenuTrigger};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditorAction {
    Undo,
    Redo,
    SelectAll,
    GoToLine,
    CopyReference,
    LanguageServices,
    TriggerCompletion,
    GoToDefinition,
    FindReferences,
    FormatDocument,
    WordWrap,
    LineNumbers,
    SaveAll,
    CloseAll,
    CloseOthers,
    Download,
    ViewChanges,
    ToggleStage,
    Revert,
}

/// Canonical editor actions menu. Callers enable the capabilities they support.
#[component]
pub fn EditorActionsMenu(
    mut open: Signal<bool>,
    interactive: bool,
    copy_reference: bool,
    navigation_available: bool,
    language_services: bool,
    code_intelligence_available: bool,
    #[props(default = false)] completion_available: bool,
    #[props(default = false)] definition_available: bool,
    #[props(default = false)] references_available: bool,
    #[props(default = false)] formatting_available: bool,
    word_wrap: bool,
    line_numbers: bool,
    save_all: bool,
    multiple_tabs: bool,
    #[props(default = false)] download_available: bool,
    changed: bool,
    #[props(default = false)] changes_visible: bool,
    #[props(default = false)] stage_available: bool,
    #[props(default = false)] unstaged: bool,
    #[props(default = "Revert File".to_owned())] revert_label: String,
    #[props(default)] revert_available: Option<bool>,
    on_action: EventHandler<EditorAction>,
) -> Element {
    rsx! {
        DropdownMenu {
            class: "relative",
            open: open(),
            on_open_change: move |next: bool| open.set(next),
            MenuTrigger {
                label: "Editor actions",
                icon: AppIcon::Menu,
                open: open(),
                on_toggle: move |()| open.toggle(),
            }
            MenuContent { class: "right-0 w-60",
                EditorMenuSection { label: "Editing" }
                EditorMenuItem { index: 0, icon: AppIcon::Undo, label: "Undo", suffix: "Mod Z", disabled: !interactive, onclick: move |()| on_action.call(EditorAction::Undo) }
                EditorMenuItem { index: 1, icon: AppIcon::Redo, label: "Redo", suffix: "Mod Shift Z", disabled: !interactive, onclick: move |()| on_action.call(EditorAction::Redo) }
                EditorMenuItem { index: 2, icon: AppIcon::SelectAll, label: "Select All", suffix: "Mod A", disabled: !interactive, onclick: move |()| on_action.call(EditorAction::SelectAll) }
                hr {}
                EditorMenuSection { label: "Navigation" }
                EditorMenuItem { index: 3, icon: AppIcon::GoToLine, label: "Go to Line", suffix: "Mod G", disabled: !interactive || !navigation_available, onclick: move |()| on_action.call(EditorAction::GoToLine) }
                EditorMenuItem { index: 4, icon: AppIcon::Copy, label: "Copy Reference", disabled: !copy_reference, onclick: move |()| on_action.call(EditorAction::CopyReference) }
                hr {}
                EditorMenuSection { label: "Code intelligence" }
                EditorMenuItem { index: 5, icon: AppIcon::LanguageServices, label: "Language Services", checked: language_services, disabled: !code_intelligence_available, onclick: move |()| on_action.call(EditorAction::LanguageServices) }
                EditorMenuItem { index: 6, icon: AppIcon::Completion, label: "Trigger Completion", suffix: "Mod Space", disabled: !interactive || !language_services || !completion_available, onclick: move |()| on_action.call(EditorAction::TriggerCompletion) }
                EditorMenuItem { index: 7, icon: AppIcon::GoToDefinition, label: "Go to Definition", suffix: "F12", disabled: !interactive || !definition_available, onclick: move |()| on_action.call(EditorAction::GoToDefinition) }
                EditorMenuItem { index: 8, icon: AppIcon::FindReferences, label: "Find References", suffix: "Shift F12", disabled: !interactive || !references_available, onclick: move |()| on_action.call(EditorAction::FindReferences) }
                EditorMenuItem { index: 9, icon: AppIcon::FormatDocument, label: "Format Document", suffix: "Shift Alt F", disabled: !interactive || !formatting_available, onclick: move |()| on_action.call(EditorAction::FormatDocument) }
                hr {}
                EditorMenuSection { label: "Editor view" }
                EditorMenuItem { index: 10, icon: AppIcon::WordWrap, label: "Word Wrap", checked: word_wrap, onclick: move |()| on_action.call(EditorAction::WordWrap) }
                EditorMenuItem { index: 11, icon: AppIcon::LineNumbers, label: "Line Numbers", checked: line_numbers, onclick: move |()| on_action.call(EditorAction::LineNumbers) }
                hr {}
                EditorMenuSection { label: "Tabs" }
                EditorMenuItem { index: 12, icon: AppIcon::Save, label: "Save All", suffix: "Mod Shift S", disabled: !save_all, onclick: move |()| on_action.call(EditorAction::SaveAll) }
                EditorMenuItem { index: 13, icon: AppIcon::Close, label: "Close All", disabled: !interactive, onclick: move |()| on_action.call(EditorAction::CloseAll) }
                EditorMenuItem { index: 14, icon: AppIcon::CloseOthers, label: "Close Others", disabled: !multiple_tabs, onclick: move |()| on_action.call(EditorAction::CloseOthers) }
                EditorMenuItem { index: 18, icon: AppIcon::Share, label: "Download File", disabled: !download_available, onclick: move |()| on_action.call(EditorAction::Download) }
                hr {}
                EditorMenuSection { label: "Source control" }
                EditorMenuItem { index: 15, icon: AppIcon::FileDiff, label: if changes_visible { "Hide Changes" } else { "View Changes" }, disabled: !changes_visible && !changed, onclick: move |()| on_action.call(EditorAction::ViewChanges) }
                EditorMenuItem { index: 16, icon: if unstaged { AppIcon::FilePlus } else { AppIcon::FileMinus }, label: if unstaged { "Stage File" } else { "Unstage File" }, disabled: !stage_available, onclick: move |()| on_action.call(EditorAction::ToggleStage) }
                hr {}
                EditorMenuItem { index: 17, icon: AppIcon::Revert, label: revert_label, disabled: !revert_available.unwrap_or(interactive), danger: true, onclick: move |()| on_action.call(EditorAction::Revert) }
            }
        }
    }
}

#[component]
fn EditorMenuSection(label: &'static str) -> Element {
    rsx! {
        div { class: "px-2 py-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground", "{label}" }
    }
}

/// Canonical item used by editor action menus in every application shell.
#[component]
pub fn EditorMenuItem(
    index: usize,
    icon: AppIcon,
    label: String,
    #[props(default)] suffix: String,
    #[props(default = false)] checked: bool,
    #[props(default = false)] disabled: bool,
    #[props(default = false)] danger: bool,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        DropdownMenuItem::<usize> {
            value: index,
            index,
            disabled,
            class: if danger { "!text-destructive" } else { "" },
            on_select: move |_| onclick.call(()),
            span { class: "flex min-w-0 items-center gap-2",
                Icon { icon, size: 14 }
                span { class: "truncate", "{label}" }
            }
            if checked || !suffix.is_empty() {
                span { class: "ml-auto flex shrink-0 items-center gap-2",
                    if checked {
                        Icon { icon: AppIcon::Check, size: 12 }
                    }
                    if !suffix.is_empty() {
                        kbd { "{suffix}" }
                    }
                }
            }
        }
    }
}
