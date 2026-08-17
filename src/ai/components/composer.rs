use dioxus::prelude::*;
use syntaxis_agent::{ExtensionWidget, ImageAttachment, PiCommand, PromptDelivery};
use syntaxis_ui::prelude::{AppIcon, Icon, IconButton};
use syntaxis_workspace::{EntryKind, WorkspaceRecord};

use crate::files::{SearchScope, WorkspaceSearchOptions, search_workspace_files};

mod attachments;
mod bridges;
mod commands;
mod draft;
mod mentions;
mod widgets;

use attachments::ComposerAttachments;
pub(crate) use attachments::load_images;
use bridges::{toggle_speech, use_paste_bridge, use_speech_bridge};
use commands::{SlashCommandMenu, exact_command, matching_commands};
use draft::{clear_saved_draft, use_persisted_draft};
use mentions::{
    FileMentionMenu, append_file_reference, append_text_reference, insert_file_mention,
    mention_query,
};
use widgets::{ExtensionWidgets, QueuePreview};

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
    agent_name: String,
    connected: bool,
    working: bool,
    pending_messages: usize,
    steering_queue: Vec<String>,
    follow_up_queue: Vec<String>,
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
                            placeholder: if working { format!("Steer {agent_name} while it works…") } else { format!("Ask {agent_name} to change or inspect this project…") },
                            aria_label: "Message {agent_name}",
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
                                    label: format!("Stop {agent_name}"),
                                    icon: AppIcon::Stop,
                                    danger: true,
                                    onclick: move |_| on_abort.call(()),
                                }
                            }
                            button {
                                class: "grid size-8.5 place-items-center rounded-lg bg-primary text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-35",
                                disabled: !can_send,
                                aria_label: if working { format!("Steer {agent_name}") } else { "Send message".to_owned() },
                                title: if working { format!("Steer {agent_name}") } else { "Send message".to_owned() },
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
                if !images.is_empty() && !accepts_images {
                    p { class: "px-2.5 pt-1.5 text-[10px] text-warning",
                        "Choose a vision-capable model to send these images."
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
