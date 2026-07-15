#[allow(unused_imports)] // Dioxus expands the parent glob for RSX hot-reload analysis.
use super::{
    component, dioxus_core, dioxus_elements, dioxus_signals, file_glyph, language_slug_for_path,
    request_close, rsx, save_path, ActionCallback, AnyStorage, ButtonExtension, CanvasExtension,
    CloseRequest, ControlSize, DataExtension, DetailsExtension, DialogExtension, DropdownMenu,
    DropdownMenuItem, DropdownMenuTrigger, EditorBuffer, EditorCommand, EditorCommandKind,
    EditorSelection, Element, EmbedExtension, EventHandler, FieldsetExtension, FileAction,
    FormEvent, GlobalAttributesExtension, HasAttributes, HasFormData, HasKeyboardData,
    HasPointerData, History, IframeExtension, ImgExtension, InputExtension, Key, KeyboardEvent,
    Language, LiExtension, LinkExtension, MenuContent, MeterExtension, Modifiers,
    ModifiersInteraction, MpaddedExtension, MspaceExtension, ObjectExtension, OlExtension,
    OpenDocument, OpenTab, OptgroupExtension, OptionExtension, PanelTab, PanelTabIndicator,
    PanelTabWidth, ParamExtension, ProgressExtension, Props, ReadableExt, ReadableHashMapExt,
    ReadableHashSetExt, ReadableOptionExt, ReadableResultExt, ReadableStrExt, ReadableVecExt,
    SelectExtension, Signal, Storage, SvgAttributesExtension, TextInput, TextInputType,
    TextareaExtension, ToastState, TrackExtension, UnifiedDiff, VideoExtension, WorkspaceRecord,
    WritableExt, WritableStringExt, WritableVecExt,
};

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
            on_close: move |()| request_close(close_path.clone(), documents, close_request),
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
            DropdownMenuTrigger {
                class: "flex h-10 w-full items-center justify-between gap-2 rounded-md border border-input bg-background px-3 text-left text-xs text-foreground",
                "aria-label": "Open file tabs",
                span { class: "truncate", {active_path().unwrap_or_else(|| "No file open".into())} }
                span { "⌄" }
            }
            MenuContent { class: "right-2 left-2 w-auto",
                for (index, tab) in tabs.into_iter().enumerate() {
                    DropdownMenuItem::<String> {
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

#[component]
pub(super) fn EditorMenuItem(
    index: usize,
    label: String,
    #[props(default)] suffix: String,
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
            span { "{label}" }
            if !suffix.is_empty() {
                kbd { "{suffix}" }
            }
        }
    }
}

#[component]
pub(super) fn ExplorerActionItem(
    index: usize,
    value: FileAction,
    label: String,
    disabled: bool,
    #[props(default = false)] danger: bool,
    on_select: EventHandler<FileAction>,
) -> Element {
    rsx! {
        DropdownMenuItem::<FileAction> {
            value,
            index,
            disabled,
            class: if danger { "!text-destructive" } else { "" },
            on_select: move |action| on_select.call(action),
            "{label}"
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_editor_shortcut(
    event: &KeyboardEvent,
    workspace: Option<WorkspaceRecord>,
    path: String,
    documents: Signal<Vec<OpenDocument>>,
    toast: Signal<Option<ToastState>>,
    mut search_panel: Signal<bool>,
    mut go_to_line: Signal<bool>,
    mut autocomplete: Signal<bool>,
) {
    let modifiers = event.modifiers();
    let command = modifiers.contains(Modifiers::CONTROL) || modifiers.contains(Modifiers::META);
    if !command {
        return;
    }
    match event.key() {
        Key::Character(value) if value.eq_ignore_ascii_case("s") => {
            event.prevent_default();
            save_path(workspace, path, documents, toast);
        }
        Key::Character(value) if value.eq_ignore_ascii_case("f") => {
            event.prevent_default();
            search_panel.set(true);
        }
        Key::Character(value) if value.eq_ignore_ascii_case("g") => {
            event.prevent_default();
            go_to_line.set(true);
        }
        Key::Character(value) if value == " " => {
            event.prevent_default();
            autocomplete.set(true);
        }
        _ => {}
    }
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

#[component]
pub(super) fn SearchPanel(
    mut query: Signal<String>,
    current: usize,
    count: usize,
    on_next: EventHandler<i8>,
    on_close: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "flex min-h-10 items-center gap-1.5 border-b border-border bg-background px-2",
            TextInput {
                size: ControlSize::Small,
                input_type: TextInputType::Search,
                value: query(),
                placeholder: "Find in file",
                aria_label: "Find in file",
                autofocus: true,
                oninput: move |event: FormEvent| query.set(event.value()),
            }
            span { class: "min-w-14 text-center text-[10px] text-muted-foreground",
                if count == 0 {
                    "No matches"
                } else {
                    {format!("{} / {count}", current + 1)}
                }
            }
            button {
                class: "size-7 text-muted-foreground hover:text-foreground",
                "aria-label": "Previous match",
                onclick: move |_| on_next.call(-1),
                "↑"
            }
            button {
                class: "size-7 text-muted-foreground hover:text-foreground",
                "aria-label": "Next match",
                onclick: move |_| on_next.call(1),
                "↓"
            }
            button {
                class: "size-7 text-muted-foreground hover:text-foreground",
                "aria-label": "Close search",
                onclick: move |_| on_close.call(()),
                "×"
            }
        }
    }
}

pub(super) fn find_matches(source: &str, query: &str) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    source
        .match_indices(query)
        .map(|(start, found)| (start, start + found.len()))
        .collect()
}

#[component]
pub(super) fn CompletionMenu(
    buffer: EditorBuffer,
    selection: EditorSelection,
    on_select: EventHandler<String>,
    on_close: EventHandler<()>,
) -> Element {
    let completions = completions_for(&buffer, selection.start);
    rsx! {
        div {
            class: "absolute top-4 right-4 z-20 w-48 rounded-md border border-border bg-popover p-1 text-xs shadow-xl",
            role: "listbox",
            "aria-label": "Code completions",
            div { class: "flex items-center justify-between px-2 py-1 text-[9px] text-muted-foreground",
                span { "COMPLETIONS" }
                button { onclick: move |_| on_close.call(()), "×" }
            }
            if completions.is_empty() {
                div { class: "px-2 py-1.5 text-muted-foreground", "No suggestions" }
            }
            for completion in completions {
                button {
                    class: "block w-full rounded-sm px-2 py-1.5 text-left text-foreground hover:bg-accent",
                    role: "option",
                    onclick: move |_| on_select.call(completion.clone()),
                    "{completion}"
                }
            }
        }
    }
}

pub(super) fn completions_for(buffer: &EditorBuffer, cursor: usize) -> Vec<String> {
    let start = word_start(&buffer.contents, cursor.min(buffer.contents.len()));
    let prefix = &buffer.contents[start..cursor.min(buffer.contents.len())];
    language_completions(language_slug_for_path(&buffer.path))
        .iter()
        .filter(|candidate| candidate.starts_with(prefix) && **candidate != prefix)
        .take(8)
        .map(|candidate| (*candidate).to_owned())
        .collect()
}

pub(super) fn language_completions(language: &str) -> &'static [&'static str] {
    match language {
        "rust" => &[
            "async", "await", "const", "enum", "fn", "impl", "let", "match", "move", "pub",
            "Result", "Self", "struct", "trait", "use",
        ],
        "javascript" | "typescript" | "tsx" => &[
            "async",
            "await",
            "const",
            "export",
            "function",
            "import",
            "interface",
            "let",
            "return",
            "type",
        ],
        "python" => &[
            "async", "await", "class", "def", "from", "import", "return", "with", "yield",
        ],
        _ => &["false", "null", "true"],
    }
}

pub(super) fn apply_completion(
    path: &str,
    completion: &str,
    selection: &EditorSelection,
    mut documents: Signal<Vec<OpenDocument>>,
    revision: Signal<u64>,
    command: Signal<Option<EditorCommand>>,
) {
    if let Some(OpenDocument::Text(buffer)) = documents
        .write()
        .iter_mut()
        .find(|document| document.path() == path)
    {
        let cursor = selection.start.min(buffer.contents.len());
        let start = word_start(&buffer.contents, cursor);
        let mut next = buffer.contents.clone();
        next.replace_range(start..cursor, completion);
        buffer.edit(next);
        let caret = start + completion.len();
        issue_command(
            revision,
            command,
            EditorCommandKind::Select {
                start: caret,
                end: caret,
            },
        );
    }
}

pub(super) fn word_start(source: &str, cursor: usize) -> usize {
    source[..cursor]
        .char_indices()
        .rev()
        .find_map(|(index, character)| {
            (!character.is_alphanumeric() && character != '_')
                .then_some(index + character.len_utf8())
        })
        .unwrap_or(0)
}

pub(super) fn language_for_path(path: &str) -> Language {
    Language::from_slug(language_slug_for_path(path)).unwrap_or(Language::Rust)
}
