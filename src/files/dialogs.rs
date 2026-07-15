#[allow(unused_imports)] // Dioxus expands the parent glob for RSX hot-reload analysis.
use super::{
    close_documents, component, dioxus_core, dioxus_signals, rsx, save_and_close, use_signal,
    ActionCallback, AnyStorage, Button, ButtonExtension, ButtonKind, CloseRequest, DangerNote,
    DataExtension, DialogActions, DialogForm, Element, EventHandler, Field, FieldsetExtension,
    FileAction, FileActionDialog, FormEvent, FormExtension, GlobalAttributesExtension, HasFormData,
    HasKeyboardData, History, InputExtension, Key, KeyboardEvent, LiExtension, LinkExtension,
    MeterExtension, Modal, OpenDocument, OptgroupExtension, OptionExtension, ParamExtension,
    ProgressExtension, Props, ReadableExt, ReadableHashMapExt, ReadableHashSetExt,
    ReadableOptionExt, ReadableResultExt, ReadableStrExt, ReadableVecExt, SelectExtension, Signal,
    Storage, StyleExtension, SvgAttributesExtension, TextInput, TextareaExtension, ToastState,
    TrackExtension, WorkspaceRecord, WritableExt,
};

#[component]
pub(super) fn FileMutationDialog(
    dialog: FileActionDialog,
    on_close: EventHandler<()>,
    on_submit: EventHandler<String>,
) -> Element {
    let mut value = use_signal(|| suggested_destination(&dialog));
    let (title, description, label, dangerous) = match dialog.action {
        FileAction::CreateFile => (
            "New file",
            "Create a workspace-relative file.",
            "Create file",
            false,
        ),
        FileAction::CreateFolder => (
            "New folder",
            "Create a workspace-relative folder.",
            "Create folder",
            false,
        ),
        FileAction::Move => (
            "Move item",
            "Choose a new workspace-relative path.",
            "Move",
            false,
        ),
        FileAction::Duplicate => (
            "Duplicate item",
            "Choose the copy's workspace-relative path.",
            "Duplicate",
            false,
        ),
        FileAction::Delete => (
            "Delete item?",
            "This removes the selected item and all children.",
            "Delete",
            true,
        ),
    };
    rsx! {
        Modal {
            title,
            description,
            on_close: move |()| on_close.call(()),
            DialogForm {
                if dangerous {
                    DangerNote { message: dialog.source.clone().unwrap_or_default() }
                } else {
                    Field {
                        control_id: "file-path",
                        label: "Workspace-relative path",
                        TextInput {
                            value: value(),
                            autofocus: true,
                            oninput: move |event: FormEvent| value.set(event.value()),
                        }
                    }
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
                        disabled: !dangerous && value().trim().is_empty(),
                        onclick: move |_| on_submit.call(value().trim().to_owned()),
                    }
                }
            }
        }
    }
}

pub(super) fn suggested_destination(dialog: &FileActionDialog) -> String {
    match dialog.action {
        FileAction::CreateFile => "new_file.txt".into(),
        FileAction::CreateFolder => "new_folder".into(),
        FileAction::Move => dialog.source.clone().unwrap_or_default(),
        FileAction::Duplicate => dialog.source.as_deref().map_or_else(
            || "copy".into(),
            |source| {
                let (stem, extension) = source.rsplit_once('.').unwrap_or((source, ""));
                if extension.is_empty() {
                    format!("{stem}-copy")
                } else {
                    format!("{stem}-copy.{extension}")
                }
            },
        ),
        FileAction::Delete => String::new(),
    }
}

#[component]
pub(super) fn DirtyCloseDialog(
    count: usize,
    on_cancel: EventHandler<()>,
    on_discard: EventHandler<()>,
    on_save: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Unsaved changes",
            description: if count == 1 { "Save this file before closing it?".into() } else { format!("Save changed files before closing {count} tabs?") },
            on_close: move |()| on_cancel.call(()),
            DialogForm {
                DangerNote { message: "Closing without saving discards editor changes." }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        onclick: move |_| on_cancel.call(()),
                    }
                    Button {
                        label: "Discard",
                        kind: ButtonKind::Danger,
                        onclick: move |_| on_discard.call(()),
                    }
                    Button {
                        label: "Save",
                        kind: ButtonKind::Primary,
                        onclick: move |_| on_save.call(()),
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn DirtyClosePrompt(
    request: CloseRequest,
    workspace: Signal<Option<WorkspaceRecord>>,
    documents: Signal<Vec<OpenDocument>>,
    active_path: Signal<Option<String>>,
    mut close_request: Signal<Option<CloseRequest>>,
    toast: Signal<Option<ToastState>>,
) -> Element {
    let discard_paths = request.paths.clone();
    let save_paths = request.paths.clone();
    rsx! {
        DirtyCloseDialog {
            count: request.paths.len(),
            on_cancel: move |()| close_request.set(None),
            on_discard: move |()| {
                close_documents(&discard_paths, documents, active_path);
                close_request.set(None);
            },
            on_save: move |()| {
                save_and_close(
                    workspace(),
                    save_paths.clone(),
                    documents,
                    active_path,
                    close_request,
                    toast,
                );
            },
        }
    }
}

#[component]
pub(super) fn GitDiscardPrompt(
    path: String,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Discard Git changes?",
            description: "Restore this path to the repository version. Untracked files are deleted.",
            on_close: move |()| on_close.call(()),
            DialogForm {
                DangerNote { message: path }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: "Discard changes",
                        kind: ButtonKind::Danger,
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn GoToLineDialog(
    current: usize,
    on_close: EventHandler<()>,
    on_submit: EventHandler<usize>,
) -> Element {
    let mut value = use_signal(|| current.to_string());
    rsx! {
        Modal {
            title: "Go to line",
            description: "Move the editor cursor to a one-based line number.",
            on_close: move |()| on_close.call(()),
            DialogForm {
                Field { control_id: "go-line", label: "Line",
                    TextInput {
                        value: value(),
                        autofocus: true,
                        oninput: move |event: FormEvent| value.set(event.value()),
                        onkeydown: move |event: KeyboardEvent| {
                            if event.key() == Key::Enter {
                                if let Ok(line) = value().parse::<usize>() {
                                    on_submit.call(line.max(1));
                                }
                            }
                        },
                    }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: "Go",
                        kind: ButtonKind::Primary,
                        disabled: value().parse::<usize>().is_err(),
                        onclick: move |_| {
                            if let Ok(line) = value().parse::<usize>() {
                                on_submit.call(line.max(1));
                            }
                        },
                    }
                }
            }
        }
    }
}
