#[allow(unused_imports)] // Dioxus expands the parent glob for RSX hot-reload analysis.
use super::{
    complete_any_word, complete_with_words, component, dioxus_core, dioxus_elements,
    dioxus_signals, document, file_glyph, generated_completion_words, language_slug_for_path,
    request_close, rsx, save_path, set_error, set_success, spawn, ActionCallback, AnyStorage,
    AppIcon, ButtonExtension, CanvasExtension, CloseRequest, ControlSize, DataExtension,
    DetailsExtension, DialogExtension, DropdownMenu, DropdownMenuItem, EditorBuffer, EditorCommand,
    EditorCommandKind, EditorSelection, Element, EmbedExtension, EventHandler, FieldsetExtension,
    FormEvent, GlobalAttributesExtension, HasAttributes, HasFormData, HasKeyboardData,
    HasPointerData, History, Icon, IframeExtension, ImgExtension, InputExtension, Key,
    KeyboardEvent, Language, LiExtension, LinkExtension, MenuButtonTrigger, MenuContent,
    MeterExtension, Modifiers, ModifiersInteraction, MpaddedExtension, MspaceExtension,
    ObjectExtension, OlExtension, OpenDocument, OpenTab, OptgroupExtension, OptionExtension,
    PanelTab, PanelTabIndicator, PanelTabWidth, ParamExtension, ProgressExtension, Props,
    ReadableExt, ReadableHashMapExt, ReadableHashSetExt, ReadableOptionExt, ReadableResultExt,
    ReadableStrExt, ReadableVecExt, SelectExtension, Signal, Storage, SvgAttributesExtension,
    TextInput, TextInputType, TextareaExtension, ToastState, TrackExtension, UnifiedDiff,
    VideoExtension, WorkspaceRecord, WritableExt, WritableStringExt, WritableVecExt,
};
use regex::RegexBuilder;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

type CompletionDictionary = Arc<[String]>;
type CompletionDictionaryCache = Mutex<HashMap<&'static str, CompletionDictionary>>;

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
    autocomplete_enabled: bool,
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
        Key::Character(value) if value == " " && autocomplete_enabled => {
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
            class: "absolute top-2 right-2 z-20 max-h-[50dvh] w-64 max-w-[calc(100%-1rem)] overflow-y-auto rounded-md border border-border bg-popover p-1 text-sm shadow-xl",
            role: "listbox",
            "aria-label": "Code completions",
            div { class: "sticky top-0 flex min-h-9 items-center justify-between bg-popover px-2 py-1 text-[10px] text-muted-foreground",
                span { "COMPLETIONS" }
                button {
                    class: "flex size-9 items-center justify-center rounded-sm text-base hover:bg-accent",
                    "aria-label": "Close completions",
                    onclick: move |_| on_close.call(()),
                    "×"
                }
            }
            if completions.is_empty() {
                div { class: "px-2 py-2 text-muted-foreground", "No suggestions" }
            }
            for completion in completions {
                button {
                    class: "block min-h-11 w-full rounded-sm px-3 py-2 text-left text-foreground hover:bg-accent active:bg-accent",
                    role: "option",
                    onclick: move |_| on_select.call(completion.clone()),
                    "{completion}"
                }
            }
        }
    }
}

pub(super) fn completions_for(buffer: &EditorBuffer, cursor: usize) -> Vec<String> {
    let words = grammar_completion_words(language_slug_for_path(&buffer.path));
    complete_with_words(&buffer.contents, cursor, &words, 8).options
}

pub(super) fn should_open_completions(buffer: &EditorBuffer, cursor: usize) -> bool {
    let cursor = cursor.min(buffer.contents.len());
    let words = grammar_completion_words(language_slug_for_path(&buffer.path));
    let completions = complete_with_words(&buffer.contents, cursor, &words, 8);
    completions.from < cursor && !completions.options.is_empty()
}

fn grammar_completion_words(language: &'static str) -> CompletionDictionary {
    static CACHE: OnceLock<CompletionDictionaryCache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(words) = cache.get(language) {
        return Arc::clone(words);
    }

    let mut words = arborium::get_language(language).map_or_else(Vec::new, |grammar| {
        (0..grammar.node_kind_count())
            .filter_map(|id| u16::try_from(id).ok())
            .filter(|id| !grammar.node_kind_is_named(*id))
            .filter_map(|id| grammar.node_kind_for_id(id))
            .filter(|word| is_grammar_word(word))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    });
    words.extend(
        generated_completion_words(language)
            .iter()
            .map(|word| (*word).to_owned()),
    );
    words.sort_unstable();
    words.dedup();
    let words = Arc::<[String]>::from(words);
    cache.insert(language, Arc::clone(&words));
    words
}

fn is_grammar_word(word: &str) -> bool {
    word.chars()
        .next()
        .is_some_and(|character| character.is_alphabetic() || character == '_')
        && word
            .chars()
            .all(|character| character.is_alphanumeric() || character == '_')
}

pub(super) fn apply_completion(
    path: &str,
    completion: &str,
    selection: &EditorSelection,
    documents: Signal<Vec<OpenDocument>>,
    revision: Signal<u64>,
    command: Signal<Option<EditorCommand>>,
) {
    let Some(source) = text_document_contents(path, documents) else {
        return;
    };
    issue_command(
        revision,
        command,
        completion_command(&source, completion, selection.start),
    );
}

fn completion_command(source: &str, completion: &str, cursor: usize) -> EditorCommandKind {
    let cursor = cursor.min(source.len());
    let start = complete_any_word(source, cursor, 0).from;
    let mut value = source.to_owned();
    value.replace_range(start..cursor, completion);
    let caret = start + completion.len();
    EditorCommandKind::Replace {
        value,
        start: caret,
        end: caret,
    }
}

pub(super) fn language_for_path(path: &str) -> Language {
    Language::from_slug(language_slug_for_path(path)).unwrap_or(Language::Rust)
}

#[cfg(test)]
mod tests {
    use super::{
        completion_command, completions_for, format_editor_reference, grammar_completion_words,
        should_open_completions,
    };
    use crate::files::{EditorBuffer, EditorSelection};
    use dioxus_code_editor::EditorCommandKind;
    use syntaxis_editor::EditorConfig;
    use syntaxis_workspace::FileVersion;

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

    #[test]
    fn completion_dictionary_comes_from_the_enabled_grammar() {
        let rust = grammar_completion_words("rust");
        let javascript = grammar_completion_words("javascript");
        let typescript = grammar_completion_words("typescript");
        let html = grammar_completion_words("html");
        let css = grammar_completion_words("css");
        let sql = grammar_completion_words("sql");

        assert!(rust.iter().any(|word| word == "fn"));
        assert!(rust.iter().any(|word| word == "struct"));
        assert!(!rust.iter().any(|word| word == "identifier"));
        assert!(javascript.iter().any(|word| word == "document"));
        assert!(!javascript
            .iter()
            .any(|word| word == "AddEventListenerOptions"));
        assert!(typescript
            .iter()
            .any(|word| word == "AddEventListenerOptions"));
        assert!(html.iter().any(|word| word == "input"));
        assert!(css.iter().any(|word| word == "display"));
        assert!(sql.iter().any(|word| word == "select"));
    }

    #[test]
    fn automatic_completion_opens_only_for_a_prefix_with_candidates() {
        let version = FileVersion {
            length: 1,
            modified_unix_nanos: 0,
        };
        let rust = EditorBuffer::open(
            "src/main.rs",
            "f".into(),
            version.clone(),
            EditorConfig::default(),
        );
        let no_match = EditorBuffer::open(
            "src/main.rs",
            "zzz".into(),
            version,
            EditorConfig::default(),
        );

        assert!(should_open_completions(&rust, 1));
        assert!(!should_open_completions(&no_match, 3));
    }

    #[test]
    fn generated_completion_candidates_are_reachable_from_common_prefixes() {
        let version = FileVersion {
            length: 1,
            modified_unix_nanos: 0,
        };
        for (path, prefix, expected) in [
            ("src/app.js", "doc", "document"),
            ("src/app.ts", "AddEvent", "AddEventListenerOptions"),
            ("index.html", "inp", "input"),
            ("styles.css", "disp", "display"),
            ("query.sql", "sel", "select"),
        ] {
            let buffer = EditorBuffer::open(
                path,
                prefix.into(),
                version.clone(),
                EditorConfig::default(),
            );

            assert!(
                completions_for(&buffer, prefix.len())
                    .iter()
                    .any(|word| word == expected),
                "{expected} should complete {prefix} in {path}",
            );
        }
    }

    #[test]
    fn accepting_completion_is_one_editor_history_transaction() {
        assert_eq!(
            completion_command("set", "setCounter", 3),
            EditorCommandKind::Replace {
                value: "setCounter".into(),
                start: 10,
                end: 10,
            }
        );
    }
}
