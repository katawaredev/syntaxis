use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use serde::{Deserialize, Serialize};
use syntaxis_ui::prelude::{AppIcon, ControlSize, Icon, MenuContent, MenuTrigger, Toast, Tone};

#[cfg(feature = "server")]
pub(crate) mod server;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct PreviewConfig {
    #[serde(default)]
    pub target: Option<PreviewTarget>,
    // Kept for config files written before explicit URL targets were supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
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

#[derive(Clone, Copy)]
struct PreviewConnectionState {
    lease: Signal<Option<PreviewLease>>,
    share: Signal<Option<PreviewShare>>,
    toast: Signal<Option<(String, Tone)>>,
    connecting: Signal<bool>,
    reload_key: Signal<u64>,
}

#[get("/api/previews/{workspace_id}")]
async fn preview_config(workspace_id: String) -> Result<PreviewConfig, ServerFnError> {
    server::preview_config(workspace_id).await
}

#[get("/api/previews/{workspace_id}/candidates")]
async fn preview_candidates(workspace_id: String) -> Result<Vec<PreviewCandidate>, ServerFnError> {
    server::preview_candidates(workspace_id).await
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
            WorkspacePreview { key: "{workspace.id.0}", workspace_id: workspace.id.0 }
        },
        None => rsx! {
            div {
                class: "flex size-full items-center justify-center gap-2 bg-card text-sm text-muted-foreground",
                role: "status",
                span {
                    class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary",
                    aria_hidden: "true",
                }
                "Loading workspace preview…"
            }
        },
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
    let session_workspace_id = workspace_id.clone();
    let session = use_server_future(move || {
        let workspace_id = session_workspace_id.clone();
        async move { resume_preview_session(workspace_id).await }
    })?;
    let mut config_applied = use_signal(|| false);
    let mut session_applied = use_signal(|| false);
    let mut auto_connect_applied = use_signal(|| false);
    let mut configured_target = use_signal(|| None::<PreviewTarget>);
    let mut url_target = use_signal(|| false);
    let mut port = use_signal(String::new);
    let mut url = use_signal(|| "http://".to_owned());
    let mut lease = use_signal(|| None::<PreviewLease>);
    let mut share = use_signal(|| None::<PreviewShare>);
    let mut sharing = use_signal(|| false);
    let mut actions_open = use_signal(|| false);
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
    let detected_target = single_candidate_target(&detected);
    let config_loading = config.state() == UseResourceState::Pending;
    let candidates_loading = candidates.state() == UseResourceState::Pending;
    let restoring = session.state() == UseResourceState::Pending;
    let initializing = config_loading || restoring;
    let controls_busy = connecting() || sharing() || initializing;
    let access_busy = connecting() || sharing();
    let target_missing = if url_target() {
        url().trim().is_empty()
    } else {
        port().trim().is_empty()
    };
    let connect_label = if config_loading {
        "Preparing…"
    } else if restoring {
        "Restoring…"
    } else if connecting() {
        "Connecting…"
    } else {
        "Connect"
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

    rsx! {
        section { class: "flex size-full min-h-0 flex-col bg-card",
            header { class: "flex min-h-12 flex-nowrap items-center gap-2 border-b border-border bg-background px-3 py-2",
                form {
                    class: "flex min-w-0 flex-1 items-center gap-2",
                    onsubmit: move |event| {
                        event.prevent_default();
                        connect();
                    },
                    label {
                        class: "shrink-0 text-xs font-semibold text-muted-foreground max-md:hidden",
                        r#for: "preview-target-kind",
                        "Target"
                    }
                    select {
                        id: "preview-target-kind",
                        class: "h-8 shrink-0 rounded-md border border-input bg-background px-2 text-xs font-semibold text-foreground outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35 max-md:w-18 max-md:px-1.5",
                        "aria-label": "Preview target type",
                        value: if url_target() { "url" } else { "port" },
                        disabled: controls_busy,
                        onchange: move |event| url_target.set(event.value() == "url"),
                        option { value: "port", "Port" }
                        option { value: "url", "URL" }
                    }
                    if url_target() {
                        input {
                            id: "preview-url",
                            class: "h-8 min-w-0 flex-1 rounded-md border border-input bg-background px-2 text-sm text-foreground outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35",
                            r#type: "url",
                            placeholder: "http://service:3000",
                            "aria-label": "Preview target URL",
                            value: url,
                            disabled: controls_busy,
                            oninput: move |event| url.set(event.value()),
                        }
                    } else {
                        input {
                            id: "preview-port",
                            class: "h-8 min-w-0 flex-1 rounded-md border border-input bg-background px-2 text-sm text-foreground outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35 md:w-24 md:flex-none",
                            r#type: "number",
                            list: "preview-detected-ports",
                            min: "1",
                            max: "65535",
                            inputmode: "numeric",
                            "aria-label": "Preview runtime port",
                            value: port,
                            disabled: controls_busy,
                            oninput: move |event| port.set(event.value()),
                        }
                        datalist { id: "preview-detected-ports",
                            for candidate in &detected {
                                option {
                                    value: candidate.port.to_string(),
                                    label: candidate.process.clone(),
                                }
                            }
                        }
                    }
                    button {
                        class: "inline-flex size-8 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground disabled:cursor-wait disabled:opacity-60",
                        r#type: "button",
                        title: "Detect preview servers again",
                        "aria-label": "Detect preview servers again",
                        disabled: candidates_loading,
                        onclick: move |_| candidates.restart(),
                        span { class: if candidates_loading { "animate-spin" } else { "" },
                            Icon { icon: AppIcon::Refresh, size: 14 }
                        }
                    }
                    button {
                        class: "inline-flex h-8 shrink-0 items-center justify-center rounded-md bg-primary px-3 text-xs font-semibold text-primary-foreground transition-colors hover:bg-primary/90 disabled:cursor-wait disabled:opacity-60 max-md:w-8 max-md:px-0",
                        r#type: "submit",
                        title: connect_label,
                        "aria-label": connect_label,
                        disabled: controls_busy || target_missing,
                        span { class: "max-md:hidden", "{connect_label}" }
                        span { class: "md:hidden",
                            if initializing || connecting() {
                                span { class: "animate-spin",
                                    Icon { icon: AppIcon::Refresh, size: 14 }
                                }
                            } else {
                                Icon { icon: AppIcon::Play, size: 14 }
                            }
                        }
                    }
                }
                if let Some(active_lease) = lease() {
                    DropdownMenu {
                        class: "relative shrink-0",
                        open: actions_open(),
                        on_open_change: move |open: bool| actions_open.set(open),
                        MenuTrigger {
                            label: "Preview actions",
                            icon: AppIcon::Menu,
                            size: ControlSize::Small,
                            open: actions_open(),
                            on_toggle: move |()| actions_open.toggle(),
                        }
                        MenuContent { class: "right-0 w-48",
                            DropdownMenuItem::<usize> {
                                value: 0_usize,
                                index: 0_usize,
                                disabled: connecting(),
                                on_select: move |_| *reload_key.write() += 1,
                                span { class: "flex items-center gap-2",
                                    Icon { icon: AppIcon::Refresh, size: 14 }
                                    "Reload preview"
                                }
                            }
                            DropdownMenuItem::<usize> {
                                value: 1_usize,
                                index: 1_usize,
                                on_select: {
                                    let url = active_lease.url.clone();
                                    move |_| open_preview_window(&url)
                                },
                                span { class: "flex items-center gap-2",
                                    Icon { icon: AppIcon::ExternalLink, size: 14 }
                                    "Open in new tab"
                                }
                            }
                            if share().is_none() {
                                DropdownMenuItem::<usize> {
                                    value: 2_usize,
                                    index: 2_usize,
                                    disabled: access_busy,
                                    on_select: move |_| enable_sharing(),
                                    span { class: "flex items-center gap-2",
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
                                "Start the project dev server in Terminal, bind it to 127.0.0.1, then enter its runtime port here."
                            }
                            p { class: "mt-2 text-xs text-muted-foreground",
                                "The port remains private; browser traffic passes through an authenticated Syntaxis gateway."
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

fn server_error_message(error: ServerFnError) -> String {
    match error {
        ServerFnError::ServerError { message, .. } => message,
        other => other.to_string(),
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

fn single_candidate_target(candidates: &[PreviewCandidate]) -> Option<PreviewTarget> {
    (candidates.len() == 1).then(|| PreviewTarget::Loopback {
        port: candidates[0].port,
    })
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
    let _ = eval.send(value);
    spawn(async move {
        match eval.join::<Option<String>>().await {
            Ok(None) => toast.set(Some(("Share link copied".into(), Tone::Success))),
            Ok(Some(message)) => {
                set_preview_error(toast, format!("Could not copy link: {message}"));
            }
            Err(problem) => {
                set_preview_error(toast, format!("Could not copy link: {problem}"));
            }
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
}
