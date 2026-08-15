use super::*;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent editor compartments have independent boolean settings"
)]
struct EditorConfiguration {
    id: String,
    value: String,
    language: String,
    filename: String,
    line_numbers: bool,
    word_wrap: bool,
    tab_width: usize,
    indent_width: usize,
    indent_with_tabs: bool,
    read_only: bool,
    spellcheck: bool,
    autocomplete: bool,
    language_services: Vec<LanguageServiceConfig>,
    lsp_module: String,
    diff_original: Option<String>,
    aria_label: String,
    placeholder: String,
    search_matches: Vec<EditorRange>,
    active_search_match: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum EditorBridgeCommand {
    Configure {
        config: Box<EditorConfiguration>,
    },
    ConfigureSearch {
        query: Option<EditorSearchQuery>,
    },
    Focus,
    TriggerCompletion,
    GoToDefinition,
    FindReferences,
    FormatDocument,
    GoToLine {
        line: usize,
    },
    Select {
        start: usize,
        end: usize,
    },
    Replace {
        value: String,
        start: usize,
        end: usize,
    },
    SearchNext,
    SearchPrevious,
    Destroy,
}

impl From<EditorCommandKind> for EditorBridgeCommand {
    fn from(command: EditorCommandKind) -> Self {
        match command {
            EditorCommandKind::Focus => Self::Focus,
            EditorCommandKind::TriggerCompletion => Self::TriggerCompletion,
            EditorCommandKind::GoToDefinition => Self::GoToDefinition,
            EditorCommandKind::FindReferences => Self::FindReferences,
            EditorCommandKind::FormatDocument => Self::FormatDocument,
            EditorCommandKind::GoToLine { line } => Self::GoToLine { line },
            EditorCommandKind::Select { start, end } => Self::Select { start, end },
            EditorCommandKind::Replace { value, start, end } => Self::Replace { value, start, end },
            EditorCommandKind::SearchNext => Self::SearchNext,
            EditorCommandKind::SearchPrevious => Self::SearchPrevious,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum EditorBridgeEvent {
    Input {
        edits: Vec<EditorEdit>,
    },
    Selection {
        start: usize,
        end: usize,
        line: usize,
        column: usize,
        selection_count: usize,
        ranges: Vec<EditorRange>,
    },
    Search {
        count: usize,
        current: Option<usize>,
        valid: bool,
    },
    LanguageServiceStatus {
        server_id: String,
        server_name: String,
        status: LanguageServiceStatus,
        #[serde(default)]
        message: String,
        #[serde(default)]
        completion: bool,
        #[serde(default)]
        definition: bool,
        #[serde(default)]
        references: bool,
        #[serde(default)]
        formatting: bool,
    },
}

#[component]
pub(super) fn InteractiveCodeEditor(editor_props: CodeEditorProps) -> Element {
    let props = editor_props;
    let generated_id = use_hook(|| {
        format!(
            "dxc-editor-{}",
            EDITOR_ID_NEXT.fetch_add(1, Ordering::Relaxed)
        )
    });
    let editor_id = if props.id.is_empty() {
        generated_id
    } else {
        props.id.clone()
    };
    let configuration = EditorConfiguration {
        id: editor_id.clone(),
        value: props.value.clone(),
        language: if props.language_name.is_empty() {
            props.language.slug().to_owned()
        } else {
            props.language_name.clone()
        },
        filename: props.filename.clone(),
        line_numbers: props.line_numbers,
        word_wrap: props.word_wrap,
        tab_width: props.tab_width,
        indent_width: props.indent_width,
        indent_with_tabs: props.indent_with_tabs,
        read_only: props.read_only,
        spellcheck: props.spellcheck,
        autocomplete: props.autocomplete,
        language_services: props.language_services.clone(),
        lsp_module: LSP_MODULE.to_string(),
        diff_original: props.diff_original.clone(),
        aria_label: props.aria_label.clone(),
        placeholder: props.placeholder.clone(),
        search_matches: props.search_matches.clone(),
        active_search_match: props.active_search_match,
    };
    let search_configuration = props.search_query.clone();
    let mut event_bridge = use_signal(|| None::<dioxus::document::Eval>);
    let last_configuration = use_hook(|| Rc::new(RefCell::new(None::<EditorConfiguration>)));
    let last_search_configuration =
        use_hook(|| Rc::new(RefCell::new(None::<Option<EditorSearchQuery>>)));

    use_effect({
        let configuration = configuration.clone();
        let last_configuration = Rc::clone(&last_configuration);
        move || {
            let mut events = document::eval(EDITOR_BRIDGE);
            drop(events.send(configuration.clone()));
            *last_configuration.borrow_mut() = Some(configuration.clone());
            event_bridge.set(Some(events));
            let event_configuration = Rc::clone(&last_configuration);
            spawn(async move {
                while let Ok(event) = events.recv::<EditorBridgeEvent>().await {
                    match event {
                        EditorBridgeEvent::Input { edits } => {
                            if let Some(configuration) = event_configuration.borrow_mut().as_mut() {
                                apply_editor_edits(&mut configuration.value, &edits);
                            }
                            props.oninput.call(edits);
                        }
                        EditorBridgeEvent::Selection {
                            start,
                            end,
                            line,
                            column,
                            selection_count,
                            ranges,
                        } => props.onselection.call(EditorSelection {
                            start,
                            end,
                            line,
                            column,
                            selection_count,
                            ranges,
                        }),
                        EditorBridgeEvent::Search {
                            count,
                            current,
                            valid,
                        } => props.onsearch.call(EditorSearchStatus {
                            count,
                            current,
                            valid,
                        }),
                        EditorBridgeEvent::LanguageServiceStatus {
                            server_id,
                            server_name,
                            status,
                            message,
                            completion,
                            definition,
                            references,
                            formatting,
                        } => {
                            props.on_language_service.call(LanguageServiceState {
                                server_id,
                                server_name,
                                status,
                                message,
                                completion,
                                definition,
                                references,
                                formatting,
                            });
                        }
                    }
                }
            });
        }
    });
    use_drop(move || {
        if let Some(events) = event_bridge() {
            drop(events.send(EditorBridgeCommand::Destroy));
        }
        if let Some(mut command) = props.command {
            command.set(None);
        }
    });
    use_effect(move || {
        let Some(mut command_signal) = props.command else {
            return;
        };
        let Some(command) = command_signal() else {
            return;
        };
        let Some(events) = event_bridge() else {
            return;
        };
        if events.send(EditorBridgeCommand::from(command.kind)).is_ok() {
            command_signal.set(None);
        }
    });

    if last_configuration.borrow().as_ref() != Some(&configuration)
        && let Some(events) = event_bridge()
        && events
            .send(EditorBridgeCommand::Configure {
                config: Box::new(configuration.clone()),
            })
            .is_ok()
    {
        *last_configuration.borrow_mut() = Some(configuration);
    }
    if last_search_configuration.borrow().as_ref() != Some(&search_configuration)
        && let Some(events) = event_bridge()
        && events
            .send(EditorBridgeCommand::ConfigureSearch {
                query: search_configuration.clone(),
            })
            .is_ok()
    {
        *last_search_configuration.borrow_mut() = Some(search_configuration);
    }
    let class = editor_class(
        props.theme,
        props.line_numbers,
        props.word_wrap,
        &props.class,
    );
    let diff_class = if props.diff_original.is_some() {
        "dxc-codemirror-diff"
    } else {
        Default::default()
    };
    rsx! {
        CodeThemeStyles { theme: props.theme }
        document::Stylesheet { href: CODE_EDITOR_CSS }
        div {
            id: editor_id,
            class: "{class} dxc-codemirror {diff_class}",
            style: "--dxc-editor-tab-width: {props.tab_width.max(1)}",
            onkeydown: move |event| props.onkeydown.call(event),
        }
    }
}

fn apply_editor_edits(value: &mut String, edits: &[EditorEdit]) -> bool {
    let mut normalized = Vec::with_capacity(edits.len());
    for edit in edits {
        let Some(start) = utf16_offset_to_byte(value, edit.start) else {
            return false;
        };
        let Some(end) = utf16_offset_to_byte(value, edit.end) else {
            return false;
        };
        if start > end {
            return false;
        }
        normalized.push((start, end, edit.text.as_str()));
    }
    normalized.sort_unstable_by(|left, right| right.0.cmp(&left.0).then(right.1.cmp(&left.1)));
    if !normalized.windows(2).all(|pair| pair[0].0 >= pair[1].1) {
        return false;
    }
    for (start, end, text) in normalized {
        value.replace_range(start..end, text);
    }
    true
}

fn utf16_offset_to_byte(value: &str, target: usize) -> Option<usize> {
    let mut utf16_offset = 0;
    for (byte_offset, character) in value.char_indices() {
        if utf16_offset == target {
            return Some(byte_offset);
        }
        utf16_offset += character.len_utf16();
        if utf16_offset > target {
            return None;
        }
    }
    (utf16_offset == target).then_some(value.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_edits_advance_the_cached_value_with_utf16_offsets() {
        let mut value = "a😀bc".to_owned();
        assert!(apply_editor_edits(
            &mut value,
            &[
                EditorEdit {
                    start: 1,
                    end: 3,
                    text: "🙂".into(),
                },
                EditorEdit {
                    start: 4,
                    end: 5,
                    text: "d".into(),
                },
            ],
        ));
        assert_eq!(value, "a🙂bd");
    }
}
