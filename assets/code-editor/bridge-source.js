import {
	autocompletion,
	closeBrackets,
	closeBracketsKeymap,
	completeAnyWord,
} from "@codemirror/autocomplete";
import {
	addCursorAbove,
	addCursorBelow,
	defaultKeymap,
	history,
	historyKeymap,
	indentWithTab,
} from "@codemirror/commands";
import {
	bracketMatching,
	defaultHighlightStyle,
	HighlightStyle,
	indentOnInput,
	indentUnit,
	LanguageDescription,
	syntaxHighlighting,
} from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { unifiedMergeView } from "@codemirror/merge";
import {
	closeSearchPanel,
	findNext,
	findPrevious,
	getSearchQuery,
	openSearchPanel,
	search,
	SearchQuery,
	selectNextOccurrence,
	selectSelectionMatches,
	setSearchQuery,
} from "@codemirror/search";
import {
	Compartment,
	EditorSelection,
	EditorState,
	StateEffect,
	StateField,
} from "@codemirror/state";
import {
	crosshairCursor,
	Decoration,
	drawSelection,
	dropCursor,
	EditorView,
	highlightActiveLine,
	highlightActiveLineGutter,
	highlightSpecialChars,
	keymap,
	lineNumbers,
	placeholder,
	rectangularSelection,
} from "@codemirror/view";
import { tags } from "@lezer/highlight";

(async () => {
	const initial = await dioxus.recv();
	const mount = document.getElementById(initial.id);
	if (!(mount instanceof HTMLElement)) return;

	const encoder = new TextEncoder();
	const byteToCodeUnit = (value, byteOffset) => {
		let bytes = 0;
		let codeUnits = 0;
		for (const character of value) {
			const width = encoder.encode(character).length;
			if (bytes + width > byteOffset) break;
			bytes += width;
			codeUnits += character.length;
		}
		return codeUnits;
	};
	const codeUnitToByte = (value, offset) =>
		encoder.encode(value.slice(0, offset)).length;

	const languageCompartment = new Compartment();
	const languageServiceCompartment = new Compartment();
	const autocompleteCompartment = new Compartment();
	const diffCompartment = new Compartment();
	const lineNumbersCompartment = new Compartment();
	const wrappingCompartment = new Compartment();
	const tabSizeCompartment = new Compartment();
	const indentCompartment = new Compartment();
	const readOnlyCompartment = new Compartment();
	const attributesCompartment = new Compartment();
	const placeholderCompartment = new Compartment();

	const setSearchRanges = StateEffect.define();
	const searchRanges = StateField.define({
		create: () => Decoration.none,
		update(value, transaction) {
			value = value.map(transaction.changes);
			for (const effect of transaction.effects) {
				if (effect.is(setSearchRanges)) value = effect.value;
			}
			return value;
		},
		provide: (field) => EditorView.decorations.from(field),
	});

	const searchDecorations = (value, ranges, activeRange) => {
		const marks = [];
		for (let index = 0; index < ranges.length; index += 1) {
			const range = ranges[index];
			const from = byteToCodeUnit(value, range.start);
			const to = byteToCodeUnit(value, range.end);
			if (from >= to || to > value.length) continue;
			marks.push(
				Decoration.mark({
					class:
						index === activeRange
							? "dxc-cm-search-match dxc-cm-search-active"
							: "dxc-cm-search-match",
				}).range(from, to),
			);
		}
		return Decoration.set(marks, true);
	};

	const theme = EditorView.theme({
		"&": {
			height: "100%",
			minHeight: "100%",
			backgroundColor: "var(--dxc-editor-background)",
			color: "var(--dxc-editor-foreground)",
			fontFamily: "var(--dxc-editor-font-family)",
			fontSize: "var(--dxc-editor-font-size)",
		},
		".cm-scroller": {
			overflow: "auto",
			fontFamily: "inherit",
			lineHeight: "var(--dxc-editor-line-height)",
		},
		".cm-content": {
			minHeight: "100%",
			padding: "14px 22px 80px 8px",
			caretColor: "var(--dxc-editor-foreground)",
		},
		".cm-line": { padding: "0" },
		".cm-gutters": {
			minHeight: "100%",
			borderRight: "1px solid var(--border)",
			backgroundColor: "var(--dxc-editor-gutter-background)",
			color: "var(--dxc-editor-muted-foreground)",
		},
		".cm-lineNumbers .cm-gutterElement": {
			minWidth: "52px",
			padding: "0 13px",
		},
		"&.cm-focused": { outline: "none" },
		"&.cm-focused .cm-cursor": {
			borderLeftColor: "var(--dxc-editor-foreground)",
		},
		"&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection":
			{
				backgroundColor: "var(--dxc-editor-selection) !important",
			},
		".cm-activeLine": {
			backgroundColor: "color-mix(in oklch, var(--primary) 5%, transparent)",
		},
		".cm-activeLineGutter": {
			backgroundColor: "color-mix(in oklch, var(--primary) 8%, transparent)",
			color: "var(--dxc-editor-foreground)",
		},
		".cm-placeholder": {
			color: "var(--dxc-editor-muted-foreground)",
			fontStyle: "normal",
		},
	});

	const highlightStyle = HighlightStyle.define([
		{ tag: tags.comment, color: "var(--dxc-light-a-c-color, #565f89)" },
		{
			tag: [tags.keyword, tags.modifier],
			color: "var(--dxc-light-a-k-color, #bb9af7)",
		},
		{
			tag: [tags.string, tags.regexp],
			color: "var(--dxc-light-a-s-color, #9ece6a)",
		},
		{
			tag: [tags.number, tags.bool, tags.null],
			color: "var(--dxc-light-a-l-color, #ff9e64)",
		},
		{
			tag: [tags.function(tags.variableName), tags.function(tags.propertyName)],
			color: "var(--dxc-light-a-f-color, #7aa2f7)",
		},
		{
			tag: [tags.typeName, tags.className, tags.namespace],
			color: "var(--dxc-light-a-t-color, #2ac3de)",
		},
		{
			tag: [tags.propertyName, tags.attributeName],
			color: "var(--dxc-light-a-v-color, #c0caf5)",
		},
		{
			tag: [tags.operator, tags.punctuation],
			color: "var(--dxc-light-a-o-color, #89ddff)",
		},
		{
			tag: [tags.tagName, tags.heading],
			color: "var(--dxc-light-a-td-color, #f7768e)",
		},
		{
			tag: [tags.link, tags.url],
			color: "var(--dxc-light-a-tu-color, #7aa2f7)",
			textDecoration: "underline",
		},
		{ tag: tags.invalid, color: "var(--destructive)" },
	]);

	const languageAliases = {
		bash: "shell",
		"c-sharp": "csharp",
		cpp: "c++",
		ini: "properties",
	};
	let languageRevision = 0;
	const loadLanguage = async (config) => {
		const revision = ++languageRevision;
		const name = languageAliases[config.language] ?? config.language;
		const description =
			LanguageDescription.matchFilename(languages, config.filename) ??
			(name === "plaintext"
				? null
				: LanguageDescription.matchLanguageName(languages, name, false));
		const support = description ? await description.load() : [];
		if (revision === languageRevision && view) {
			view.dispatch({ effects: languageCompartment.reconfigure(support) });
		}
	};

	const editorAttributes = (config) => ({
		"aria-label": config.aria_label,
		"aria-multiline": "true",
		"aria-readonly": config.read_only ? "true" : "false",
		autocapitalize: "off",
		autocomplete: "off",
		autocorrect: "off",
		spellcheck: config.spellcheck ? "true" : "false",
	});
	const indentText = (config) =>
		config.indent_with_tabs
			? "\t"
			: " ".repeat(Math.max(1, config.indent_width));
	const isDiff = (config) => config.diff_original != null;
	const diffExtension = (config) =>
		isDiff(config)
			? unifiedMergeView({
					original: config.diff_original,
					highlightChanges: true,
					gutter: config.line_numbers,
					syntaxHighlightDeletions: true,
					allowInlineDiffs: true,
					mergeControls: false,
					diffConfig: {
						scanLimit: 500,
						timeout: 100,
					},
					collapseUnchanged: {
						margin: 4,
						minSize: 12,
					},
				})
			: [];

	let suppressInput = false;
	let currentConfig = initial;
	let view;
	let languageServiceRevision = 0;
	let languageServiceHandles = [];
	const releaseLanguageService = () => {
		languageServiceRevision += 1;
		for (const handle of languageServiceHandles) handle.release();
		languageServiceHandles = [];
	};
	const configureLanguageService = async (config) => {
		const revision = ++languageServiceRevision;
		for (const handle of languageServiceHandles) handle.release();
		languageServiceHandles = [];
		view.dispatch({ effects: languageServiceCompartment.reconfigure([]) });
		if (!config.language_services.length || isDiff(config)) return;
		try {
			const module = await import(config.lsp_module);
			const handles = (
				await Promise.all(
					config.language_services.map(async (service) => {
						dioxus.send({
							type: "language_service_status",
							server_id: service.server_id,
							server_name: service.server_name,
							status: "starting",
							message: "",
						});
						try {
							return await module.connectLanguageService({
								sessionKey: service.session_key,
								endpoint: service.endpoint,
								rootUri: service.root_uri,
								filename: config.filename,
								languageId: service.language_id,
								onStatus(status, message = "") {
									dioxus.send({
										type: "language_service_status",
										server_id: service.server_id,
										server_name: service.server_name,
										status,
										message,
									});
								},
							});
						} catch (error) {
							dioxus.send({
								type: "language_service_status",
								server_id: service.server_id,
								server_name: service.server_name,
								status: "unavailable",
								message: error instanceof Error ? error.message : String(error),
							});
							return null;
						}
					}),
				)
			).filter(Boolean);
			if (revision !== languageServiceRevision) {
				for (const handle of handles) handle.release();
				return;
			}
			languageServiceHandles = handles;
			view.dispatch({
				effects: languageServiceCompartment.reconfigure(
					handles.map((handle) => handle.extension),
				),
			});
		} catch (error) {
			if (revision === languageServiceRevision) {
				for (const service of config.language_services) {
					dioxus.send({
						type: "language_service_status",
						server_id: service.server_id,
						server_name: service.server_name,
						status: "unavailable",
						message: error instanceof Error ? error.message : String(error),
					});
				}
			}
		}
	};
	const hiddenSearchPanel = () => {
		const dom = document.createElement("div");
		dom.hidden = true;
		return { dom };
	};
	const editorSearchQuery = (config) =>
		new SearchQuery({
			search: config?.query ?? "",
			caseSensitive: config?.case_sensitive ?? false,
			wholeWord: config?.whole_word ?? false,
			regexp: config?.regex ?? false,
		});
	let editorSearchMatches = [];
	let currentSearchQuery = null;
	const emitSearch = (state) => {
		const query = getSearchQuery(state);
		if (!currentSearchQuery || !query.valid || !query.search) {
			dioxus.send({
				type: "search",
				count: 0,
				current: null,
				valid: !query.search || query.valid,
			});
			return;
		}
		const selection = state.selection.main;
		const current = editorSearchMatches.findIndex(
			(match) => match.from === selection.from && match.to === selection.to,
		);
		dioxus.send({
			type: "search",
			count: editorSearchMatches.length,
			current: current < 0 ? null : current,
			valid: true,
		});
	};
	const rebuildSearch = (state) => {
		const query = getSearchQuery(state);
		editorSearchMatches =
			currentSearchQuery && query.valid && query.search
				? Array.from(query.getCursor(state))
				: [];
		emitSearch(state);
	};
	const emitSelection = (state) => {
		const value = state.doc.toString();
		const main = state.selection.main;
		const line = state.doc.lineAt(main.head);
		dioxus.send({
			type: "selection",
			start: codeUnitToByte(value, main.from),
			end: codeUnitToByte(value, main.to),
			line: line.number,
			column: main.head - line.from + 1,
			selection_count: state.selection.ranges.length,
			ranges: state.selection.ranges.map((range) => ({
				start: codeUnitToByte(value, range.from),
				end: codeUnitToByte(value, range.to),
			})),
		});
	};

	view = new EditorView({
		parent: mount,
		state: EditorState.create({
			doc: initial.value,
			extensions: [
				theme,
				syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
				syntaxHighlighting(highlightStyle),
				highlightSpecialChars(),
				history(),
				drawSelection(),
				dropCursor(),
				rectangularSelection(),
				crosshairCursor(),
				highlightActiveLine(),
				highlightActiveLineGutter(),
				bracketMatching(),
				closeBrackets(),
				EditorState.languageData.of(() => [{ autocomplete: completeAnyWord }]),
				searchRanges,
				search({ createPanel: hiddenSearchPanel }),
				EditorState.allowMultipleSelections.of(true),
				keymap.of([
					{ key: "Mod-d", run: selectNextOccurrence },
					{ key: "Mod-Shift-l", run: selectSelectionMatches },
					{ key: "Alt-Shift-ArrowUp", run: addCursorAbove },
					{ key: "Alt-Shift-ArrowDown", run: addCursorBelow },
					indentWithTab,
					...closeBracketsKeymap,
					...defaultKeymap,
					...historyKeymap,
				]),
				languageCompartment.of([]),
				languageServiceCompartment.of([]),
				autocompleteCompartment.of(
					initial.autocomplete && !isDiff(initial) ? autocompletion() : [],
				),
				diffCompartment.of(diffExtension(initial)),
				lineNumbersCompartment.of(initial.line_numbers ? lineNumbers() : []),
				wrappingCompartment.of(
					initial.word_wrap ? EditorView.lineWrapping : [],
				),
				tabSizeCompartment.of(
					EditorState.tabSize.of(Math.max(1, initial.tab_width)),
				),
				indentCompartment.of([
					indentUnit.of(indentText(initial)),
					indentOnInput(),
				]),
				readOnlyCompartment.of(
					EditorState.readOnly.of(initial.read_only || isDiff(initial)),
				),
				attributesCompartment.of(
					EditorView.contentAttributes.of(editorAttributes(initial)),
				),
				placeholderCompartment.of(
					initial.placeholder ? placeholder(initial.placeholder) : [],
				),
				EditorView.updateListener.of((update) => {
					if (update.docChanged && !suppressInput) {
						const edits = [];
						update.changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
							edits.push({ start: fromA, end: toA, text: inserted.toString() });
						});
						dioxus.send({ type: "input", edits });
					}
					if (update.docChanged || update.selectionSet)
						emitSelection(update.state);
					if (update.docChanged) {
						rebuildSearch(update.state);
					} else if (update.selectionSet) {
						emitSearch(update.state);
					}
				}),
			],
		}),
	});

	const scrollPositions = new Map();
	const configure = (config) => {
		const filenameChanged = config.filename !== currentConfig.filename;
		const languageChanged =
			config.language !== currentConfig.language || filenameChanged;
		const restoredScroll = filenameChanged
			? (scrollPositions.get(config.filename) ?? { top: 0, left: 0 })
			: null;
		if (filenameChanged) {
			scrollPositions.set(currentConfig.filename, {
				top: view.scrollDOM.scrollTop,
				left: view.scrollDOM.scrollLeft,
			});
		}
		const languageServiceChanged =
			JSON.stringify(config.language_services) !==
				JSON.stringify(currentConfig.language_services) || languageChanged;
		const diffChanged =
			config.diff_original !== currentConfig.diff_original ||
			config.line_numbers !== currentConfig.line_numbers;
		const effects = [
			lineNumbersCompartment.reconfigure(
				config.line_numbers ? lineNumbers() : [],
			),
			wrappingCompartment.reconfigure(
				config.word_wrap ? EditorView.lineWrapping : [],
			),
			autocompleteCompartment.reconfigure(
				config.autocomplete && !isDiff(config) ? autocompletion() : [],
			),
			tabSizeCompartment.reconfigure(
				EditorState.tabSize.of(Math.max(1, config.tab_width)),
			),
			indentCompartment.reconfigure([
				indentUnit.of(indentText(config)),
				indentOnInput(),
			]),
			readOnlyCompartment.reconfigure(
				EditorState.readOnly.of(config.read_only || isDiff(config)),
			),
			attributesCompartment.reconfigure(
				EditorView.contentAttributes.of(editorAttributes(config)),
			),
			placeholderCompartment.reconfigure(
				config.placeholder ? placeholder(config.placeholder) : [],
			),
			setSearchRanges.of(
				searchDecorations(
					config.value,
					config.search_matches,
					config.active_search_match,
				),
			),
		];
		if (diffChanged) {
			effects.push(view.scrollSnapshot());
			effects.push(diffCompartment.reconfigure(diffExtension(config)));
		}
		const currentValue = view.state.doc.toString();
		suppressInput = true;
		view.dispatch({
			changes:
				currentValue === config.value
					? undefined
					: { from: 0, to: currentValue.length, insert: config.value },
			effects,
		});
		suppressInput = false;
		currentConfig = config;
		if (restoredScroll) {
			view.requestMeasure({
				read: () => restoredScroll,
				write: (position) => {
					if (currentConfig.filename !== config.filename) return;
					view.scrollDOM.scrollTop = position.top;
					view.scrollDOM.scrollLeft = position.left;
				},
			});
		}
		if (languageChanged) void loadLanguage(config);
		if (languageServiceChanged) void configureLanguageService(config);
	};

	const configureSearch = (query) => {
		currentSearchQuery = query;
		view.dispatch({ effects: setSearchQuery.of(editorSearchQuery(query)) });
		rebuildSearch(view.state);
		if (query?.query) {
			openSearchPanel(view);
		} else {
			closeSearchPanel(view);
		}
	};

	configure(initial);
	configureSearch(null);
	void loadLanguage(initial);
	void configureLanguageService(initial);
	emitSelection(view.state);

	while (true) {
		const command = await dioxus.recv();
		if (!command || command.type === "destroy") break;
		if (command.type === "configure") {
			configure(command.config);
		} else if (command.type === "configure_search") {
			configureSearch(command.query);
		} else if (command.type === "focus") {
			view.focus();
		} else if (command.type === "go_to_line") {
			const line = view.state.doc.line(
				Math.min(Math.max(1, command.line), view.state.doc.lines),
			);
			view.dispatch({
				selection: { anchor: line.from },
				scrollIntoView: true,
			});
			view.focus();
		} else if (command.type === "select") {
			const value = view.state.doc.toString();
			const anchor = byteToCodeUnit(value, command.start);
			const head = byteToCodeUnit(value, command.end);
			view.dispatch({
				selection: { anchor, head },
				scrollIntoView: true,
			});
			view.focus();
		} else if (command.type === "replace") {
			const anchor = byteToCodeUnit(command.value, command.start);
			const head = byteToCodeUnit(command.value, command.end);
			suppressInput = true;
			view.dispatch({
				changes: {
					from: 0,
					to: view.state.doc.length,
					insert: command.value,
				},
				selection: EditorSelection.single(anchor, head),
				scrollIntoView: true,
			});
			suppressInput = false;
			dioxus.send({ type: "input", value: command.value });
			emitSelection(view.state);
			view.focus();
		} else if (command.type === "search_next") {
			findNext(view);
			view.focus();
		} else if (command.type === "search_previous") {
			findPrevious(view);
			view.focus();
		}
	}

	releaseLanguageService();
	view.destroy();
})();
