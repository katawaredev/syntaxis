#[allow(unused_imports)] // Dioxus expands the parent glob for RSX hot-reload analysis.
use super::{
    component, dioxus_core, dioxus_elements, dioxus_signals, document, file_glyph,
    language_slug_for_path, request_close, rsx, save_path, set_error, set_success, spawn,
    ActionCallback, AnyStorage, AppIcon, ButtonExtension, CanvasExtension, CloseRequest,
    ControlSize, DataExtension, DetailsExtension, DialogExtension, DropdownMenu, DropdownMenuItem,
    DropdownMenuTrigger, EditorBuffer, EditorCommand, EditorCommandKind, EditorSelection, Element,
    EmbedExtension, EventHandler, FieldsetExtension, FormEvent, GlobalAttributesExtension,
    HasAttributes, HasFormData, HasKeyboardData, HasPointerData, History, Icon, IframeExtension,
    ImgExtension, InputExtension, Key, KeyboardEvent, Language, LiExtension, LinkExtension,
    MenuContent, MeterExtension, Modifiers, ModifiersInteraction, MpaddedExtension,
    MspaceExtension, ObjectExtension, OlExtension, OpenDocument, OpenTab, OptgroupExtension,
    OptionExtension, PanelTab, PanelTabIndicator, PanelTabWidth, ParamExtension, ProgressExtension,
    Props, ReadableExt, ReadableHashMapExt, ReadableHashSetExt, ReadableOptionExt,
    ReadableResultExt, ReadableStrExt, ReadableVecExt, SelectExtension, Signal, Storage,
    SvgAttributesExtension, TextInput, TextInputType, TextareaExtension, ToastState,
    TrackExtension, UnifiedDiff, VideoExtension, WorkspaceRecord, WritableExt, WritableStringExt,
    WritableVecExt,
};
use regex::RegexBuilder;

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

pub(super) fn copy_editor_reference(reference: String, toast: Signal<Option<ToastState>>) {
    let eval = document::eval(
        r#"
        const text = await dioxus.recv();
        try {
            if (globalThis.navigator?.clipboard?.writeText) {
                await globalThis.navigator.clipboard.writeText(text);
            } else {
                const input = document.createElement("textarea");
                input.value = text;
                input.style.position = "fixed";
                input.style.opacity = "0";
                document.body.appendChild(input);
                input.select();
                const copied = document.execCommand("copy");
                input.remove();
                if (!copied) throw new Error("The browser rejected the copy command.");
            }
            return null;
        } catch (error) {
            return error instanceof Error ? error.message : String(error);
        }
        "#,
    );
    let _ = eval.send(reference);
    spawn(async move {
        match eval.join::<Option<String>>().await {
            Ok(None) => set_success(toast, "Copied file reference"),
            Ok(Some(message)) => set_error(toast, format!("Could not copy reference: {message}")),
            Err(error) => set_error(toast, format!("Could not copy reference: {error}")),
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct SearchOptions {
    pub(super) case_sensitive: bool,
    pub(super) whole_word: bool,
    pub(super) regex: bool,
}

#[component]
pub(super) fn SearchPanel(
    mut query: Signal<String>,
    mut current: Signal<usize>,
    mut options: Signal<SearchOptions>,
    mut replacement: Signal<String>,
    mut replace_open: Signal<bool>,
    count: usize,
    error: Option<String>,
    on_next: EventHandler<i8>,
    on_replace: EventHandler<()>,
    on_replace_all: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    let active = options();
    let group_class = if error.is_some() {
        "flex min-w-0 flex-1 items-center overflow-hidden rounded-md border border-destructive bg-card/70 shadow-xs"
    } else {
        "flex min-w-0 flex-1 items-center overflow-hidden rounded-md border border-input bg-card/70 shadow-xs focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/25"
    };
    rsx! {
        div { class: "flex shrink-0 flex-col border-b border-border bg-background",
            div { class: "flex min-h-10 items-center gap-1 px-1.5",
                div { class: group_class,
                    input {
                        class: "h-7.5 min-w-0 flex-1 bg-transparent px-2 text-xs text-foreground outline-none placeholder:text-muted-foreground/70",
                        r#type: "text",
                        value: query(),
                        placeholder: "Find in file",
                        "aria-label": "Find in file",
                        "aria-invalid": error.is_some(),
                        autofocus: true,
                        oninput: move |event: FormEvent| {
                            query.set(event.value());
                            current.set(0);
                        },
                        onkeydown: move |event: KeyboardEvent| {
                            match event.key() {
                                Key::Enter => {
                                    event.prevent_default();
                                    if event.modifiers().contains(Modifiers::SHIFT) {
                                        on_next.call(-1);
                                    } else {
                                        on_next.call(1);
                                    }
                                }
                                Key::Escape => {
                                    event.prevent_default();
                                    on_close.call(());
                                }
                                _ => {}
                            }
                        },
                    }
                    SearchModeButton {
                        label: "Match case",
                        icon: AppIcon::MatchCase,
                        active: active.case_sensitive,
                        onclick: move |()| {
                            options.write().case_sensitive = !active.case_sensitive;
                            current.set(0);
                        },
                    }
                    SearchModeButton {
                        label: "Match whole word",
                        icon: AppIcon::MatchWholeWord,
                        active: active.whole_word,
                        onclick: move |()| {
                            options.write().whole_word = !active.whole_word;
                            current.set(0);
                        },
                    }
                    SearchModeButton {
                        label: "Use regular expression",
                        icon: AppIcon::Regex,
                        active: active.regex,
                        onclick: move |()| {
                            options.write().regex = !active.regex;
                            current.set(0);
                        },
                    }
                    SearchModeButton {
                        label: if replace_open() { "Hide replace" } else { "Show replace" },
                        icon: AppIcon::ToggleReplace,
                        active: replace_open(),
                        onclick: move |()| replace_open.toggle(),
                    }
                }
                span {
                    class: if error.is_some() { "min-w-10 shrink-0 text-center text-[10px] text-destructive" } else { "min-w-10 shrink-0 text-center text-[10px] tabular-nums text-muted-foreground" },
                    title: error.clone().unwrap_or_default(),
                    if error.is_some() {
                        "Invalid"
                    } else if count == 0 {
                        "0/0"
                    } else {
                        {format!("{}/{}", current().min(count - 1) + 1, count)}
                    }
                }
                button {
                    class: "grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-35",
                    r#type: "button",
                    disabled: count == 0 || error.is_some(),
                    "aria-label": "Previous match",
                    title: "Previous match (Shift Enter)",
                    onclick: move |_| on_next.call(-1),
                    Icon { icon: AppIcon::Previous, size: 14 }
                }
                button {
                    class: "grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-35",
                    r#type: "button",
                    disabled: count == 0 || error.is_some(),
                    "aria-label": "Next match",
                    title: "Next match (Enter)",
                    onclick: move |_| on_next.call(1),
                    Icon { icon: AppIcon::Next, size: 14 }
                }
                button {
                    class: "grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground",
                    r#type: "button",
                    "aria-label": "Close search",
                    title: "Close search (Escape)",
                    onclick: move |_| on_close.call(()),
                    Icon { icon: AppIcon::Close, size: 14 }
                }
            }
            if replace_open() {
                div { class: "flex min-h-10 items-center gap-1 border-t border-border/60 px-1.5",
                    div { class: "flex min-w-0 flex-1 items-center overflow-hidden rounded-md border border-input bg-card/70 shadow-xs focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/25",
                        input {
                            class: "h-7.5 min-w-0 flex-1 bg-transparent px-2 text-xs text-foreground outline-none placeholder:text-muted-foreground/70",
                            r#type: "text",
                            value: replacement(),
                            placeholder: "Replace with…",
                            "aria-label": "Replace with",
                            autofocus: true,
                            oninput: move |event: FormEvent| replacement.set(event.value()),
                            onkeydown: move |event: KeyboardEvent| {
                                match event.key() {
                                    Key::Enter => {
                                        event.prevent_default();
                                        if event.modifiers().intersects(Modifiers::CONTROL | Modifiers::META) {
                                            on_replace_all.call(());
                                        } else {
                                            on_replace.call(());
                                        }
                                    }
                                    Key::Escape => {
                                        event.prevent_default();
                                        on_close.call(());
                                    }
                                    _ => {}
                                }
                            },
                        }
                    }
                    button {
                        class: "grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-35",
                        r#type: "button",
                        disabled: count == 0 || error.is_some(),
                        "aria-label": "Replace current match",
                        title: "Replace current match (Enter)",
                        onclick: move |_| on_replace.call(()),
                        Icon { icon: AppIcon::ReplaceNext, size: 14 }
                    }
                    button {
                        class: "grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-35",
                        r#type: "button",
                        disabled: count == 0 || error.is_some(),
                        "aria-label": "Replace all matches",
                        title: "Replace all matches (Mod Enter)",
                        onclick: move |_| on_replace_all.call(()),
                        Icon { icon: AppIcon::ReplaceAll, size: 14 }
                    }
                }
            }
        }
    }
}

#[component]
fn SearchModeButton(
    label: String,
    icon: AppIcon,
    active: bool,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            class: if active { "grid size-7 shrink-0 place-items-center rounded-sm bg-accent text-foreground" } else { "grid size-7 shrink-0 place-items-center rounded-sm text-muted-foreground hover:bg-accent/70 hover:text-foreground" },
            r#type: "button",
            title: label.clone(),
            "aria-label": label,
            "aria-pressed": active,
            onclick: move |_| onclick.call(()),
            Icon { icon, size: 14 }
        }
    }
}

pub(super) fn find_matches(
    source: &str,
    query: &str,
    options: SearchOptions,
) -> Result<Vec<(usize, usize)>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let expression = build_search_regex(query, options)?;
    Ok(matching_ranges(&expression, source, options.whole_word))
}

pub(super) fn replace_search_match(
    source: &str,
    query: &str,
    replacement: &str,
    options: SearchOptions,
    range: (usize, usize),
) -> Result<String, String> {
    let expression = build_search_regex(query, options)?;
    let ranges = matching_ranges(&expression, source, options.whole_word);
    if !ranges.contains(&range) {
        return Err("The selected match is no longer available.".into());
    }
    let expanded = expand_replacement(&expression, source, range, replacement, options.regex);
    let mut next = source.to_owned();
    next.replace_range(range.0..range.1, &expanded);
    Ok(next)
}

pub(super) fn replace_all_search_matches(
    source: &str,
    query: &str,
    replacement: &str,
    options: SearchOptions,
) -> Result<String, String> {
    if query.is_empty() {
        return Ok(source.to_owned());
    }
    let expression = build_search_regex(query, options)?;
    let ranges = matching_ranges(&expression, source, options.whole_word);
    let replacements = ranges
        .into_iter()
        .map(|range| {
            let expanded =
                expand_replacement(&expression, source, range, replacement, options.regex);
            (range, expanded)
        })
        .collect::<Vec<_>>();
    let mut next = source.to_owned();
    for ((start, end), expanded) in replacements.into_iter().rev() {
        next.replace_range(start..end, &expanded);
    }
    Ok(next)
}

fn build_search_regex(query: &str, options: SearchOptions) -> Result<regex::Regex, String> {
    let pattern = if options.regex {
        query.to_owned()
    } else {
        regex::escape(query)
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!options.case_sensitive)
        .multi_line(true)
        .build()
        .map_err(|error| error.to_string())
}

fn matching_ranges(
    expression: &regex::Regex,
    source: &str,
    whole_word: bool,
) -> Vec<(usize, usize)> {
    expression
        .find_iter(source)
        .filter(|found| !whole_word || is_whole_word(source, found.start(), found.end()))
        .map(|found| (found.start(), found.end()))
        .collect()
}

fn expand_replacement(
    expression: &regex::Regex,
    source: &str,
    (start, end): (usize, usize),
    replacement: &str,
    expand_captures: bool,
) -> String {
    if !expand_captures {
        return replacement.to_owned();
    }
    let Some(captures) = expression.captures_at(source, start) else {
        return replacement.to_owned();
    };
    if captures
        .get(0)
        .is_none_or(|found| found.start() != start || found.end() != end)
    {
        return replacement.to_owned();
    }
    let mut expanded = String::new();
    captures.expand(replacement, &mut expanded);
    expanded
}

fn is_whole_word(source: &str, start: usize, end: usize) -> bool {
    let begins_at_boundary = source[..start]
        .chars()
        .next_back()
        .is_none_or(|character| !is_word_character(character));
    let ends_at_boundary = source[end..]
        .chars()
        .next()
        .is_none_or(|character| !is_word_character(character));
    begins_at_boundary && ends_at_boundary
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
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

#[cfg(test)]
mod tests {
    use super::format_editor_reference;
    use crate::files::EditorSelection;

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
