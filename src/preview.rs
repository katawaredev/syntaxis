use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::DropdownMenuItem;
use serde::{Deserialize, Serialize};
use syntaxis_ui::prelude::{AppIcon, ComboButton, Icon, InteractivePopover, Toast, Tone};

use crate::client_error::server_error_message;

#[cfg(feature = "server")]
pub(crate) mod server;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct PreviewConfig {
    #[serde(default)]
    pub target: Option<PreviewTarget>,
    // Kept for config files written before explicit URL targets were supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default)]
    pub start_command: String,
    #[serde(default)]
    pub stop_command: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub(crate) enum PreviewTarget {
    Loopback { port: u16 },
    Url { url: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct PreviewLease {
    pub id: String,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct PreviewShare {
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct PreviewSession {
    pub lease: PreviewLease,
    pub share: Option<PreviewShare>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct PreviewCandidate {
    pub port: u16,
    pub process: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct PreviewProcessStatus {
    pub running: bool,
}

#[derive(Clone, Copy)]
struct PreviewConnectionState {
    lease: Signal<Option<PreviewLease>>,
    share: Signal<Option<PreviewShare>>,
    toast: Signal<Option<(String, Tone)>>,
    connecting: Signal<bool>,
    reload_key: Signal<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreviewAction {
    Connect,
    Start,
    Stop,
}

#[get("/api/previews/{workspace_id}")]
async fn preview_config(workspace_id: String) -> Result<PreviewConfig, ServerFnError> {
    server::preview_config(workspace_id).await
}

#[get("/api/previews/{workspace_id}/candidates")]
async fn preview_candidates(workspace_id: String) -> Result<Vec<PreviewCandidate>, ServerFnError> {
    server::preview_candidates(workspace_id).await
}

#[get("/api/previews/{workspace_id}/process")]
async fn preview_process_status(
    workspace_id: String,
) -> Result<PreviewProcessStatus, ServerFnError> {
    server::preview_process_status(workspace_id).await
}

#[post("/api/previews/{workspace_id}/settings")]
async fn update_preview_config(
    workspace_id: String,
    config: PreviewConfig,
) -> Result<(), ServerFnError> {
    server::update_preview_config(workspace_id, config).await
}

#[post("/api/previews/{workspace_id}/process/start")]
async fn start_preview_process(
    workspace_id: String,
    start_command: String,
    stop_command: String,
) -> Result<PreviewProcessStatus, ServerFnError> {
    server::start_preview_process(workspace_id, start_command, stop_command).await
}

#[post("/api/previews/{workspace_id}/process/stop")]
async fn stop_preview_process(
    workspace_id: String,
    stop_command: String,
) -> Result<PreviewProcessStatus, ServerFnError> {
    server::stop_preview_process(workspace_id, stop_command).await
}

#[post(
    "/api/previews/{workspace_id}/lease",
    headers: dioxus::fullstack::HeaderMap
)]
async fn create_preview_lease(
    workspace_id: String,
    target: PreviewTarget,
) -> Result<PreviewLease, ServerFnError> {
    server::create_preview_lease(workspace_id, target, &headers).await
}

#[post(
    "/api/previews/{workspace_id}/session/resume",
    headers: dioxus::fullstack::HeaderMap
)]
async fn resume_preview_session(
    workspace_id: String,
) -> Result<Option<PreviewSession>, ServerFnError> {
    server::resume_preview_session(workspace_id, &headers).await
}

#[post("/api/previews/{workspace_id}/leases/{lease_id}/share")]
async fn create_preview_share(
    workspace_id: String,
    lease_id: String,
) -> Result<PreviewShare, ServerFnError> {
    server::create_preview_share(workspace_id, lease_id).await
}

#[post("/api/previews/{workspace_id}/leases/{lease_id}/share/revoke")]
async fn revoke_preview_share(workspace_id: String, lease_id: String) -> Result<(), ServerFnError> {
    server::revoke_preview_share(workspace_id, lease_id).await
}

#[component]
pub(crate) fn Preview(slug: String) -> Element {
    let _ = slug;
    let active = use_context::<crate::workspace::ActiveWorkspace>();
    match active.current() {
        Some(workspace) => rsx! {
            SuspenseBoundary {
                fallback: |_| rsx! {
                    PreviewLoading {}
                },
                WorkspacePreview { key: "{workspace.id.0}", workspace_id: workspace.id.0 }
            }
        },
        None => rsx! {
            PreviewLoading {}
        },
    }
}

#[component]
fn PreviewLoading() -> Element {
    rsx! {
        div {
            class: "flex size-full items-center justify-center gap-2 bg-card text-sm text-muted-foreground",
            role: "status",
            span {
                class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary",
                aria_hidden: "true",
            }
            "Loading workspace preview…"
        }
    }
}

#[component]
fn WorkspacePreview(workspace_id: String) -> Element {
    let config_workspace_id = workspace_id.clone();
    let config = use_server_future(move || {
        let workspace_id = config_workspace_id.clone();
        async move { preview_config(workspace_id).await }
    })?;
    let candidates_workspace_id = workspace_id.clone();
    let mut candidates = use_server_future(move || {
        let workspace_id = candidates_workspace_id.clone();
        async move { preview_candidates(workspace_id).await }
    })?;
    let commands_workspace_id = workspace_id.clone();
    let commands = use_server_future(move || {
        let workspace_id = commands_workspace_id.clone();
        async move { crate::terminal::api::list_run_commands(workspace_id).await }
    })?;
    let process_workspace_id = workspace_id.clone();
    let process_status = use_server_future(move || {
        let workspace_id = process_workspace_id.clone();
        async move { preview_process_status(workspace_id).await }
    })?;
    let session_workspace_id = workspace_id.clone();
    let session = use_server_future(move || {
        let workspace_id = session_workspace_id.clone();
        async move { resume_preview_session(workspace_id).await }
    })?;
    let mut config_applied = use_signal(|| false);
    let mut session_applied = use_signal(|| false);
    let mut auto_connect_applied = use_signal(|| false);
    let mut process_status_applied = use_signal(|| false);
    let mut detection_requested = use_signal(|| false);
    let mut configured_target = use_signal(|| None::<PreviewTarget>);
    let mut url_target = use_signal(|| false);
    let mut port = use_signal(String::new);
    let mut url = use_signal(|| "http://".to_owned());
    let mut lease = use_signal(|| None::<PreviewLease>);
    let mut share = use_signal(|| None::<PreviewShare>);
    let mut sharing = use_signal(|| false);
    let mut combo_open = use_signal(|| false);
    let mut settings_open = use_signal(|| false);
    let mut start_command = use_signal(String::new);
    let mut stop_command = use_signal(String::new);
    let mut process_running = use_signal(|| false);
    let mut process_busy = use_signal(|| false);
    let mut toast = use_signal(|| None::<(String, Tone)>);
    let connecting = use_signal(|| false);
    let mut frame_loading = use_signal(|| false);
    let mut reload_key = use_signal(|| 0_u64);
    let connection_state = PreviewConnectionState {
        lease,
        share,
        toast,
        connecting,
        reload_key,
    };
    let detected = candidates().and_then(Result::ok).unwrap_or_default();
    let detected_commands = commands().and_then(Result::ok).unwrap_or_default();
    let (suggested_start_command, suggested_stop_command) =
        suggest_preview_commands(&detected_commands);
    let detected_target = single_candidate_target(&detected);
    let config_loading = config.state() == UseResourceState::Pending;
    let candidates_loading = candidates.state() == UseResourceState::Pending;
    let process_status_loading = process_status.state() == UseResourceState::Pending;
    let restoring = session.state() == UseResourceState::Pending;
    let initializing = config_loading || process_status_loading || restoring;
    let controls_busy = connecting() || sharing() || initializing;
    let access_busy = connecting() || sharing();
    let target_missing = if url_target() {
        url().trim().is_empty()
    } else {
        port().trim().is_empty()
    };
    let primary_action = default_preview_action(process_running(), &start_command());
    let controls_disabled = initializing
        || connecting()
        || process_busy()
        || (primary_action == PreviewAction::Connect && target_missing);
    let start_disabled =
        process_busy() || (!process_running() && start_command().trim().is_empty());
    let (primary_label, primary_title, primary_icon) = if initializing {
        ("Preparing…", "Preparing preview controls", AppIcon::Refresh)
    } else if connecting() {
        ("Connecting…", "Connecting to preview", AppIcon::Refresh)
    } else if process_busy() {
        (
            "Working…",
            "Preview command is changing state",
            AppIcon::Refresh,
        )
    } else {
        match primary_action {
            PreviewAction::Connect => ("Connect", "Connect to the preview target", AppIcon::Eye),
            PreviewAction::Start => (
                "Start",
                "Start the configured preview command",
                AppIcon::Play,
            ),
            PreviewAction::Stop => ("Stop", "Stop the preview command", AppIcon::Stop),
        }
    };

    use_effect(move || {
        let active_lease = lease().map(|lease| lease.id);
        let _ = reload_key();
        frame_loading.set(active_lease.is_some());
    });

    use_effect(move || {
        if config_applied() {
            return;
        }
        let Some(result) = config() else {
            return;
        };
        config_applied.set(true);
        match result {
            Ok(config) => {
                start_command.set(config.start_command.clone());
                stop_command.set(config.stop_command.clone());
                let target = config
                    .target
                    .or_else(|| config.port.map(|port| PreviewTarget::Loopback { port }));
                configured_target.set(target.clone());
                match target {
                    Some(PreviewTarget::Loopback {
                        port: configured_port,
                    }) => port.set(configured_port.to_string()),
                    Some(PreviewTarget::Url {
                        url: configured_url,
                    }) => {
                        url_target.set(true);
                        url.set(configured_url);
                    }
                    None => port.set("5173".into()),
                }
            }
            Err(problem) => {
                port.set("5173".into());
                set_preview_error(toast, server_error_message(problem));
            }
        }
    });

    use_effect(move || {
        if !detection_requested() {
            return;
        }
        let Some(result) = candidates() else {
            return;
        };
        detection_requested.set(false);
        match result {
            Ok(available) if available.is_empty() => {
                set_preview_error(toast, "No running preview server was detected.");
            }
            Ok(available) => {
                if let Some(candidate) = available.first() {
                    url_target.set(false);
                    port.set(candidate.port.to_string());
                    toast.set(Some((
                        format!("Detected {} on port {}", candidate.process, candidate.port),
                        Tone::Success,
                    )));
                }
            }
            Err(problem) => set_preview_error(toast, server_error_message(problem)),
        }
    });

    use_effect(move || {
        if process_status_applied() {
            return;
        }
        let Some(result) = process_status() else {
            return;
        };
        process_status_applied.set(true);
        match result {
            Ok(status) => process_running.set(status.running),
            Err(problem) => set_preview_error(toast, server_error_message(problem)),
        }
    });

    use_effect(move || {
        if session_applied() {
            return;
        }
        let Some(result) = session() else {
            return;
        };
        session_applied.set(true);
        match result {
            Ok(Some(restored)) => {
                lease.set(Some(restored.lease));
                share.set(restored.share);
                *reload_key.write() += 1;
            }
            Ok(None) => {}
            Err(problem) => set_preview_error(toast, server_error_message(problem)),
        }
    });

    let connect_workspace_id = workspace_id.clone();
    let connect = move || {
        if restoring || connecting() {
            return;
        }
        let target = match selected_preview_target(url_target(), &port(), &url()) {
            Ok(target) => target,
            Err(message) => {
                set_preview_error(toast, message);
                return;
            }
        };
        connect_preview_target(connect_workspace_id.clone(), target, connection_state);
    };

    let auto_workspace_id = workspace_id.clone();
    use_effect(move || {
        if auto_connect_applied()
            || !config_applied()
            || !session_applied()
            || restoring
            || lease().is_some()
            || connecting()
        {
            return;
        }
        let target = configured_target().or_else(|| detected_target.clone());
        if target.is_none() && candidates.state() == UseResourceState::Pending {
            return;
        }
        let Some(target) = target else {
            return;
        };
        auto_connect_applied.set(true);
        apply_preview_target(&target, url_target, port, url);
        connect_preview_target(auto_workspace_id.clone(), target, connection_state);
    });

    let share_workspace_id = workspace_id.clone();
    let mut enable_sharing = move || {
        let Some(active_lease) = lease() else {
            return;
        };
        sharing.set(true);
        toast.set(None);
        let workspace_id = share_workspace_id.clone();
        spawn(async move {
            match create_preview_share(workspace_id, active_lease.id).await {
                Ok(created) => share.set(Some(created)),
                Err(problem) => set_preview_error(toast, server_error_message(problem)),
            }
            sharing.set(false);
        });
    };

    let revoke_workspace_id = workspace_id.clone();
    let mut revoke_sharing = move || {
        let Some(active_lease) = lease() else {
            return;
        };
        sharing.set(true);
        toast.set(None);
        let workspace_id = revoke_workspace_id.clone();
        spawn(async move {
            match revoke_preview_share(workspace_id, active_lease.id).await {
                Ok(()) => share.set(None),
                Err(problem) => set_preview_error(toast, server_error_message(problem)),
            }
            sharing.set(false);
        });
    };

    let manage_workspace_id = workspace_id.clone();
    let toggle_process = move || {
        if process_busy() {
            return;
        }
        if !process_running() && start_command().trim().is_empty() {
            settings_open.set(true);
            set_preview_error(toast, "Enter a start command for this preview.");
            return;
        }
        process_busy.set(true);
        toast.set(None);
        let workspace_id = manage_workspace_id.clone();
        let start = start_command().trim().to_owned();
        let stop = stop_command().trim().to_owned();
        let stopping = process_running();
        spawn(async move {
            let status_workspace_id = workspace_id.clone();
            let result = if stopping {
                stop_preview_process(workspace_id, stop).await
            } else {
                start_preview_process(workspace_id, start, stop).await
            };
            match result {
                Ok(status) => {
                    process_running.set(status.running);
                    if status.running {
                        toast.set(Some(("Preview command started".into(), Tone::Success)));
                        dioxus_sdk_time::sleep(std::time::Duration::from_millis(700)).await;
                        if let Ok(status) = preview_process_status(status_workspace_id).await
                            && !status.running
                        {
                            process_running.set(false);
                            set_preview_error(
                                toast,
                                "The preview command exited before a server became available.",
                            );
                        }
                        candidates.restart();
                    } else {
                        lease.set(None);
                        share.set(None);
                        toast.set(Some(("Preview command stopped".into(), Tone::Success)));
                    }
                }
                Err(problem) => {
                    if stopping {
                        process_running.set(false);
                        lease.set(None);
                        share.set(None);
                    }
                    set_preview_error(toast, server_error_message(problem));
                }
            }
            process_busy.set(false);
        });
    };

    let save_settings_workspace_id = workspace_id.clone();
    let persist_settings = move || {
        if process_busy() {
            return;
        }
        let target = match selected_preview_target(url_target(), &port(), &url()) {
            Ok(target) => target,
            Err(message) => {
                set_preview_error(toast, message);
                return;
            }
        };
        process_busy.set(true);
        toast.set(None);
        let workspace_id = save_settings_workspace_id.clone();
        let start = start_command().trim().to_owned();
        let stop = stop_command().trim().to_owned();
        let saved_target = target.clone();
        spawn(async move {
            let config = PreviewConfig {
                target: Some(target),
                port: None,
                start_command: start,
                stop_command: stop,
            };
            match update_preview_config(workspace_id, config).await {
                Ok(()) => {
                    configured_target.set(Some(saved_target));
                    settings_open.set(false);
                    toast.set(Some(("Preview settings saved".into(), Tone::Success)));
                }
                Err(problem) => set_preview_error(toast, server_error_message(problem)),
            }
            process_busy.set(false);
        });
    };

    rsx! {
        section { class: "flex size-full min-h-0 flex-col bg-card",
            header { class: "flex min-h-12 flex-nowrap items-center gap-2 border-b border-border bg-background px-3 py-2",
                div { class: "flex min-w-0 flex-1 items-center gap-2",
                    span { class: "grid size-7 shrink-0 place-items-center rounded-md bg-primary/10 text-primary",
                        Icon { icon: AppIcon::Eye, size: 14 }
                    }
                    span { class: "min-w-0",
                        strong { class: "block truncate text-xs font-semibold", "Preview" }
                        small { class: "block truncate text-[9px] text-muted-foreground",
                            if url_target() {
                                "{url}"
                            } else {
                                "Port {port}"
                            }
                        }
                    }
                }
                InteractivePopover {
                    id: "preview-settings",
                    label: "Preview settings",
                    open: settings_open(),
                    on_open_change: move |next| settings_open.set(next),
                    trigger_class: if settings_open() { "touch-target inline-flex h-7 items-center gap-1.5 rounded-md bg-accent px-2 text-[11px] font-medium text-foreground max-[520px]:justify-center max-[520px]:gap-0 max-[520px]:px-0" } else { "touch-target inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-[11px] font-medium text-muted-foreground hover:bg-accent hover:text-foreground max-[520px]:justify-center max-[520px]:gap-0 max-[520px]:px-0" },
                    content_class: "absolute top-[calc(100%+6px)] right-0 z-80 max-h-[calc(var(--app-height,100dvh)-4rem)] w-[min(390px,calc(100vw-1rem))] overflow-y-auto rounded-xl border border-border bg-popover p-3 shadow-2xl",
                    trigger: rsx! {
                        Icon { icon: AppIcon::Settings, size: 14 }
                        span { class: "max-[520px]:hidden", "Settings" }
                    },
                    div { class: "mb-3",
                        strong { class: "block text-xs font-semibold", "Preview settings" }
                        p { class: "mt-1 text-[10px] leading-relaxed text-muted-foreground",
                            "Configure how the preview starts and where it connects."
                        }
                    }
                    div { class: "grid gap-3",
                        label { class: "min-w-0 text-[10px] font-semibold text-muted-foreground",
                            "Start command"
                            input {
                                class: "mt-1 h-9 w-full rounded-md border border-input bg-background px-2.5 font-mono text-xs text-foreground outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35",
                                r#type: "text",
                                list: "preview-start-commands",
                                placeholder: suggested_start_command.as_deref().unwrap_or("npm run dev"),
                                value: start_command,
                                disabled: process_busy() || process_running(),
                                oninput: move |event| start_command.set(event.value()),
                            }
                        }
                        datalist { id: "preview-start-commands",
                            for command in &detected_commands {
                                option {
                                    value: command.command.clone(),
                                    label: command.label.clone(),
                                }
                            }
                        }
                        label { class: "min-w-0 text-[10px] font-semibold text-muted-foreground",
                            "Stop command (optional)"
                            input {
                                class: "mt-1 h-9 w-full rounded-md border border-input bg-background px-2.5 font-mono text-xs text-foreground outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35",
                                r#type: "text",
                                list: "preview-stop-commands",
                                placeholder: suggested_stop_command.as_deref().unwrap_or("docker compose down"),
                                value: stop_command,
                                disabled: process_busy(),
                                oninput: move |event| stop_command.set(event.value()),
                            }
                        }
                        datalist { id: "preview-stop-commands",
                            for command in &detected_commands {
                                option {
                                    value: command.command.clone(),
                                    label: command.label.clone(),
                                }
                            }
                        }
                        div { class: "grid grid-cols-[6.5rem_minmax(0,1fr)] gap-2",
                            label { class: "text-[10px] font-semibold text-muted-foreground",
                                "Target"
                                select {
                                    class: "mt-1 h-9 w-full rounded-md border border-input bg-background px-2 text-xs font-semibold text-foreground outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35",
                                    "aria-label": "Preview target type",
                                    value: if url_target() { "url" } else { "port" },
                                    disabled: controls_busy,
                                    onchange: move |event| url_target.set(event.value() == "url"),
                                    option { value: "port", "Port" }
                                    option { value: "url", "URL" }
                                }
                            }
                            if url_target() {
                                label { class: "min-w-0 text-[10px] font-semibold text-muted-foreground",
                                    "URL"
                                    input {
                                        class: "mt-1 h-9 w-full rounded-md border border-input bg-background px-2.5 text-xs text-foreground outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35",
                                        r#type: "url",
                                        placeholder: "http://service:3000",
                                        value: url,
                                        disabled: controls_busy,
                                        oninput: move |event| url.set(event.value()),
                                    }
                                }
                            } else {
                                label { class: "min-w-0 text-[10px] font-semibold text-muted-foreground",
                                    "Port"
                                    input {
                                        class: "mt-1 h-9 w-full rounded-md border border-input bg-background px-2.5 text-xs text-foreground outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35",
                                        r#type: "number",
                                        list: "preview-detected-ports",
                                        min: "1",
                                        max: "65535",
                                        inputmode: "numeric",
                                        value: port,
                                        disabled: controls_busy,
                                        oninput: move |event| port.set(event.value()),
                                    }
                                }
                            }
                        }
                        datalist { id: "preview-detected-ports",
                            for candidate in &detected {
                                option {
                                    value: candidate.port.to_string(),
                                    label: candidate.process.clone(),
                                }
                            }
                        }
                        if !url_target() {
                            button {
                                class: "inline-flex h-8 items-center justify-center gap-1.5 rounded-md border border-border bg-secondary px-2 text-[11px] font-medium text-secondary-foreground hover:bg-accent disabled:cursor-wait disabled:opacity-50",
                                r#type: "button",
                                disabled: candidates_loading || detection_requested(),
                                onclick: move |_| {
                                    candidates.restart();
                                    detection_requested.set(true);
                                },
                                span { class: if candidates_loading || detection_requested() { "animate-spin" } else { "" },
                                    Icon { icon: AppIcon::Refresh, size: 13 }
                                }
                                if candidates_loading || detection_requested() {
                                    "Detecting…"
                                } else {
                                    "Detect preview server"
                                }
                            }
                        }
                        p { class: "text-[9px] leading-relaxed text-muted-foreground",
                            "Stop terminates the started process first, then runs the optional stop command."
                        }
                        button {
                            class: "inline-flex h-9 items-center justify-center rounded-md bg-primary px-3 text-xs font-semibold text-primary-foreground hover:bg-primary/90 disabled:cursor-wait disabled:opacity-60",
                            r#type: "button",
                            disabled: process_busy() || target_missing,
                            onclick: {
                                let mut persist_settings = persist_settings.clone();
                                move |_| persist_settings()
                            },
                            if process_busy() {
                                "Saving…"
                            } else {
                                "Save settings"
                            }
                        }
                    }
                    if let Some(active_lease) = lease() {
                        div { class: "mt-3 grid gap-1 border-t border-border pt-2",
                            button {
                                class: "flex h-8 items-center gap-2 rounded-md px-2 text-left text-xs text-muted-foreground hover:bg-accent hover:text-foreground",
                                r#type: "button",
                                disabled: connecting(),
                                onclick: move |_| *reload_key.write() += 1,
                                Icon { icon: AppIcon::Refresh, size: 14 }
                                "Reload preview"
                            }
                            button {
                                class: "flex h-8 items-center gap-2 rounded-md px-2 text-left text-xs text-muted-foreground hover:bg-accent hover:text-foreground",
                                r#type: "button",
                                onclick: {
                                    let url = active_lease.url.clone();
                                    move |_| open_preview_window(&url)
                                },
                                Icon { icon: AppIcon::ExternalLink, size: 14 }
                                "Open in new tab"
                            }
                            if share().is_none() {
                                button {
                                    class: "flex h-8 items-center gap-2 rounded-md px-2 text-left text-xs text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-50",
                                    r#type: "button",
                                    disabled: access_busy,
                                    onclick: move |_| enable_sharing(),
                                    Icon { icon: AppIcon::Share, size: 14 }
                                    if sharing() {
                                        "Sharing…"
                                    } else {
                                        "Share preview"
                                    }
                                }
                            }
                        }
                    }
                }
                ComboButton {
                    label: primary_label,
                    title: primary_title,
                    icon: primary_icon,
                    danger: primary_action == PreviewAction::Stop && !process_busy(),
                    disabled: controls_disabled,
                    open: combo_open(),
                    menu_label: "Preview actions",
                    menu_class: "w-48",
                    on_click: {
                        let connect = connect.clone();
                        let mut toggle_process = toggle_process.clone();
                        move |()| match primary_action {
                            PreviewAction::Connect => connect(),
                            PreviewAction::Start | PreviewAction::Stop => toggle_process(),
                        }
                    },
                    on_open_change: move |next| combo_open.set(next),
                    DropdownMenuItem::<PreviewAction> {
                        class: if primary_action == PreviewAction::Connect { "!bg-accent !text-foreground" } else { "" },
                        value: PreviewAction::Connect,
                        index: 0_usize,
                        disabled: controls_busy || target_missing,
                        on_select: {
                            let connect = connect.clone();
                            move |_| {
                                combo_open.set(false);
                                connect();
                            }
                        },
                        span { class: "flex items-center gap-2",
                            Icon { icon: AppIcon::Eye, size: 14 }
                            "Connect"
                        }
                    }
                    DropdownMenuItem::<PreviewAction> {
                        class: if primary_action == PreviewAction::Connect { "" } else { "!bg-accent !text-foreground" },
                        value: if process_running() { PreviewAction::Stop } else { PreviewAction::Start },
                        index: 1_usize,
                        disabled: start_disabled,
                        on_select: {
                            let mut toggle_process = toggle_process.clone();
                            move |_| {
                                combo_open.set(false);
                                toggle_process();
                            }
                        },
                        span { class: "flex items-center gap-2",
                            Icon {
                                icon: if process_running() { AppIcon::Stop } else { AppIcon::Play },
                                size: 14,
                            }
                            if process_running() {
                                "Stop"
                            } else {
                                "Start"
                            }
                        }
                    }
                }
            }
            if let Some(active_share) = share() {
                div { class: "flex min-h-10 flex-wrap items-center gap-2 border-b border-amber-500/25 bg-amber-500/8 px-3 py-1.5 text-xs",
                    span { class: "font-semibold text-foreground", "Shared preview" }
                    span { class: "text-muted-foreground",
                        "Anyone with the link can view it until you stop sharing or the preview server stops."
                    }
                    button {
                        class: "ml-auto inline-flex h-7 items-center gap-1.5 rounded-md border border-border bg-background px-2 text-[11px] font-semibold text-foreground hover:bg-accent",
                        r#type: "button",
                        disabled: access_busy,
                        onclick: {
                            let url = active_share.url.clone();
                            move |_| copy_preview_link(url.clone(), toast)
                        },
                        Icon { icon: AppIcon::Copy, size: 13 }
                        "Copy link"
                    }
                    button {
                        class: "inline-flex h-7 items-center justify-center rounded-md px-2 text-[11px] font-semibold text-destructive hover:bg-destructive/10",
                        r#type: "button",
                        disabled: access_busy,
                        onclick: move |_| revoke_sharing(),
                        if sharing() {
                            "Revoking…"
                        } else {
                            "Revoke"
                        }
                    }
                }
            }
            div { class: "relative min-h-0 flex-1 bg-background",
                if initializing {
                    div {
                        class: "flex size-full flex-col items-center justify-center gap-3 p-7 text-center text-sm text-muted-foreground",
                        role: "status",
                        span {
                            class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary",
                            aria_hidden: "true",
                        }
                        if config_loading {
                            "Loading preview settings…"
                        } else if process_status_loading {
                            "Checking preview command…"
                        } else {
                            "Restoring active preview…"
                        }
                    }
                } else if connecting() {
                    div {
                        class: "flex size-full flex-col items-center justify-center gap-3 p-7 text-center text-sm text-muted-foreground",
                        role: "status",
                        span {
                            class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary",
                            aria_hidden: "true",
                        }
                        "Connecting to preview…"
                    }
                } else if let Some(active_lease) = lease() {
                    iframe {
                        key: "{reload_key}",
                        class: "size-full border-0 bg-white",
                        src: active_lease.url,
                        title: "Application preview",
                        "sandbox": "allow-downloads allow-forms allow-modals allow-pointer-lock allow-popups allow-same-origin allow-scripts",
                        allow: "clipboard-read; clipboard-write; fullscreen",
                        referrerpolicy: "no-referrer",
                        onload: move |_| frame_loading.set(false),
                    }
                    if frame_loading() {
                        div {
                            class: "pointer-events-none absolute inset-0 flex items-center justify-center gap-2 bg-background text-sm text-muted-foreground",
                            role: "status",
                            span {
                                class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary",
                                aria_hidden: "true",
                            }
                            "Loading preview page…"
                        }
                    }
                } else {
                    div { class: "flex size-full flex-col items-center justify-center p-7 text-center",
                        div { class: "mb-3.5 grid size-13.5 place-items-center rounded-2xl border border-border bg-card text-muted-foreground",
                            if candidates_loading {
                                span {
                                    class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary",
                                    aria_hidden: "true",
                                }
                            } else {
                                Icon { icon: AppIcon::Eye, size: 24 }
                            }
                        }
                        if candidates_loading {
                            h2 { class: "text-lg font-semibold text-foreground",
                                "Looking for preview servers…"
                            }
                            p { class: "mt-2 max-w-md leading-relaxed text-muted-foreground",
                                "Checking running workspace processes for a web server. You can still enter a port or URL above."
                            }
                        } else {
                            h2 { class: "text-lg font-semibold text-foreground",
                                "Connect a web preview"
                            }
                            p { class: "mt-2 max-w-md leading-relaxed text-muted-foreground",
                                "Configure the target and optional commands in Settings, then use the action button to start or connect."
                            }
                            p { class: "mt-2 text-xs text-muted-foreground",
                                "The port remains private; browser traffic passes through an authenticated gateway."
                            }
                        }
                    }
                }
            }
            if let Some((message, tone)) = toast() {
                Toast {
                    message,
                    tone,
                    on_close: move |()| toast.set(None),
                }
            }
        }
    }
}

fn selected_preview_target(
    url_target: bool,
    port: &str,
    url: &str,
) -> Result<PreviewTarget, String> {
    if url_target {
        let url = url.trim();
        if url.is_empty() {
            Err("Enter an HTTP or HTTPS target URL.".into())
        } else {
            Ok(PreviewTarget::Url { url: url.into() })
        }
    } else {
        let port = port
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|port| *port != 0)
            .ok_or_else(|| "Enter a port between 1 and 65535.".to_owned())?;
        Ok(PreviewTarget::Loopback { port })
    }
}

fn default_preview_action(process_running: bool, start_command: &str) -> PreviewAction {
    if process_running {
        PreviewAction::Stop
    } else if start_command.trim().is_empty() {
        PreviewAction::Connect
    } else {
        PreviewAction::Start
    }
}

fn single_candidate_target(candidates: &[PreviewCandidate]) -> Option<PreviewTarget> {
    (candidates.len() == 1).then(|| PreviewTarget::Loopback {
        port: candidates[0].port,
    })
}

fn suggest_preview_commands(
    commands: &[crate::terminal::api::RunCommand],
) -> (Option<String>, Option<String>) {
    let start = commands
        .iter()
        .filter_map(|command| {
            let value = format!("{} {}", command.label, command.command).to_ascii_lowercase();
            let excluded = [" test", "check", "lint", "build", "format"]
                .iter()
                .any(|term| value.contains(term));
            let score = if excluded {
                0
            } else if value.contains("dx serve") || value.contains("runserver") {
                100
            } else if value.contains(" dev") || value.contains("serve") {
                80
            } else if value.contains(" start") || value.contains(" preview") {
                60
            } else if value.contains("docker compose up") || value.contains(" web") {
                40
            } else {
                0
            };
            (score > 0).then_some((score, command.command.clone()))
        })
        .max_by_key(|(score, _)| *score)
        .map(|(_, command)| command);
    let stop = commands
        .iter()
        .filter_map(|command| {
            let value = format!("{} {}", command.label, command.command).to_ascii_lowercase();
            let score = if value.contains("docker compose down") {
                100
            } else if value.contains(" stop") || value.ends_with("stop") {
                60
            } else if value.contains(" down") || value.ends_with("down") {
                40
            } else {
                0
            };
            (score > 0).then_some((score, command.command.clone()))
        })
        .max_by_key(|(score, _)| *score)
        .map(|(_, command)| command);
    (start, stop)
}

fn apply_preview_target(
    target: &PreviewTarget,
    mut url_target: Signal<bool>,
    mut port: Signal<String>,
    mut url: Signal<String>,
) {
    match target {
        PreviewTarget::Loopback {
            port: selected_port,
        } => {
            url_target.set(false);
            port.set(selected_port.to_string());
        }
        PreviewTarget::Url { url: selected_url } => {
            url_target.set(true);
            url.set(selected_url.clone());
        }
    }
}

fn connect_preview_target(
    workspace_id: String,
    target: PreviewTarget,
    mut state: PreviewConnectionState,
) {
    state.connecting.set(true);
    state.toast.set(None);
    spawn(async move {
        match create_preview_lease(workspace_id, target).await {
            Ok(created) => {
                state.lease.set(Some(created));
                state.share.set(None);
                *state.reload_key.write() += 1;
            }
            Err(problem) => set_preview_error(state.toast, server_error_message(problem)),
        }
        state.connecting.set(false);
    });
}

fn set_preview_error(mut toast: Signal<Option<(String, Tone)>>, message: impl Into<String>) {
    toast.set(Some((message.into(), Tone::Destructive)));
}

fn copy_preview_link(value: String, mut toast: Signal<Option<(String, Tone)>>) {
    spawn(async move {
        match crate::clipboard::copy_text(value).await {
            Ok(()) => toast.set(Some(("Share link copied".into(), Tone::Success))),
            Err(error) => set_preview_error(toast, format!("Could not copy link: {error}")),
        }
    });
}

fn open_preview_window(url: &str) {
    let url = serde_json::to_string(url).expect("preview URLs serialize as JSON strings");
    let _ = document::eval(&format!(
        "globalThis.open({url}, '_blank', 'noopener,noreferrer');"
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_targets_validate_ports_and_trim_urls() {
        assert_eq!(
            selected_preview_target(false, " 5173 ", "").unwrap(),
            PreviewTarget::Loopback { port: 5_173 }
        );
        assert_eq!(
            selected_preview_target(true, "", " https://app.example.test ").unwrap(),
            PreviewTarget::Url {
                url: "https://app.example.test".into(),
            }
        );
        selected_preview_target(false, "0", "").unwrap_err();
        selected_preview_target(false, "65536", "").unwrap_err();
        selected_preview_target(true, "", " ").unwrap_err();
    }

    #[test]
    fn default_action_follows_preview_configuration_and_process_state() {
        assert_eq!(default_preview_action(false, ""), PreviewAction::Connect);
        assert_eq!(
            default_preview_action(false, "npm run dev"),
            PreviewAction::Start
        );
        assert_eq!(default_preview_action(true, ""), PreviewAction::Stop);
    }

    #[test]
    fn preview_command_suggestions_prefer_servers_and_optional_cleanup() {
        let commands = vec![
            crate::terminal::api::RunCommand {
                id: "test".into(),
                label: "cargo · test".into(),
                command: "cargo test".into(),
                custom: false,
            },
            crate::terminal::api::RunCommand {
                id: "dev".into(),
                label: "npm · dev".into(),
                command: "npm run dev".into(),
                custom: false,
            },
            crate::terminal::api::RunCommand {
                id: "down".into(),
                label: "compose · down".into(),
                command: "docker compose down".into(),
                custom: false,
            },
        ];

        assert_eq!(
            suggest_preview_commands(&commands),
            (
                Some("npm run dev".into()),
                Some("docker compose down".into())
            )
        );
    }
}
