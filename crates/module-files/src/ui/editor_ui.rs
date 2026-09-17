//! Shared editor interactions.

#[allow(
    unused_imports,
    reason = "Dioxus expands the parent glob for RSX hot-reload analysis"
)]
use super::{
    ActionCallback, AnyStorage, AppIcon, ButtonExtension, CanvasExtension, CloseRequest,
    ControlSize, DataExtension, DetailsExtension, DialogExtension, DropdownMenu, DropdownMenuItem,
    EditorCommand, EditorCommandKind, EditorSelection, Element, EmbedExtension, EventHandler,
    FieldsetExtension, FormEvent, GlobalAttributesExtension, HasAttributes, HasFormData,
    HasKeyboardData, HasPointerData, History, Icon, IframeExtension, ImgExtension, InputExtension,
    Key, KeyboardEvent, LiExtension, LinkExtension, MenuButtonTrigger, MenuContent, MeterExtension,
    Modifiers, ModifiersInteraction, MountedData, MpaddedExtension, MspaceExtension,
    ObjectExtension, OlExtension, OpenDocument, OpenTab, OptgroupExtension, OptionExtension,
    PanelTab, PanelTabIndicator, PanelTabWidth, ParamExtension, ProgressExtension, Props,
    ReadableExt, ReadableHashMapExt, ReadableHashSetExt, ReadableOptionExt, ReadableResultExt,
    ReadableStrExt, ReadableVecExt, SelectExtension, Signal, Storage, SvgAttributesExtension,
    TextInput, TextInputType, TextareaExtension, ToastState, TrackExtension, UnifiedDiff,
    VideoExtension, WorkspaceRecord, WritableExt, WritableStringExt, WritableVecExt, component,
    dioxus_core, dioxus_elements, dioxus_signals, file_glyph, language_slug_for_path,
    request_close, rsx, save_path, set_error, set_success, spawn,
};
use std::rc::Rc;

pub(super) fn render_tab(
    tab: OpenTab,
    mut active_path: Signal<Option<String>>,
    documents: Signal<Vec<OpenDocument>>,
    close_request: Signal<Option<CloseRequest>>,
    mut diff: Signal<Option<UnifiedDiff>>,
) -> Element {
    let path = tab.path;
    let close_path = path.clone();
    rsx! {
        PanelTab {
            key: "{path}",
            label: tab.label,
            dirty: tab.dirty,
            active: active_path().as_deref() == Some(&path),
            width: PanelTabWidth::Content,
            indicator: PanelTabIndicator::Glyph(file_glyph(&path).into()),
            on_select: move |_| {
                active_path.set(Some(path.clone()));
                diff.set(None);
            },
            on_close: move |()| {
                request_close(close_path.clone(), documents, active_path, close_request);
            },
        }
    }
}

#[component]
pub(super) fn MobileTabs(
    tabs: Vec<OpenTab>,
    mut active_path: Signal<Option<String>>,
    mut open: Signal<bool>,
    on_close: EventHandler<String>,
) -> Element {
    rsx! {
        DropdownMenu {
            class: "relative hidden min-w-0 flex-1 max-md:block",
            open: open(),
            on_open_change: move |next: bool| open.set(next),
            MenuButtonTrigger {
                class: "flex h-10 w-full items-center justify-between gap-2 rounded-md border border-input bg-background px-3 text-left text-xs text-foreground",
                label: "Open file tabs",
                on_toggle: move |()| open.toggle(),
                span { class: "truncate", {active_path().unwrap_or_else(|| "No file open".into())} }
                span { "⌄" }
            }
            MenuContent { class: "right-2 left-2 w-auto",
                for (index, tab) in tabs.into_iter().enumerate() {
                    DropdownMenuItem::<String> {
                        key: "{tab.path}",
                        value: tab.path.clone(),
                        index,
                        on_select: move |path| {
                            active_path.set(Some(path));
                            open.set(false);
                        },
                        span { class: "flex-1 truncate", "{tab.path}" }
                        if tab.dirty {
                            span { class: "text-primary", "*" }
                        }
                        button {
                            class: "px-2",
                            "aria-label": "Close {tab.label}",
                            onclick: move |event| {
                                event.stop_propagation();
                                on_close.call(tab.path.clone());
                            },
                            "×"
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn format_editor_reference(
    path: &str,
    source: &str,
    selection: &EditorSelection,
) -> String {
    let start = char_boundary_at_or_before(source, selection.start.min(source.len()));
    let end = char_boundary_at_or_before(source, selection.end.min(source.len()));
    let (start, end) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let (start_line, start_column) = line_column_at(source, start);
    if start == end {
        return format!("{path}:{start_line}:{start_column}");
    }

    let (end_line, end_column) = line_column_at(source, end);
    if start_line == end_line {
        format!("{path}:{start_line}:{start_column}-{end_column}")
    } else {
        format!("{path}:{start_line}:{start_column}-{end_line}:{end_column}")
    }
}

pub(super) fn copy_editor_reference(
    files: crate::FilesPorts,
    reference: String,
    toast: Signal<Option<ToastState>>,
) {
    spawn(async move {
        let Some(clipboard) = files.clipboard().cloned() else {
            set_error(toast, "Clipboard access is unavailable.");
            return;
        };
        match clipboard.copy_text(&reference).await {
            Ok(()) => set_success(toast, "Copied file reference"),
            Err(error) => set_error(
                toast,
                format!("Could not copy reference: {}", error.message),
            ),
        }
    });
}

pub(super) fn text_document_contents(
    path: &str,
    documents: Signal<Vec<OpenDocument>>,
) -> Option<String> {
    documents.read().iter().find_map(|document| match document {
        OpenDocument::Text(buffer) if buffer.path == path => Some(buffer.contents.clone()),
        _ => None,
    })
}

fn char_boundary_at_or_before(source: &str, mut offset: usize) -> usize {
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn line_column_at(source: &str, offset: usize) -> (usize, usize) {
    let before = &source[..offset];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = before
        .rsplit_once('\n')
        .map_or(before, |(_, current_line)| current_line)
        .chars()
        .count()
        + 1;
    (line, column)
}

#[derive(Clone, Copy)]
pub(super) struct EditorShortcutState {
    pub(super) search_panel: Signal<bool>,
    pub(super) search_input: Signal<Option<Rc<MountedData>>>,
    pub(super) go_to_line: Signal<bool>,
}

pub(super) fn handle_editor_shortcut(
    event: &KeyboardEvent,
    files: crate::FilesPorts,
    workspace: Option<WorkspaceRecord>,
    path: String,
    documents: Signal<Vec<OpenDocument>>,
    toast: Signal<Option<ToastState>>,
    mut state: EditorShortcutState,
) {
    let modifiers = event.modifiers();
    let command = modifiers.contains(Modifiers::CONTROL) || modifiers.contains(Modifiers::META);
    if !command {
        return;
    }
    match event.key() {
        Key::Character(value) if value.eq_ignore_ascii_case("s") => {
            event.prevent_default();
            save_path(files, workspace, path, documents, toast);
        }
        Key::Character(value) if value.eq_ignore_ascii_case("f") => {
            event.prevent_default();
            state.search_panel.set(true);
            focus_file_search(state.search_input);
        }
        Key::Character(value) if value.eq_ignore_ascii_case("g") => {
            event.prevent_default();
            state.go_to_line.set(true);
        }
        _ => {}
    }
}

fn focus_file_search(search_input: Signal<Option<Rc<MountedData>>>) {
    let Some(input) = search_input() else {
        return;
    };
    spawn(async move {
        let _ = input.set_focus(true).await;
    });
}

pub(super) fn issue_command(
    mut revision: Signal<u64>,
    mut command: Signal<Option<EditorCommand>>,
    kind: EditorCommandKind,
) {
    *revision.write() += 1;
    command.set(Some(EditorCommand {
        revision: revision(),
        kind,
    }));
}

#[cfg(test)]
mod tests {
    use super::EditorSelection;
    use super::format_editor_reference;

    #[test]
    fn file_reference_formats_cursor_and_single_line_selection() {
        let source = "first\nsecond line\nthird";
        assert_eq!(
            format_editor_reference(
                "src/main.rs",
                source,
                &EditorSelection {
                    start: 8,
                    end: 8,
                    ..EditorSelection::default()
                },
            ),
            "src/main.rs:2:3"
        );
        assert_eq!(
            format_editor_reference(
                "src/main.rs",
                source,
                &EditorSelection {
                    start: 6,
                    end: 12,
                    ..EditorSelection::default()
                },
            ),
            "src/main.rs:2:1-7"
        );
    }

    #[test]
    fn file_reference_formats_multiline_utf8_selection() {
        let source = "αβ\nline two\n";
        assert_eq!(
            format_editor_reference(
                "notes.md",
                source,
                &EditorSelection {
                    start: 2,
                    end: source.len(),
                    ..EditorSelection::default()
                },
            ),
            "notes.md:1:2-3:1"
        );
    }
}
