use dioxus::html::HasFileData;
use dioxus::prelude::*;
use syntaxis_agent::{
    AgentSessionSummary, AgentStatus, ClientMessage, ImageAttachment, MAX_SESSION_NAME_CHARS,
};
use syntaxis_notifications::NotificationTarget;
use syntaxis_ui::prelude::{Button, ButtonKind, DialogActions, Drawer, Modal, Toast, Tone};

use super::components::{
    AgentComposer, AgentHeader, AgentSessionSidebar, AgentTimeline, ComposerSubmission,
    ExtensionRequestDialog,
};
use super::extensions::ExtensionsPanel;
use super::instructions::GlobalInstructionsPanel;
use super::management::{AiPanel, AiSidebarTabs, SettingsPanel, SettingsSidebar};
use super::resources::{PromptTemplatesPanel, SkillsPanel};
use super::runtime::{AgentRuntime, use_agent_runtime};
use super::session::session_action;
use super::worktree::{IsolatedWorktreeDialog, use_worktree_flow};
use super::{AiQuery, AiSettingsSection, components, notifications};

const AI_CHAT_CSS: Asset = asset!("/assets/ai/chat.css");
const AI_CHAT_SCRIPT: Asset = asset!("/assets/ai-chat.js");

#[derive(Clone)]
struct PendingMessageEdit {
    entry_id: String,
    previous_draft: String,
    previous_attachments: Vec<ImageAttachment>,
}

#[component]
pub fn Ai(slug: String, query: AiQuery) -> Element {
    rsx! {
        AiRoute {
            slug,
            requested_session_id: query.session_id,
            settings_section: None,
        }
    }
}

#[component]
pub fn AiSettings(slug: String, section: AiSettingsSection) -> Element {
    rsx! {
        AiRoute {
            slug,
            requested_session_id: None,
            settings_section: Some(section),
        }
    }
}

#[component]
fn AiRoute(
    slug: String,
    requested_session_id: Option<String>,
    settings_section: Option<AiSettingsSection>,
) -> Element {
    let active = use_context::<crate::workspace::ActiveWorkspace>();
    let content = match active.current() {
        Some(workspace) => rsx! {
            RemoteAgent {
                key: "{workspace.id.0}",
                workspace_id: workspace.id.0,
                workspace_name: workspace.name,
                workspace_slug: slug,
                requested_session_id,
                settings_section,
            }
        },
        None => rsx! {
            div { class: "absolute inset-0 flex flex-col items-center justify-center gap-2 bg-card text-muted-foreground",
                span { class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary" }
                "Loading agent…"
            }
        },
    };
    rsx! {
        document::Script { src: AI_CHAT_SCRIPT }
        {content}
    }
}

#[component]
fn RemoteAgent(
    workspace_id: String,
    workspace_name: String,
    workspace_slug: String,
    requested_session_id: Option<String>,
    settings_section: Option<AiSettingsSection>,
) -> Element {
    let active_workspace = use_context::<crate::workspace::ActiveWorkspace>();
    let notification_center = use_context::<notifications::NotificationCenter>();
    let files_session = use_context::<syntaxis_module_files::FilesUiState>();
    let runtime = use_agent_runtime(
        &workspace_id,
        requested_session_id.as_deref(),
        active_workspace,
    );
    let AgentRuntime {
        connection,
        snapshot,
        sessions,
        sessions_loaded,
        selected_id,
        session_loading,
        mut draft,
        mut error,
        mut extension_request,
        mut attachments,
        mut composer_error,
        draft_session,
        creating_session,
        pending_new_prompt,
        mut client,
        ..
    } = runtime;
    let mut drawer = use_context::<super::AiDrawerState>().open;
    let mut sidebar_open = use_signal(|| true);
    let mut panel = use_signal(|| {
        if settings_section.is_some() {
            AiPanel::Settings
        } else {
            AiPanel::Chat
        }
    });
    let mut selected_settings_section = use_signal(|| settings_section.unwrap_or_default());
    use_effect(use_reactive(
        (&settings_section,),
        move |(settings_section,)| {
            if let Some(section) = settings_section {
                selected_settings_section.set(section);
                panel.set(AiPanel::Settings);
            } else {
                panel.set(AiPanel::Chat);
            }
        },
    ));
    let management_revision = use_signal(|| 0_u64);
    let mut drag_active = use_signal(|| false);
    let mut rename_target = use_signal(|| None::<AgentSessionSummary>);
    let mut rename_value = use_signal(String::new);
    let mut delete_target = use_signal(|| None::<AgentSessionSummary>);
    let mut editing_message = use_signal(|| None::<PendingMessageEdit>);
    let mut compact_dialog = use_signal(|| false);
    let mut compact_instructions = use_signal(String::new);
    let mut session_toast = use_signal(|| None::<(String, Tone)>);
    let worktree_flow = use_worktree_flow(active_workspace, session_toast);
    let drawer_blocked = worktree_flow.is_dialog_open()
        || rename_target().is_some()
        || delete_target().is_some()
        || compact_dialog()
        || extension_request().is_some();
    use_effect(move || {
        if worktree_flow.is_dialog_open()
            || rename_target().is_some()
            || delete_target().is_some()
            || compact_dialog()
            || extension_request().is_some()
        {
            drawer.set(false);
        }
    });
    use_effect({
        let workspace_id = workspace_id.clone();
        move || {
            notification_center.view(
                workspace_id.clone(),
                selected_id().map(|session_id| NotificationTarget::Agent { session_id }),
            );
        }
    });
    use_drop({
        let workspace_id = workspace_id.clone();
        move || notification_center.stop_viewing(&workspace_id)
    });

    let navigator = use_navigator();
    use_effect({
        let workspace_slug = workspace_slug.clone();
        move || {
            if panel() == AiPanel::Settings {
                navigator.replace(crate::app::Route::AiSettings {
                    slug: workspace_slug.clone(),
                    section: selected_settings_section(),
                });
                return;
            }
            if draft_session() {
                navigator.replace(crate::app::Route::Ai {
                    slug: workspace_slug.clone(),
                    query: AiQuery::default(),
                });
                return;
            }
            let Some(session_id) = selected_id() else {
                return;
            };
            navigator.replace(crate::app::Route::Ai {
                slug: workspace_slug.clone(),
                query: AiQuery::with_session(session_id),
            });
        }
    });

    let connected = connection.read().is_ready();
    let connection_failed = connection.read().is_failed();
    let current = snapshot();
    let active_id = selected_id();
    use_effect(use_reactive((&active_id,), move |_| {
        editing_message.set(None);
    }));
    let draft_key = format!(
        "syntaxis:ai-draft:{workspace_id}:{}",
        active_id.as_deref().unwrap_or("new")
    );
    let session_title = if session_loading() {
        if draft_session() {
            "Creating chat…".into()
        } else {
            "Loading chat…".into()
        }
    } else if draft_session() {
        "New chat".into()
    } else {
        active_id
            .as_ref()
            .and_then(|id| sessions().into_iter().find(|session| session.id == *id))
            .map_or_else(|| "Agent".into(), |session| session.title)
    };
    let is_working = matches!(
        current.status,
        AgentStatus::Working | AgentStatus::Compacting
    );
    let agent_name = current
        .model
        .as_ref()
        .map_or_else(|| "Agent".to_owned(), |model| model.name.clone());
    let accepts_images = current
        .model
        .as_ref()
        .is_some_and(|model| model.supports_images);
    let send_prompt = EventHandler::new(move |submission: ComposerSubmission| {
        if let Some(edit) = editing_message() {
            runtime.send_to_selected(ClientMessage::ForkMessage {
                entry_id: edit.entry_id,
            });
            editing_message.set(None);
        }
        runtime.submit_prompt(submission, is_working);
    });
    let files_dirty = files_session.has_dirty();
    let new_worktree_disabled_reason =
        worktree_flow.new_disabled_reason(active_workspace, files_dirty);
    let error_toast = composer_error()
        .or_else(&*error)
        .map(|message| (message, Tone::Destructive));
    let toast_message = error_toast.or_else(&*session_toast);
    let composer_connected = connected
        && (active_id.is_some() || draft_session())
        && !creating_session()
        && pending_new_prompt().is_none();
    let title_detail = if let Some(section) = settings_section {
        format!("AI Settings · {}", section.label())
    } else {
        current
            .extension_title
            .clone()
            .unwrap_or_else(|| session_title.clone())
    };
    let page_title = format!("{workspace_name} · {title_detail}");
    rsx! {
        document::Stylesheet { href: AI_CHAT_CSS }
        document::Title { "{page_title}" }
        div { class: if sidebar_open() { "grid size-full min-h-0 min-w-0 grid-cols-[260px_minmax(0,1fr)] overflow-hidden max-md:block" } else { "grid size-full min-h-0 min-w-0 grid-cols-[minmax(0,1fr)] overflow-hidden max-md:block" },
            if sidebar_open() {
                aside { class: "flex min-h-0 min-w-0 flex-col border-r border-border bg-sidebar max-md:hidden",
                    AiSidebarTabs { panel, on_change: move |_| {} }
                    if panel() == AiPanel::Chat {
                        div { class: "min-h-0 flex-1",
                            AgentSessionSidebar {
                                workspace_id: workspace_id.clone(),
                                sessions: sessions(),
                                selected_id,
                                loading: !sessions_loaded(),
                                unavailable: connection_failed,
                                connected,
                                on_select: move |session_id: String| runtime.select_session(session_id),
                                on_new: move |()| runtime.create_session(),
                                on_clone: move |session_id: String| {
                                    runtime.send_to_session(session_id, ClientMessage::CloneSession);
                                },
                                on_export: move |session_id: String| {
                                    runtime.send_to_session(session_id, ClientMessage::ExportHtml);
                                },
                                on_rename: move |session_id: String| {
                                    if let Some(session) = sessions()
                                        .into_iter()
                                        .find(|session| session.id == session_id)
                                    {
                                        rename_value.set(session.title.clone());
                                        rename_target.set(Some(session));
                                    }
                                },
                                on_delete: move |session_id: String| {
                                    delete_target
                                        .set(sessions().into_iter().find(|session| session.id == session_id));
                                },
                            }
                        }
                    } else {
                        SettingsSidebar {
                            selected: selected_settings_section(),
                            on_selected: {
                                let workspace_slug = workspace_slug.clone();
                                move |section| {
                                    navigator
                                        .push(crate::app::Route::AiSettings {
                                            slug: workspace_slug.clone(),
                                            section,
                                        });
                                }
                            },
                        }
                    }
                }
            }
            if drawer() && !drawer_blocked {
                Drawer {
                    title: "Agent",
                    label: "AI sidebar",
                    content_class: "h-full w-[min(330px,88vw)] justify-self-start border-0 border-r border-border bg-sidebar shadow-[15px_0_50px_#0008]",
                    restore_focus: "button[aria-label='Open AI sidebar']",
                    on_close: move |()| drawer.set(false),
                    div { class: "flex h-full min-h-0 flex-col",
                        AiSidebarTabs { panel, on_change: move |_| {} }
                        if panel() == AiPanel::Chat {
                            div { class: "min-h-0 flex-1",
                                AgentSessionSidebar {
                                    workspace_id: workspace_id.clone(),
                                    sessions: sessions(),
                                    selected_id,
                                    loading: !sessions_loaded(),
                                    unavailable: connection_failed,
                                    connected,
                                    on_select: move |session_id: String| {
                                        runtime.select_session(session_id);
                                        drawer.set(false);
                                    },
                                    on_new: move |()| {
                                        drawer.set(false);
                                        runtime.create_session();
                                    },
                                    on_clone: move |session_id: String| {
                                        drawer.set(false);
                                        runtime.send_to_session(session_id, ClientMessage::CloneSession);
                                    },
                                    on_export: move |session_id: String| {
                                        drawer.set(false);
                                        runtime.send_to_session(session_id, ClientMessage::ExportHtml);
                                    },
                                    on_rename: move |session_id: String| {
                                        drawer.set(false);
                                        if let Some(session) = sessions()
                                            .into_iter()
                                            .find(|session| session.id == session_id)
                                        {
                                            rename_value.set(session.title.clone());
                                            rename_target.set(Some(session));
                                        }
                                    },
                                    on_delete: move |session_id: String| {
                                        drawer.set(false);
                                        delete_target
                                            .set(sessions().into_iter().find(|session| session.id == session_id));
                                    },
                                }
                            }
                        } else {
                            SettingsSidebar {
                                selected: selected_settings_section(),
                                on_selected: {
                                    let workspace_slug = workspace_slug.clone();
                                    move |section| {
                                        navigator
                                            .push(crate::app::Route::AiSettings {
                                                slug: workspace_slug.clone(),
                                                section,
                                            });
                                    }
                                },
                            }
                        }
                    }
                }
            }
            section { class: "flex h-full min-h-0 min-w-0 flex-col overflow-hidden bg-card max-md:h-full",
                if panel() == AiPanel::Chat {
                    AgentHeader {
                        workspace_id: workspace_id.clone(),
                        workspace_name: workspace_name.clone(),
                        connection: connection.read().label(),
                        session_title,
                        snapshot: current.clone(),
                        controls_disabled: !connected || active_id.is_none() || is_working,
                        workspace_locked: !current.items.is_empty() || is_working,
                        new_worktree_disabled_reason,
                        sidebar_open: sidebar_open(),
                        on_toggle_sidebar: move |()| sidebar_open.toggle(),
                        on_open_sidebar: move |()| drawer.set(true),
                        on_new_worktree: move |()| worktree_flow.open_dialog(),
                        on_model: move |(provider, model_id, thinking_level)| {
                            runtime
                                .send_to_selected(ClientMessage::SetModel {
                                    provider,
                                    model_id,
                                    thinking_level,
                                });
                        },
                        on_thinking: move |level| {
                            runtime
                                .send_to_selected(ClientMessage::SetThinkingLevel {
                                    level,
                                });
                        },
                        on_compact: move |()| {
                            compact_instructions.set(String::new());
                            compact_dialog.set(true);
                        },
                    }
                    if let Some(message) = connection.read().banner() {
                        div { class: "border-b border-warning/25 bg-warning/8 px-3 py-2 text-center text-[11px] text-warning",
                            "{message}"
                        }
                    }
                    div {
                        class: "relative flex min-h-0 flex-1 flex-col overflow-hidden",
                        ondragover: move |event: DragEvent| {
                            event.prevent_default();
                            if accepts_images && connected {
                                drag_active.set(true);
                            }
                        },
                        ondragleave: move |_| drag_active.set(false),
                        ondrop: move |event: DragEvent| {
                            event.prevent_default();
                            drag_active.set(false);
                            if accepts_images && connected {
                                spawn(components::load_images(event.files(), attachments, composer_error));
                            }
                        },
                        AgentTimeline {
                            items: current.items.clone(),
                            status: current.status,
                            agent_name: agent_name.clone(),
                            loading: session_loading(),
                            creating: creating_session() && draft_session(),
                            unavailable: connection_failed,
                            can_edit: connected && !is_working,
                            on_suggestion: move |text: String| {
                                draft.set(text);
                                composer_error.set(None);
                                focus_ai_composer();
                            },
                            on_edit: move |(entry_id, text, images): (String, String, Vec<ImageAttachment>)| {
                                let (previous_draft, previous_attachments) = editing_message()
                                    .map_or_else(
                                        || (draft(), attachments()),
                                        |edit| (edit.previous_draft, edit.previous_attachments),
                                    );
                                editing_message
                                    .set(
                                        Some(PendingMessageEdit {
                                            entry_id,
                                            previous_draft,
                                            previous_attachments,
                                        }),
                                    );
                                draft.set(text);
                                attachments.set(images);
                                composer_error.set(None);
                                focus_ai_composer();
                            },
                            on_copy: move |text: String| copy_ai_message(text, session_toast),
                        }
                        AgentComposer {
                            draft,
                            attachments,
                            composer_error,
                            agent_name,
                            connected: composer_connected,
                            working: is_working,
                            pending_messages: current.pending_messages,
                            steering_queue: current.steering_queue.clone(),
                            follow_up_queue: current.follow_up_queue.clone(),
                            extension_widgets: current.extension_widgets.clone(),
                            draft_key,
                            commands: current.commands.clone(),
                            accepts_images,
                            editing_message: editing_message().is_some(),
                            workspace: active_workspace
                                .current()
                                .expect("active workspace is available in the agent route"),
                            active_file: files_session.active_path(),
                            active_reference: files_session.active_reference(),
                            on_send: send_prompt,
                            on_abort: move |()| runtime.send_to_selected(ClientMessage::Abort),
                            on_cancel_edit: move |()| {
                                if let Some(edit) = editing_message() {
                                    draft.set(edit.previous_draft);
                                    attachments.set(edit.previous_attachments);
                                    editing_message.set(None);
                                    composer_error.set(None);
                                    focus_ai_composer();
                                }
                            },
                        }
                        if drag_active() {
                            div { class: "pointer-events-none absolute inset-3 z-90 grid place-items-center rounded-2xl border-2 border-dashed border-primary bg-primary/10 text-sm font-medium text-primary backdrop-blur-sm",
                                "Drop images to attach"
                            }
                        }
                    }
                } else if selected_settings_section() == AiSettingsSection::GlobalInstructions {
                    GlobalInstructionsPanel {
                        workspace_id: workspace_id.clone(),
                        toast: session_toast,
                        sidebar_open: sidebar_open(),
                        on_toggle_sidebar: move |()| sidebar_open.toggle(),
                        on_open_sidebar: move |()| drawer.set(true),
                    }
                } else if selected_settings_section() == AiSettingsSection::Extensions {
                    ExtensionsPanel {
                        workspace_id: workspace_id.clone(),
                        revision: management_revision,
                        toast: session_toast,
                        sidebar_open: sidebar_open(),
                        on_toggle_sidebar: move |()| sidebar_open.toggle(),
                        on_open_sidebar: move |()| drawer.set(true),
                    }
                } else if selected_settings_section() == AiSettingsSection::PromptTemplates {
                    PromptTemplatesPanel {
                        workspace_id: workspace_id.clone(),
                        revision: management_revision,
                        toast: session_toast,
                        sidebar_open: sidebar_open(),
                        on_toggle_sidebar: move |()| sidebar_open.toggle(),
                        on_open_sidebar: move |()| drawer.set(true),
                    }
                } else if selected_settings_section() == AiSettingsSection::Skills {
                    SkillsPanel {
                        workspace_id: workspace_id.clone(),
                        revision: management_revision,
                        toast: session_toast,
                        sidebar_open: sidebar_open(),
                        on_toggle_sidebar: move |()| sidebar_open.toggle(),
                        on_open_sidebar: move |()| drawer.set(true),
                    }
                } else {
                    SettingsPanel {
                        workspace_id: workspace_id.clone(),
                        revision: management_revision,
                        toast: session_toast,
                        selected_section: selected_settings_section,
                        sidebar_open: sidebar_open(),
                        on_toggle_sidebar: move |()| sidebar_open.toggle(),
                        on_open_sidebar: move |()| drawer.set(true),
                        on_provider_accounts_changed: move |()| {
                            client.restart();
                        },
                    }
                }
            }
        }
        IsolatedWorktreeDialog {
            flow: worktree_flow,
            files_dirty,
            active_workspace,
            files_session,
        }
        if compact_dialog() {
            Modal {
                title: "Compact context",
                description: "Pi will summarize older conversation context while keeping the current task active.",
                on_close: move |()| compact_dialog.set(false),
                div { class: "flex flex-col gap-2.5 px-5 pt-3 pb-5",
                    label {
                        class: "text-xs font-medium",
                        r#for: "compact-instructions",
                        "Optional instructions"
                    }
                    textarea {
                        id: "compact-instructions",
                        class: "min-h-24 w-full resize-y rounded-md border border-input bg-background p-3 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/20",
                        value: compact_instructions(),
                        autofocus: true,
                        placeholder: "For example: preserve test failures and changed files",
                        oninput: move |event| compact_instructions.set(event.value()),
                    }
                    DialogActions {
                        Button {
                            label: "Cancel",
                            kind: ButtonKind::Ghost,
                            onclick: move |_| compact_dialog.set(false),
                        }
                        Button {
                            label: "Compact context",
                            kind: ButtonKind::Primary,
                            onclick: move |_| {
                                let instructions = compact_instructions().trim().to_owned();
                                runtime
                                    .send_to_selected(ClientMessage::Compact {
                                        custom_instructions: (!instructions.is_empty()).then_some(instructions),
                                    });
                                compact_dialog.set(false);
                            },
                        }
                    }
                }
            }
        }
        if let Some(session) = rename_target() {
            Modal {
                title: "Rename chat",
                description: "Set the display name stored by the agent for this session.",
                on_close: move |()| rename_target.set(None),
                div { class: "flex flex-col gap-2.25 px-5 pt-3 pb-5",
                    label {
                        class: "text-xs font-medium text-foreground",
                        r#for: "ai-session-name",
                        "Chat name"
                    }
                    input {
                        id: "ai-session-name",
                        class: "h-10 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/20",
                        r#type: "text",
                        value: rename_value(),
                        autofocus: true,
                        maxlength: MAX_SESSION_NAME_CHARS,
                        oninput: move |event| rename_value.set(event.value()),
                    }
                    DialogActions {
                        Button {
                            label: "Cancel",
                            kind: ButtonKind::Ghost,
                            onclick: move |_| rename_target.set(None),
                        }
                        Button {
                            label: "Rename chat",
                            kind: ButtonKind::Primary,
                            disabled: rename_value().trim().is_empty(),
                            onclick: move |_| {
                                client
                                    .send(ClientMessage::RenameSession {
                                        session_id: session.id.clone(),
                                        name: rename_value().trim().to_owned(),
                                    });
                                rename_target.set(None);
                            },
                        }
                    }
                }
            }
        }
        if let Some(session) = delete_target() {
            Modal {
                title: "Delete chat?",
                description: "This stops the chat and permanently deletes its agent session file.",
                on_close: move |()| delete_target.set(None),
                div { class: "rounded-lg border border-border bg-secondary/35 px-3 py-2 text-xs",
                    strong { class: "block truncate", "{session.title}" }
                    if session.running {
                        small { class: "mt-1 block text-warning",
                            "The agent is running in this chat and will be stopped."
                        }
                    }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        onclick: move |_| delete_target.set(None),
                    }
                    Button {
                        label: "Delete chat",
                        kind: ButtonKind::Danger,
                        onclick: move |_| {
                            client
                                .send(ClientMessage::DeleteSession {
                                    session_id: session.id.clone(),
                                });
                            delete_target.set(None);
                        },
                    }
                }
            }
        }
        if let Some(request) = extension_request() {
            ExtensionRequestDialog {
                request: request.clone(),
                on_respond: move |(value, confirmed, cancelled)| {
                    if let Some(session_id) = selected_id() {
                        client
                            .send(
                                session_action(
                                    session_id,
                                    ClientMessage::ExtensionUiResponse {
                                        request_id: request.id.clone(),
                                        value,
                                        confirmed,
                                        cancelled,
                                    },
                                ),
                            );
                    }
                    extension_request.set(None);
                },
            }
        }
        if let Some((message, tone)) = toast_message {
            Toast {
                message,
                tone,
                on_close: move |()| {
                    composer_error.set(None);
                    error.set(None);
                    session_toast.set(None);
                },
            }
        }
    }
}

pub(crate) fn focus_ai_composer() {
    let _ = document::eval(
        r#"
        requestAnimationFrame(() => {
            const composer = document.getElementById("syntaxis-ai-composer");
            composer?.focus({ preventScroll: false });
            if (composer) composer.setSelectionRange(composer.value.length, composer.value.length);
        });
        "#,
    );
}

fn copy_ai_message(value: String, mut toast: Signal<Option<(String, Tone)>>) {
    spawn(async move {
        match crate::clipboard::copy_text(value).await {
            Ok(()) => toast.set(Some(("Message copied".into(), Tone::Success))),
            Err(error) => toast.set(Some((
                format!("Could not copy message: {error}"),
                Tone::Destructive,
            ))),
        }
    });
}
