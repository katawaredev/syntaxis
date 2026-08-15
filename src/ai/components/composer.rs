use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use dioxus::prelude::*;
use serde::Deserialize;
use syntaxis_agent::{
    ExtensionWidget, ImageAttachment, MAX_IMAGE_BYTES, MAX_PROMPT_IMAGES, MAX_TOTAL_IMAGE_BYTES,
    PiCommand, PromptDelivery,
};
use syntaxis_ui::prelude::{AppIcon, Icon, IconButton};
use syntaxis_workspace::{EntryKind, WorkspaceRecord};

use crate::files::{SearchScope, WorkspaceSearchOptions, search_workspace_files};

#[derive(Clone)]
pub(crate) struct ComposerSubmission {
    pub(crate) text: String,
    pub(crate) images: Vec<ImageAttachment>,
    pub(crate) delivery: PromptDelivery,
}

#[component]
pub(crate) fn AgentComposer(
    mut draft: Signal<String>,
    mut attachments: Signal<Vec<ImageAttachment>>,
    mut composer_error: Signal<Option<String>>,
    connected: bool,
    working: bool,
    pending_messages: usize,
    steering_queue: Vec<String>,
    follow_up_queue: Vec<String>,
    extension_statuses: Vec<(String, String)>,
    extension_widgets: Vec<ExtensionWidget>,
    draft_key: String,
    commands: Vec<PiCommand>,
    accepts_images: bool,
    editing_message: bool,
    workspace: WorkspaceRecord,
    active_file: Option<String>,
    active_reference: Option<String>,
    on_send: EventHandler<ComposerSubmission>,
    on_abort: EventHandler<()>,
    on_cancel_edit: EventHandler<()>,
) -> Element {
    let speech_active = use_speech_bridge(draft, composer_error);
    let mut touch_input = use_signal(|| false);
    let mut draft_dirty = use_persisted_draft(draft, &draft_key);
    use_paste_bridge(attachments, composer_error);
    let images = attachments();
    let can_send = connected
        && (!draft().trim().is_empty() || !images.is_empty())
        && (images.is_empty() || accepts_images);
    let matched_commands = matching_commands(&commands, &draft());
    let mut command_index = use_signal(|| 0_usize);
    let command_key = draft().strip_prefix('/').map(str::to_owned);
    use_effect(use_reactive((&command_key,), move |_| command_index.set(0)));
    let selected_command = matched_commands
        .get(command_index().min(matched_commands.len().saturating_sub(1)))
        .cloned();
    let mention = mention_query(&draft());
    let bootstrap_workspace = workspace.clone();
    let mention_bootstrap = use_resource(move || {
        let workspace = bootstrap_workspace.clone();
        async move { crate::workspace::client::workspace_files_bootstrap(workspace).await }
    });
    let mention_workspace = workspace.clone();
    let mention_results = use_resource(move || {
        let workspace = mention_workspace.clone();
        let query = mention_query(&draft()).map(|mention| mention.query);
        let bootstrap = mention_bootstrap().and_then(Result::ok);
        let root_candidates = bootstrap
            .as_ref()
            .map(|bootstrap| {
                bootstrap
                    .entries
                    .iter()
                    .filter(|entry| entry.kind != EntryKind::Symlink)
                    .filter(|entry| {
                        entry.kind == EntryKind::File
                            || !entry.path.as_str().chars().any(char::is_whitespace)
                    })
                    .take(12)
                    .map(|entry| {
                        let mut path = entry.path.as_str().to_owned();
                        if entry.kind == EntryKind::Directory {
                            path.push('/');
                        }
                        path
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let ignored_paths = bootstrap.map(|bootstrap| bootstrap.ignored_paths);
        async move {
            let Some(query) = query else {
                return Vec::new();
            };
            if query.is_empty() {
                return root_candidates;
            }
            let Some(ignored_paths) = ignored_paths else {
                return Vec::new();
            };
            dioxus_sdk_time::sleep(std::time::Duration::from_millis(120)).await;
            search_workspace_files(
                workspace,
                query,
                WorkspaceSearchOptions {
                    fuzzy: true,
                    case_sensitive: false,
                    scope: SearchScope::FileNames,
                },
                ignored_paths.into_iter().collect(),
                false,
            )
            .await
            .map(|results| {
                results
                    .items
                    .into_iter()
                    .take(12)
                    .map(|item| item.entry.path.as_str().to_owned())
                    .collect()
            })
            .unwrap_or_default()
        }
    });
    let mentioned_files = mention_results().unwrap_or_default();
    let mention_status = mention.as_ref().and_then(|mention| {
        if mention_bootstrap().is_none() || mention_results().is_none() {
            Some("Searching project files…".to_owned())
        } else if mention_bootstrap().is_some_and(|result| result.is_err()) {
            Some("Project file search is unavailable.".to_owned())
        } else if !mention.query.is_empty() && mentioned_files.is_empty() {
            Some("No project files match.".to_owned())
        } else if mentioned_files.is_empty() {
            Some("No project files yet.".to_owned())
        } else {
            None
        }
    });
    let first_mentioned_file = mentioned_files.first().cloned();
    let mut mention_index = use_signal(|| 0_usize);
    let mention_key = mention.as_ref().map(|mention| mention.query.clone());
    use_effect(use_reactive((&mention_key,), move |_| mention_index.set(0)));
    let selected_mentioned_file = mentioned_files
        .get(mention_index().min(mentioned_files.len().saturating_sub(1)))
        .cloned()
        .or(first_mentioned_file);
    let keyboard_commands = commands.clone();
    let button_commands = commands.clone();
    let follow_up_commands = commands.clone();
    let keyboard_draft_key = draft_key.clone();
    let button_draft_key = draft_key.clone();
    let follow_up_draft_key = draft_key.clone();
    rsx! {
        footer { class: "bg-card px-2.5 pt-1 pb-[max(0.65rem,env(safe-area-inset-bottom))]",
            div { class: "relative mx-auto max-w-3xl",
                SlashCommandMenu {
                    commands: matched_commands.clone(),
                    draft,
                    selected: command_index(),
                }
                FileMentionMenu {
                    paths: mentioned_files.clone(),
                    draft,
                    mention: mention.clone(),
                    selected: mention_index(),
                    status: mention_status,
                }
                ExtensionWidgets {
                    widgets: extension_widgets.clone(),
                    placement: "aboveEditor",
                }
                div { class: "overflow-hidden rounded-2xl border border-input bg-card shadow-[0_8px_30px_#0002] transition-[border,box-shadow] focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/20",
                    if editing_message {
                        div { class: "flex items-center justify-between gap-3 border-b border-border bg-secondary/45 px-3 py-2 text-[11px]",
                            span { class: "min-w-0 text-muted-foreground",
                                strong { class: "font-medium text-foreground", "Editing message" }
                                " · Sending will branch the conversation from here."
                            }
                            button {
                                class: "shrink-0 rounded-md px-2 py-1 font-medium text-foreground transition-colors hover:bg-accent",
                                r#type: "button",
                                onclick: move |_| on_cancel_edit.call(()),
                                "Cancel"
                            }
                        }
                    }
                    if !images.is_empty() {
                        ComposerAttachments {
                            images: images.clone(),
                            on_remove: move |index| {
                                attachments.write().remove(index);
                                composer_error.set(None);
                            },
                        }
                    }
                    QueuePreview { steering: steering_queue, follow_up: follow_up_queue }
                    div { class: "ai-composer-editor",
                        textarea {
                            id: "syntaxis-ai-composer",
                            class: "ai-composer-input",
                            rows: 3,
                            value: draft(),
                            disabled: !connected,
                            placeholder: if working { "Steer the agent while it works…" } else { "Ask agent to change or inspect this project…" },
                            aria_label: "Message agent",
                            "data-images-enabled": accepts_images && connected,
                            ontouchstart: move |_| touch_input.set(true),
                            oninput: move |event| {
                                draft_dirty.set(true);
                                draft.set(event.value());
                                composer_error.set(None);
                            },
                            onkeydown: move |event: KeyboardEvent| {
                                if editing_message && event.key() == Key::Escape {
                                    event.prevent_default();
                                    on_cancel_edit.call(());
                                } else if mention.is_some() && !mentioned_files.is_empty()
                                    && matches!(event.key(), Key::ArrowDown | Key::ArrowUp)
                                {
                                    event.prevent_default();
                                    let length = mentioned_files.len();
                                    if event.key() == Key::ArrowDown {
                                        mention_index.set((mention_index() + 1) % length);
                                    } else {
                                        mention_index.set((mention_index() + length - 1) % length);
                                    }
                                } else if mention.is_some() && event.key() == Key::Escape {
                                    event.prevent_default();
                                    draft
                                        .set(
                                            format!(
                                                "{}@",
                                                &draft()[..mention.as_ref().map_or(0, |value| value.start)],
                                            ),
                                        );
                                } else if mention.is_some() && event.key() == Key::Tab {
                                    event.prevent_default();
                                    if let (Some(mention), Some(path)) = (
                                        mention.as_ref(),
                                        selected_mentioned_file.as_ref(),
                                    ) {
                                        insert_file_mention(draft, mention, path);
                                    }
                                } else if command_key.is_some() && !matched_commands.is_empty()
                                    && matches!(event.key(), Key::ArrowDown | Key::ArrowUp)
                                {
                                    event.prevent_default();
                                    let length = matched_commands.len();
                                    if event.key() == Key::ArrowDown {
                                        command_index.set((command_index() + 1) % length);
                                    } else {
                                        command_index.set((command_index() + length - 1) % length);
                                    }
                                } else if command_key.is_some()
                                    && (event.key() == Key::Tab || (event.key() == Key::Enter && !touch_input()))
                                    && selected_command.is_some()
                                {
                                    event.prevent_default();
                                    if let Some(command) = selected_command.as_ref() {
                                        draft.set(format!("/{} ", command.name));
                                    }
                                } else if command_key.is_some() && !matched_commands.is_empty()
                                    && event.key() == Key::Escape
                                {
                                    event.prevent_default();
                                    draft.set(String::new());
                                } else if event.key() == Key::Enter
                                    && !event.modifiers().contains(Modifiers::SHIFT) && !touch_input()
                                {
                                    event.prevent_default();
                                    if let (Some(mention), Some(path)) = (
                                        mention.as_ref(),
                                        selected_mentioned_file.as_ref(),
                                    ) {
                                        insert_file_mention(draft, mention, path);
                                    } else {
                                        submit_composer(
                                            can_send,
                                            draft,
                                            attachments,
                                            composer_error,
                                            &keyboard_commands,
                                            &keyboard_draft_key,
                                            if working { PromptDelivery::Steer } else { PromptDelivery::Prompt },
                                            on_send,
                                        );
                                    }
                                }
                            },
                        }
                    }
                    div { class: "flex min-h-10 items-center gap-1 px-2 pb-2",
                        label {
                            class: if accepts_images && connected { "grid size-8 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" } else { "grid size-8 cursor-not-allowed place-items-center rounded-lg text-muted-foreground opacity-35" },
                            aria_label: if accepts_images { "Attach images" } else { "Selected model does not accept images" },
                            title: if accepts_images { "Attach images" } else { "Selected model does not accept images" },
                            input {
                                class: "hidden",
                                r#type: "file",
                                accept: "image/*",
                                multiple: true,
                                disabled: !accepts_images || !connected,
                                onchange: move |event: FormEvent| {
                                    spawn(load_images(event.files(), attachments, composer_error));
                                },
                            }
                            Icon { icon: AppIcon::Attachment, size: 15 }
                        }
                        if let Some(active_file) = active_file.clone() {
                            button {
                                class: "grid size-8 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-accent hover:text-foreground",
                                r#type: "button",
                                aria_label: "Reference active file",
                                title: "Reference {active_file}",
                                onclick: move |_| append_file_reference(draft, &active_file),
                                Icon { icon: AppIcon::Code, size: 15 }
                            }
                        }
                        if let Some(active_reference) = active_reference
                            .clone()
                            .filter(|reference| {
                                active_file.as_ref().is_none_or(|path| reference != path)
                            })
                        {
                            button {
                                class: "grid size-8 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-accent hover:text-foreground",
                                r#type: "button",
                                aria_label: "Reference editor location or selection",
                                title: "Reference {active_reference}",
                                onclick: move |_| append_text_reference(draft, &active_reference),
                                Icon { icon: AppIcon::LineNumbers, size: 15 }
                            }
                        }
                        IconButton {
                            label: if speech_active() { "Stop dictation" } else { "Dictate message" },
                            icon: AppIcon::Microphone,
                            pressed: speech_active(),
                            disabled: !connected,
                            onclick: move |_| toggle_speech(),
                        }
                        span { class: "min-w-0 flex-1 truncate px-1 text-[9px] text-muted-foreground",
                            span { class: "max-[520px]:hidden",
                                if working {
                                    if pending_messages > 0 {
                                        "Steer queued · {pending_messages} pending"
                                    } else {
                                        "Enter steers · Shift+Enter adds a line"
                                    }
                                } else {
                                    "Markdown supported · Enter sends · Shift+Enter adds a line"
                                }
                            }
                        }
                        div { class: "ml-auto flex shrink-0 items-center gap-1",
                            if working {
                                IconButton {
                                    label: "Stop agent",
                                    icon: AppIcon::Stop,
                                    danger: true,
                                    onclick: move |_| on_abort.call(()),
                                }
                            }
                            button {
                                class: "grid size-8.5 place-items-center rounded-lg bg-primary text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-35",
                                disabled: !can_send,
                                aria_label: if working { "Steer agent" } else { "Send message" },
                                title: if working { "Steer agent" } else { "Send message" },
                                onclick: move |_| {
                                    submit_composer(
                                        can_send,
                                        draft,
                                        attachments,
                                        composer_error,
                                        &button_commands,
                                        &button_draft_key,
                                        if working { PromptDelivery::Steer } else { PromptDelivery::Prompt },
                                        on_send,
                                    );
                                },
                                Icon { icon: AppIcon::Send, size: 15 }
                            }
                            if working {
                                button {
                                    class: "grid size-8.5 place-items-center rounded-lg border border-input bg-background text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-35",
                                    disabled: !can_send,
                                    aria_label: "Send after agent finishes",
                                    title: "Follow up after the current task finishes",
                                    onclick: move |_| {
                                        submit_composer(
                                            can_send,
                                            draft,
                                            attachments,
                                            composer_error,
                                            &follow_up_commands,
                                            &follow_up_draft_key,
                                            PromptDelivery::FollowUp,
                                            on_send,
                                        );
                                    },
                                    Icon { icon: AppIcon::Next, size: 15 }
                                }
                            }
                        }
                    }
                }
                ExtensionWidgets { widgets: extension_widgets, placement: "belowEditor" }
                if !extension_statuses.is_empty() {
                    div {
                        class: "flex flex-wrap gap-x-3 gap-y-1 px-2.5 pt-1.5 text-[9px] text-muted-foreground",
                        role: "status",
                        for (key, text) in extension_statuses {
                            span { key: "{key}", title: "{key}", "{text}" }
                        }
                    }
                }
                if !images.is_empty() && !accepts_images {
                    p { class: "px-2.5 pt-1.5 text-[10px] text-warning",
                        "Choose a vision-capable model to send these images."
                    }
                }
            }
        }
    }
}

#[component]
fn ExtensionWidgets(widgets: Vec<ExtensionWidget>, placement: String) -> Element {
    rsx! {
        for widget in widgets.into_iter().filter(|widget| widget.placement == placement) {
            div {
                key: "{widget.key}",
                class: "mb-1 max-h-36 overflow-auto rounded-lg border border-border bg-secondary/35 px-3 py-2 font-mono text-[10px] leading-relaxed text-muted-foreground",
                for line in widget.lines {
                    div { "{line}" }
                }
            }
        }
    }
}

#[component]
fn QueuePreview(steering: Vec<String>, follow_up: Vec<String>) -> Element {
    if steering.is_empty() && follow_up.is_empty() {
        return rsx! {};
    }
    rsx! {
        div {
            class: "grid max-h-28 gap-1 overflow-y-auto border-b border-border/70 bg-secondary/25 px-3 py-2 text-[10px]",
            role: "status",
            "aria-live": "polite",
            for message in steering {
                div { class: "flex min-w-0 items-center gap-2",
                    span { class: "shrink-0 rounded bg-primary/10 px-1.5 py-0.5 font-medium text-primary",
                        "Next turn"
                    }
                    span { class: "truncate text-muted-foreground", "{message}" }
                }
            }
            for message in follow_up {
                div { class: "flex min-w-0 items-center gap-2",
                    span { class: "shrink-0 rounded bg-secondary px-1.5 py-0.5 font-medium text-foreground",
                        "After task"
                    }
                    span { class: "truncate text-muted-foreground", "{message}" }
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct FileMention {
    start: usize,
    query: String,
}

fn mention_query(text: &str) -> Option<FileMention> {
    let start = text
        .rfind(|character: char| character.is_whitespace())
        .map_or(0, |index| index + 1);
    let token = &text[start..];
    token.strip_prefix('@').map(|query| FileMention {
        start,
        query: query.to_owned(),
    })
}

fn insert_file_mention(mut draft: Signal<String>, mention: &FileMention, path: &str) {
    let current = draft();
    let mut updated = current[..mention.start].to_owned();
    updated.push('@');
    if path.chars().any(char::is_whitespace) {
        updated.push('"');
        updated.push_str(path);
        updated.push('"');
    } else {
        updated.push_str(path);
    }
    if !path.ends_with('/') {
        updated.push(' ');
    }
    draft.set(updated);
    crate::ai::agent_view::focus_ai_composer();
}

fn append_file_reference(mut draft: Signal<String>, path: &str) {
    let mut value = draft();
    if !value.is_empty() && !value.ends_with(char::is_whitespace) {
        value.push(' ');
    }
    value.push('@');
    if path.chars().any(char::is_whitespace) {
        value.push('"');
        value.push_str(path);
        value.push('"');
    } else {
        value.push_str(path);
    }
    value.push(' ');
    draft.set(value);
    crate::ai::agent_view::focus_ai_composer();
}

fn append_text_reference(mut draft: Signal<String>, reference: &str) {
    let mut value = draft();
    if !value.is_empty() && !value.ends_with(char::is_whitespace) {
        value.push(' ');
    }
    value.push_str(reference);
    value.push(' ');
    draft.set(value);
    crate::ai::agent_view::focus_ai_composer();
}

#[component]
fn FileMentionMenu(
    paths: Vec<String>,
    mut draft: Signal<String>,
    mention: Option<FileMention>,
    selected: usize,
    status: Option<String>,
) -> Element {
    rsx! {
        if let Some(mention) = mention {
            if !paths.is_empty() || status.is_some() {
                div { class: "absolute right-0 bottom-[calc(100%+7px)] left-0 z-60 overflow-hidden rounded-xl border border-border bg-popover shadow-2xl",
                    div { class: "flex items-center gap-2 border-b border-border px-3 py-2 text-[10px] text-muted-foreground",
                        Icon { icon: AppIcon::Code, size: 13 }
                        "Project files"
                        span { class: "ml-auto max-[520px]:hidden", "Enter to reference" }
                        span { class: "ml-auto hidden max-[520px]:inline", "Tap to reference" }
                    }
                    div { class: "max-h-[min(16rem,35dvh)] overflow-y-auto p-1.5",
                        if let Some(status) = status {
                            p {
                                class: "px-2.5 py-4 text-center text-[10px] text-muted-foreground",
                                role: "status",
                                "{status}"
                            }
                        }
                        for (index, path) in paths.into_iter().enumerate() {
                            button {
                                key: "{path}",
                                class: if index == selected { "flex min-h-9 w-full items-center gap-2 rounded-lg bg-accent px-2.5 py-2 text-left max-[520px]:min-h-11" } else { "flex min-h-9 w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left hover:bg-accent max-[520px]:min-h-11" },
                                onclick: {
                                    let mention = mention.clone();
                                    let path = path.clone();
                                    move |_| insert_file_mention(draft, &mention, &path)
                                },
                                Icon { icon: AppIcon::Code, size: 13 }
                                span { class: "truncate font-mono text-[10px]", "{path}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "composer submission intentionally receives its independent reactive inputs explicitly"
)]
fn submit_composer(
    can_send: bool,
    draft: Signal<String>,
    mut attachments: Signal<Vec<ImageAttachment>>,
    mut composer_error: Signal<Option<String>>,
    commands: &[PiCommand],
    draft_key: &str,
    delivery: PromptDelivery,
    on_send: EventHandler<ComposerSubmission>,
) {
    if !can_send {
        return;
    }
    if let Some(command) = exact_command(commands, &draft())
        .filter(|command| command.invocation.as_deref() == Some("interactive"))
    {
        composer_error.set(Some(format!(
            "/{} requires the agent's terminal interface and cannot run here.",
            command.name
        )));
        return;
    }
    on_send.call(ComposerSubmission {
        text: draft(),
        images: attachments(),
        delivery,
    });
    clear_saved_draft(draft_key);
    attachments.set(Vec::new());
}

fn clear_saved_draft(draft_key: &str) {
    let draft_key = draft_key.to_owned();
    spawn(async move {
        let _ = crate::storage::remove(draft_key).await;
    });
}

fn use_persisted_draft(draft: Signal<String>, draft_key: &str) -> Signal<bool> {
    let draft_key = draft_key.to_owned();
    let mut requested_key = use_signal(String::new);
    let mut loaded_key = use_signal(|| None::<String>);
    let mut dirty = use_signal(|| false);
    let mut save_revision = use_signal(|| 0_u64);

    use_effect(use_reactive((&draft_key,), move |(key,)| {
        requested_key.set(key.clone());
        loaded_key.set(None);
        dirty.set(false);
        let mut draft = draft;
        draft.set(String::new());
        spawn(async move {
            let stored = crate::storage::get(key.clone())
                .await
                .ok()
                .flatten()
                .unwrap_or_default();
            if requested_key.peek().as_str() != key {
                return;
            }
            if !*dirty.peek() {
                draft.set(stored);
            }
            loaded_key.set(Some(key));
        });
    }));

    use_effect(move || {
        let value = draft();
        let Some(key) = loaded_key() else {
            return;
        };
        *save_revision.write() += 1;
        let revision = save_revision();
        spawn(async move {
            dioxus_sdk_time::sleep(std::time::Duration::from_millis(150)).await;
            if save_revision() != revision {
                return;
            }
            if value.is_empty() {
                let _ = crate::storage::remove(key).await;
            } else {
                let _ = crate::storage::set(key, value).await;
            }
        });
    });

    dirty
}

#[component]
fn SlashCommandMenu(commands: Vec<PiCommand>, draft: Signal<String>, selected: usize) -> Element {
    rsx! {
        if !commands.is_empty() {
            div { class: "absolute right-0 bottom-[calc(100%+7px)] left-0 z-60 overflow-hidden rounded-xl border border-border bg-popover shadow-2xl",
                div { class: "flex items-center gap-2 border-b border-border px-3 py-2 text-[10px] text-muted-foreground",
                    Icon { icon: AppIcon::Command, size: 13 }
                    "Agent commands"
                    span { class: "ml-auto max-[520px]:hidden", "Enter to insert" }
                    span { class: "ml-auto hidden max-[520px]:inline", "Tap to insert" }
                }
                div { class: "max-h-[min(16rem,35dvh)] overflow-y-auto p-1.5",
                    for (index, command) in commands.into_iter().enumerate() {
                        SlashCommandRow {
                            key: "{command.name}",
                            command,
                            draft,
                            active: index == selected,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SlashCommandRow(command: PiCommand, mut draft: Signal<String>, active: bool) -> Element {
    let insertion = format!("/{} ", command.name);
    rsx! {
        button {
            class: if active { "flex min-h-10 w-full items-center gap-3 rounded-lg bg-accent px-2.5 py-2 text-left max-[520px]:min-h-11" } else { "flex min-h-10 w-full items-center gap-3 rounded-lg px-2.5 py-2 text-left hover:bg-accent max-[520px]:min-h-11" },
            onclick: move |_| {
                draft.set(insertion.clone());
                crate::ai::agent_view::focus_ai_composer();
            },
            span { class: "grid size-6 shrink-0 place-items-center rounded-md bg-secondary font-mono text-[10px] text-primary",
                "/"
            }
            span { class: "min-w-0 flex-1",
                strong { class: "block truncate font-mono text-[11px]", "/{command.name}" }
                if let Some(argument_hint) = command.argument_hint.as_ref() {
                    small { class: "ml-1 font-mono text-[9px] text-muted-foreground",
                        "{argument_hint}"
                    }
                }
                if !command.description.is_empty() {
                    small { class: "block truncate text-[9px] text-muted-foreground",
                        "{command.description}"
                    }
                }
            }
            span { class: "shrink-0 rounded bg-secondary px-1.5 py-0.5 text-[8px] text-muted-foreground",
                if command.invocation.as_deref() == Some("interactive") {
                    "terminal"
                } else {
                    "{command.source}"
                }
            }
        }
    }
}

#[component]
fn ComposerAttachments(images: Vec<ImageAttachment>, on_remove: EventHandler<usize>) -> Element {
    rsx! {
        div { class: "flex gap-2 overflow-x-auto border-b border-border/70 px-3 pt-3 pb-2",
            for (index, image) in images.iter().enumerate() {
                AttachmentPreview {
                    key: "{index}-{image.name}",
                    image: image.clone(),
                    on_remove: move |()| on_remove.call(index),
                }
            }
        }
    }
}

#[component]
fn AttachmentPreview(image: ImageAttachment, on_remove: EventHandler<()>) -> Element {
    rsx! {
        div { class: "group relative size-18 shrink-0 overflow-hidden rounded-xl border border-border bg-background",
            img {
                class: "size-full object-cover",
                src: image.data_url(),
                alt: image.name.clone(),
            }
            button {
                class: "touch-only-visible absolute top-1 right-1 grid size-7 place-items-center rounded-full bg-background/90 text-foreground opacity-0 shadow transition-opacity group-hover:opacity-100 focus-visible:opacity-100",
                aria_label: "Remove {image.name}",
                title: "Remove image",
                onclick: move |_| on_remove.call(()),
                Icon { icon: AppIcon::Close, size: 11 }
            }
            span { class: "absolute right-0 bottom-0 left-0 truncate bg-black/60 px-1.5 py-1 text-[8px] text-white",
                "{image.name}"
            }
        }
    }
}

pub(crate) async fn load_images(
    files: Vec<dioxus::html::FileData>,
    mut attachments: Signal<Vec<ImageAttachment>>,
    mut error: Signal<Option<String>>,
) {
    for file in files {
        if attachments().len() >= MAX_PROMPT_IMAGES {
            error.set(Some(format!("Attach up to {MAX_PROMPT_IMAGES} images.")));
            break;
        }
        let mime_type = file.content_type().unwrap_or_default();
        if !mime_type.starts_with("image/") {
            error.set(Some(format!("{} is not an image.", file.name())));
            continue;
        }
        let total = attachments().iter().map(|image| image.size).sum::<u64>();
        if file.size() > MAX_IMAGE_BYTES
            || total.saturating_add(file.size()) > MAX_TOTAL_IMAGE_BYTES
        {
            error.set(Some("Images can be 8 MiB each and 16 MiB total.".into()));
            continue;
        }
        match file.read_bytes().await {
            Ok(bytes) => attachments.write().push(ImageAttachment {
                name: file.name(),
                mime_type,
                size: file.size(),
                data: BASE64.encode(bytes),
            }),
            Err(_) => error.set(Some(format!("Could not read {}.", file.name()))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FileMention, mention_query};

    #[test]
    fn mention_query_uses_the_final_token() {
        assert_eq!(
            mention_query("@"),
            Some(FileMention {
                start: 0,
                query: String::new(),
            })
        );
        assert_eq!(
            mention_query("Review @src/com"),
            Some(FileMention {
                start: 7,
                query: "src/com".into(),
            })
        );
    }

    #[test]
    fn mention_query_ignores_completed_references() {
        assert_eq!(mention_query("Review @src/main.rs please"), None);
        assert_eq!(mention_query("plain text"), None);
    }
}

#[derive(Deserialize)]
struct PasteBridgeEvent {
    kind: String,
    name: Option<String>,
    mime_type: Option<String>,
    data: Option<String>,
    message: Option<String>,
}

fn use_paste_bridge(attachments: Signal<Vec<ImageAttachment>>, error: Signal<Option<String>>) {
    let mut bridge = use_signal(|| None::<dioxus::document::Eval>);
    use_effect(move || {
        let mut events = document::eval(
            r#"
            const id = await dioxus.recv();
            const listener = event => {
                if (event.detail?.id === id) dioxus.send(event.detail);
            };
            window.addEventListener("syntaxis-ai-paste", listener);
            await dioxus.recv();
            window.removeEventListener("syntaxis-ai-paste", listener);
            "#,
        );
        let _ = events.send("syntaxis-ai-composer");
        bridge.set(Some(events));
        spawn(async move {
            while let Ok(event) = events.recv::<PasteBridgeEvent>().await {
                apply_paste_event(event, attachments, error);
            }
        });
    });
    use_drop(move || {
        if let Some(events) = bridge() {
            let _ = events.send(true);
        }
    });
}

fn apply_paste_event(
    event: PasteBridgeEvent,
    mut attachments: Signal<Vec<ImageAttachment>>,
    mut error: Signal<Option<String>>,
) {
    if event.kind == "error" {
        error.set(event.message);
        return;
    }
    let Some(data) = event.data else {
        return;
    };
    let mime_type = event.mime_type.unwrap_or_default();
    if !mime_type.starts_with("image/") {
        return;
    }
    if attachments().len() >= MAX_PROMPT_IMAGES {
        error.set(Some(format!("Attach up to {MAX_PROMPT_IMAGES} images.")));
        return;
    }
    let max_encoded_size = usize::try_from(MAX_IMAGE_BYTES)
        .unwrap_or(usize::MAX)
        .saturating_mul(4)
        / 3
        + 4;
    if data.len() > max_encoded_size {
        error.set(Some("Images can be 8 MiB each and 16 MiB total.".into()));
        return;
    }
    let Ok(bytes) = BASE64.decode(&data) else {
        error.set(Some("Could not read the pasted image.".into()));
        return;
    };
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let total = attachments().iter().map(|image| image.size).sum::<u64>();
    if size > MAX_IMAGE_BYTES || total.saturating_add(size) > MAX_TOTAL_IMAGE_BYTES {
        error.set(Some("Images can be 8 MiB each and 16 MiB total.".into()));
        return;
    }
    attachments.write().push(ImageAttachment {
        name: event.name.unwrap_or_else(|| "Pasted image".into()),
        mime_type,
        size,
        data,
    });
    error.set(None);
}

#[derive(Deserialize)]
struct SpeechBridgeEvent {
    kind: String,
    text: Option<String>,
    message: Option<String>,
}

fn use_speech_bridge(draft: Signal<String>, error: Signal<Option<String>>) -> Signal<bool> {
    let active = use_signal(|| false);
    let mut bridge = use_signal(|| None::<dioxus::document::Eval>);
    use_effect(move || {
        let mut events = document::eval(
            r#"
            const id = await dioxus.recv();
            const listener = event => {
                if (event.detail?.id === id) dioxus.send(event.detail);
            };
            window.addEventListener("syntaxis-ai-speech", listener);
            await dioxus.recv();
            window.removeEventListener("syntaxis-ai-speech", listener);
            "#,
        );
        let _ = events.send("syntaxis-ai-composer");
        bridge.set(Some(events));
        spawn(async move {
            while let Ok(event) = events.recv::<SpeechBridgeEvent>().await {
                apply_speech_event(event, draft, active, error);
            }
        });
    });
    use_drop(move || {
        if let Some(events) = bridge() {
            let _ = events.send(true);
        }
    });
    active
}

fn apply_speech_event(
    event: SpeechBridgeEvent,
    mut draft: Signal<String>,
    mut active: Signal<bool>,
    mut error: Signal<Option<String>>,
) {
    match event.kind.as_str() {
        "start" => {
            active.set(true);
            error.set(None);
        }
        "end" => active.set(false),
        "transcript" => {
            if let Some(text) = event.text {
                let mut value = draft.write();
                if !value.is_empty() && !value.ends_with(char::is_whitespace) {
                    value.push(' ');
                }
                value.push_str(text.trim());
            }
        }
        "error" => {
            active.set(false);
            error.set(Some(event.message.unwrap_or_else(|| {
                "Speech recognition is unavailable in this browser.".into()
            })));
        }
        _ => {}
    }
}

fn toggle_speech() {
    let _ = document::eval(
        r#"
        window.SyntaxisAiChat?.toggleSpeech("syntaxis-ai-composer");
        "#,
    );
}

fn matching_commands(commands: &[PiCommand], draft: &str) -> Vec<PiCommand> {
    let Some(query) = draft.strip_prefix('/') else {
        return Vec::new();
    };
    if query.chars().any(char::is_whitespace) {
        return Vec::new();
    }
    let query = query.to_ascii_lowercase();
    commands
        .iter()
        .filter(|command| {
            query.is_empty()
                || command.name.to_ascii_lowercase().contains(&query)
                || command.description.to_ascii_lowercase().contains(&query)
        })
        .take(10)
        .cloned()
        .collect()
}

fn exact_command<'a>(commands: &'a [PiCommand], draft: &str) -> Option<&'a PiCommand> {
    let name = draft.trim().strip_prefix('/')?.split_whitespace().next()?;
    commands.iter().find(|command| command.name == name)
}
