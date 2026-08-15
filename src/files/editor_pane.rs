#[allow(
    unused_imports,
    reason = "Dioxus expands the parent glob for RSX hot-reload analysis"
)]
use super::*;

#[derive(Clone, PartialEq)]
pub(super) struct EditorPaneState {
    pub(super) workspace: Signal<Option<WorkspaceRecord>>,
    pub(super) documents: Signal<Vec<OpenDocument>>,
    pub(super) loading_path: Signal<Option<String>>,
    pub(super) diff: Signal<Option<UnifiedDiff>>,
    pub(super) markdown_preview: Signal<bool>,
    pub(super) svg_preview: Signal<bool>,
    pub(super) csv_preview: Signal<bool>,
    pub(super) language_service_connections: Signal<Vec<LanguageServiceConfig>>,
    pub(super) language_service_states: Signal<Vec<LanguageServiceState>>,
    pub(super) editor_command: Signal<Option<EditorCommand>>,
    pub(super) line_numbers: Signal<bool>,
    pub(super) word_wrap: Signal<bool>,
    pub(super) autocomplete_enabled: Signal<bool>,
    pub(super) search_panel: Signal<bool>,
    pub(super) search_options: Signal<SearchOptions>,
    pub(super) search_query: Signal<String>,
    pub(super) search_match: Signal<usize>,
    pub(super) editor_search_status: Signal<EditorSearchStatus>,
    pub(super) editor_selection: Signal<EditorSelection>,
    pub(super) search_input: Signal<Option<std::rc::Rc<MountedData>>>,
    pub(super) go_to_line: Signal<bool>,
    pub(super) toast: Signal<Option<ToastState>>,
}

#[component]
pub(super) fn EditorPane(
    active_document: Option<ActiveDocumentView>,
    active_markdown: bool,
    active_svg: bool,
    active_csv: bool,
    initial_loading: bool,
    initial_failed: bool,
    workspace_editor_matches: Vec<EditorRange>,
    state: EditorPaneState,
) -> Element {
    let EditorPaneState {
        workspace,
        documents,
        loading_path,
        diff,
        markdown_preview,
        svg_preview,
        csv_preview,
        language_service_connections,
        mut language_service_states,
        editor_command,
        line_numbers,
        word_wrap,
        autocomplete_enabled,
        search_panel,
        search_options,
        search_query,
        mut search_match,
        mut editor_search_status,
        mut editor_selection,
        search_input,
        go_to_line,
        toast,
    } = state;

    rsx! {
        div { class: "relative min-h-0 min-w-0 flex-1 overflow-auto bg-card",
            if active_document.is_some() {
                if let Some(path) = loading_path() {
                    div { class: "pointer-events-none sticky top-2 z-20 h-0 overflow-visible",
                        div { class: "ml-auto mr-3 w-fit rounded-md border border-border bg-popover/95 px-2.5 py-1.5 text-[10px] text-muted-foreground shadow-lg backdrop-blur-sm",
                            "Opening {file_label(&path)}…"
                        }
                    }
                }
            }
            match active_document {
                None => rsx! {
                    EmptyEditor {
                        loading: loading_path()
                            .map(|path| format!("Opening {}…", file_label(&path)))
                            .or_else(|| initial_loading.then(|| "Loading workspace…".into())),
                        unavailable: initial_failed,
                    }
                },
                Some(
                    ActiveDocumentView::Text { contents, .. },
                ) if diff().is_none() && active_markdown && markdown_preview() => {
                    rsx! {
                        MarkdownPreview { source: contents }
                    }
                }
                Some(
                    ActiveDocumentView::Text { path, contents, .. },
                ) if diff().is_none() && active_svg && svg_preview() => rsx! {
                    SafeSvgPreview { source: contents, path }
                },
                Some(
                    ActiveDocumentView::Text { path, contents, .. },
                ) if diff().is_none() && active_csv && csv_preview() => rsx! {
                    CsvPreview { source: contents, path }
                },
                Some(ActiveDocumentView::Text { path, contents, status, config }) => {
                    let language = language_for_path(&path);
                    let language_slug = language_slug_for_path(&path);
                    let configured_language_services = if let Some(workspace) = workspace() {
                        let servers = language_servers_for_language(
                            language_slug,
                            &workspace.profile.technologies,
                        );
                        let connections = language_service_connections();
                        servers
                            .iter()
                            .filter_map(|server| {
                                connections
                                    .iter()
                                    .find(|connection| connection.server_id == server.id)
                                    .cloned()
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let reload_path = path.clone();
                    let input_path = path.clone();
                    let diff_original = diff().map(|diff| diff.original.unwrap_or_default());
                    rsx! {
                        div { class: "relative size-full min-h-0",
                            if status == BufferStatus::Conflict {
                                div { class: "absolute top-2 right-3 z-10 flex items-center gap-2 rounded-md border border-warning/40 bg-popover px-2.5 py-1.5 text-[10px] shadow-lg",
                                    span { class: "text-warning", "File changed on disk" }
                                    button {
                                        class: "text-primary hover:underline",
                                        onclick: move |_| {
                                            if let Some(workspace) = workspace() {
                                                reload_document(workspace, reload_path.clone(), documents, toast);
                                            }
                                        },
                                        "Reload"
                                    }
                                }
                            }
                            CodeEditor {
                                id: "syntaxis-active-editor",
                                class: "size-full min-h-full rounded-none",
                                value: contents,
                                language,
                                language_name: language_slug,
                                filename: path.clone(),
                                line_numbers: line_numbers(),
                                word_wrap: word_wrap(),
                                tab_width: config.tab_width,
                                indent_width: config.indent_size,
                                indent_with_tabs: config.indent_style == IndentStyle::Tabs,
                                autocomplete: autocomplete_enabled(),
                                language_services: configured_language_services,
                                command: Some(editor_command),
                                search_matches: if search_panel() { Vec::new() } else { workspace_editor_matches.clone() },
                                active_search_match: if search_panel() { None } else { (!workspace_editor_matches.is_empty()).then_some(0) },
                                search_query: search_panel()
                                    .then(|| {
                                        let options = search_options();
                                        EditorSearchQuery {
                                            query: search_query(),
                                            case_sensitive: options.case_sensitive,
                                            whole_word: options.whole_word,
                                            regex: options.regex,
                                        }
                                    }),
                                diff_original,
                                onsearch: move |status: EditorSearchStatus| {
                                    if let Some(current) = status.current {
                                        search_match.set(current);
                                    }
                                    editor_search_status.set(status);
                                },
                                onselection: move |selection: EditorSelection| editor_selection.set(selection),
                                oninput: move |edits: Vec<EditorEdit>| apply_document_edits(&input_path, &edits, documents),
                                on_language_service: move |state: LanguageServiceState| {
                                    let mut states = language_service_states.write();
                                    if let Some(current) = states
                                        .iter_mut()
                                        .find(|current| current.server_id == state.server_id)
                                    {
                                        *current = state;
                                    } else {
                                        states.push(state);
                                    }
                                },
                                onkeydown: move |event| handle_editor_shortcut(
                                    &event,
                                    workspace(),
                                    path.clone(),
                                    documents,
                                    toast,
                                    EditorShortcutState {
                                        search_panel,
                                        search_input,
                                        go_to_line,
                                    },
                                ),
                            }
                        }
                    }
                }
                Some(ActiveDocumentView::Image { path, data_url, size }) => rsx! {
                    ImagePreview { path, data_url, size }
                },
                Some(ActiveDocumentView::Large { path, size }) => rsx! {
                    UnsupportedPreview {
                        path,
                        size,
                        title: "File is too large",
                        reason: "Files larger than 4 MiB are not loaded into the editor.",
                    }
                },
                Some(ActiveDocumentView::Unsupported { path, size, reason }) => rsx! {
                    UnsupportedPreview {
                        path,
                        size,
                        title: "Preview unavailable",
                        reason,
                    }
                },
            }
        }
    }
}
