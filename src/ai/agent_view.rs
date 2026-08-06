use dioxus::html::HasFileData;
use dioxus::prelude::*;
use syntaxis_agent::{AgentSessionSummary, AgentStatus, ClientMessage};
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
use super::runtime::{use_agent_runtime, AgentRuntime};
use super::session::session_action;
use super::worktree::{use_worktree_flow, IsolatedWorktreeDialog};
use super::{components, notifications, AiQuery, AiSettingsSection};

const AI_CHAT_CSS: Asset = asset!("/assets/ai/chat.css");

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
    match active.current() {
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
                "Loading Pi…"
            }
        },
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
    let files_session = use_context::<crate::files::FilesSessionState>();
    let event_state = use_context::<crate::workspace::WorkspaceEventState>();
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
        attachments,
        mut composer_error,
        draft_session,
        creating_session,
        pending_new_prompt,
        client,
    } = runtime;
    let mut drawer = use_signal(|| false);
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
    let mut delete_target = use_signal(|| None::<AgentSessionSummary>);
    let mut session_toast = use_signal(|| None::<(String, Tone)>);
    let worktree_flow = use_worktree_flow(active_workspace, session_toast);
    let drawer_blocked = worktree_flow.is_dialog_open()
        || delete_target().is_some()
        || extension_request().is_some();
    use_effect(move || {
        if worktree_flow.is_dialog_open()
            || delete_target().is_some()
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
            .map_or_else(|| "Pi".into(), |session| session.title)
    };
    let is_working = matches!(
        current.status,
        AgentStatus::Working | AgentStatus::Compacting
    );
    let accepts_images = current
        .model
        .as_ref()
        .is_some_and(|model| model.supports_images);
    let send_prompt = EventHandler::new(move |submission: ComposerSubmission| {
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
    rsx! {
        document::Stylesheet { href: AI_CHAT_CSS }
        div { class: if sidebar_open() { "grid size-full min-h-0 min-w-0 grid-cols-[260px_minmax(0,1fr)] overflow-hidden max-md:block" } else { "grid size-full min-h-0 min-w-0 grid-cols-[minmax(0,1fr)] overflow-hidden max-md:block" },
            if sidebar_open() {
                aside { class: "flex min-h-0 min-w-0 flex-col border-r border-border bg-sidebar max-md:hidden",
                    AiSidebarTabs { panel, on_change: move |_| {} }
                    if panel() == AiPanel::Chat {
                        div { class: "min-h-0 flex-1",
                            AgentSessionSidebar {
                                workspace_id: workspace_id.clone(),
                                sessions: sessions(),
                                selected_id: active_id.clone(),
                                loading: !sessions_loaded(),
                                unavailable: connection_failed,
                                connected,
                                on_select: move |session_id: String| runtime.select_session(session_id),
                                on_new: move |()| runtime.create_session(),
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
                    title: "Pi",
                    label: "AI sidebar",
                    content_class: "h-full w-[min(330px,88vw)] justify-self-start border-0 border-r border-border bg-sidebar shadow-[15px_0_50px_#0008]",
                    restore_focus: "button[aria-label='Open AI sidebar']",
                    on_close: move |()| drawer.set(false),
                    div { class: "flex h-full min-h-0 flex-col",
                        AiSidebarTabs { panel, on_change: move |_| drawer.set(false) }
                        if panel() == AiPanel::Chat {
                            div { class: "min-h-0 flex-1",
                                AgentSessionSidebar {
                                    workspace_id: workspace_id.clone(),
                                    sessions: sessions(),
                                    selected_id: active_id.clone(),
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
                                        drawer.set(false);
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
                        on_model: move |(provider, model_id)| {
                            runtime
                                .send_to_selected(ClientMessage::SetModel {
                                    provider,
                                    model_id,
                                });
                        },
                        on_thinking: move |level| {
                            runtime
                                .send_to_selected(ClientMessage::SetThinkingLevel {
                                    level,
                                });
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
                            loading: session_loading(),
                            creating: creating_session() && draft_session(),
                            unavailable: connection_failed,
                            on_suggestion: move |text: String| {
                                draft.set(text);
                                composer_error.set(None);
                            },
                        }
                        AgentComposer {
                            draft,
                            attachments,
                            composer_error,
                            connected: composer_connected,
                            working: is_working,
                            pending_messages: current.pending_messages,
                            draft_key,
                            commands: current.commands.clone(),
                            accepts_images,
                            on_send: send_prompt,
                            on_abort: move |()| runtime.send_to_selected(ClientMessage::Abort),
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
                    }
                }
            }
        }
        IsolatedWorktreeDialog {
            flow: worktree_flow,
            files_dirty,
            active_workspace,
            files_session,
            event_state,
        }
        if let Some(session) = delete_target() {
            Modal {
                title: "Delete chat?",
                description: "This stops the chat and permanently deletes its Pi session file.",
                on_close: move |()| delete_target.set(None),
                div { class: "rounded-lg border border-border bg-secondary/35 px-3 py-2 text-xs",
                    strong { class: "block truncate", "{session.title}" }
                    if session.running {
                        small { class: "mt-1 block text-warning",
                            "Pi is running in this chat and will be stopped."
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
