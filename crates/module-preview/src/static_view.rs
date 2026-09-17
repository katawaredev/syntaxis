use dioxus::prelude::*;
use syntaxis_app_contracts::{WorkspaceEventBus, WorkspaceEventDelivery, WorkspaceEventKind};
use syntaxis_module_files::FilesUiState;
use syntaxis_ui::prelude::{AppIcon, Button, ButtonKind, Icon, Toast, Tone};
use syntaxis_workspace::{RelativePath, WorkspaceRecord};

use crate::process_view::ProcessPreviewView;
use crate::{PreviewLease, PreviewPorts, PreviewTarget};

#[component]
pub fn PreviewView(workspace: WorkspaceRecord) -> Element {
    let ports = use_context::<PreviewPorts>();
    if ports.process().is_some() {
        return rsx! { ProcessPreviewView { workspace } };
    }
    rsx! { StaticPreviewView { workspace } }
}

#[component]
fn StaticPreviewView(workspace: WorkspaceRecord) -> Element {
    let ports = use_context::<PreviewPorts>();
    let files = use_context::<FilesUiState>();
    let events = use_context::<WorkspaceEventBus>();
    let active_path = files.active_path().filter(|path| is_html(path));
    let mut requested_path = use_signal(|| active_path.clone());
    let mut lease = use_signal(|| None::<PreviewLease>);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);
    let preview = ports.preview().cloned();

    use_effect(move || {
        if let Some(path) = active_path.clone()
            && requested_path.peek().as_ref() != Some(&path)
        {
            requested_path.set(Some(path));
        }
    });

    let event_workspace = workspace.clone();
    let dependency_lease = lease;
    use_resource(move || {
        let bus = events.clone();
        let workspace_id = event_workspace.id.clone();
        async move {
            let mut subscription = bus.subscribe();
            loop {
                match subscription.next().await {
                    WorkspaceEventDelivery::Event(event) if event.workspace_id == workspace_id => {
                        let relevant = match event.kind {
                            WorkspaceEventKind::ResyncRequired => true,
                            WorkspaceEventKind::Changes { changes } => {
                                dependency_lease.peek().as_ref().is_some_and(|lease| {
                                    changes.iter().any(|change| {
                                        lease.dependencies.iter().any(|path| path == &change.path)
                                    })
                                })
                            }
                        };
                        if relevant {
                            *refresh.write() += 1;
                        }
                    }
                    WorkspaceEventDelivery::Event(_) => {}
                    WorkspaceEventDelivery::ResyncRequired { .. } => *refresh.write() += 1,
                    WorkspaceEventDelivery::Closed => break,
                }
            }
        }
    });

    let open_workspace = workspace.clone();
    use_effect(move || {
        let generation = refresh();
        let path = requested_path();
        let preview = preview.clone();
        let workspace = open_workspace.clone();
        spawn(async move {
            let Some(path) = path else {
                if let (Some(preview), Some(previous)) = (preview, lease()) {
                    let _ = preview.close(&workspace, previous).await;
                    lease.set(None);
                }
                return;
            };
            let Some(preview) = preview else {
                error.set(Some(
                    "Static preview is unavailable in this runtime.".into(),
                ));
                return;
            };
            loading.set(true);
            error.set(None);
            let document_path = match RelativePath::try_from(path) {
                Ok(path) => PreviewTarget::StaticDocument { path },
                Err(problem) => {
                    error.set(Some(problem.message));
                    loading.set(false);
                    return;
                }
            };
            let PreviewTarget::StaticDocument { path: target_path } = &document_path else {
                unreachable!("the static preview constructs only static document targets")
            };
            let result = if let Some(previous) = lease() {
                if previous.dependencies.first() == Some(target_path) {
                    preview.refresh(&workspace, &previous).await
                } else {
                    let _ = preview.close(&workspace, previous).await;
                    preview.open(&workspace, document_path).await
                }
            } else {
                preview.open(&workspace, document_path).await
            };
            if refresh() != generation {
                if let Ok(stale) = result {
                    let _ = preview.close(&workspace, stale).await;
                }
                return;
            }
            match result {
                Ok(next) => lease.set(Some(next)),
                Err(problem) => error.set(Some(problem.message)),
            }
            loading.set(false);
        });
    });

    let drop_workspace = workspace.clone();
    let drop_preview = ports.preview().cloned();
    use_drop(move || {
        if let (Some(preview), Some(active)) = (drop_preview, lease.peek().clone()) {
            spawn(async move {
                let _ = preview.close(&drop_workspace, active).await;
            });
        }
    });

    rsx! {
        section { class: "flex size-full min-h-0 flex-col bg-card", "aria-label": "HTML preview",
            header { class: "flex min-h-12 items-center gap-2 border-b border-border bg-background px-3 py-2",
                span { class: "grid size-7 place-items-center rounded-md bg-primary/10 text-primary",
                    Icon { icon: AppIcon::Eye, size: 14 }
                }
                div { class: "min-w-0 flex-1",
                    strong { class: "block truncate text-xs font-semibold", "Preview" }
                    small { class: "block truncate text-[9px] text-muted-foreground",
                        if let Some(path) = requested_path() { "Sandboxed static document · {path}" } else { "Open an HTML file in Files" }
                    }
                }
                Button {
                    label: if loading() { "Reloading…" } else { "Reload" },
                    kind: ButtonKind::Ghost,
                    disabled: loading() || requested_path().is_none(),
                    onclick: move |_| *refresh.write() += 1,
                }
            }
            if let Some(active) = lease() {
                iframe {
                    key: "{active.id}-{refresh}",
                    class: "min-h-0 w-full flex-1 border-0 bg-white",
                    title: "Workspace preview",
                    "sandbox": "",
                    referrerpolicy: "no-referrer",
                    src: active.url,
                }
            } else if loading() {
                div { class: "grid flex-1 place-items-center text-sm text-muted-foreground", "Preparing preview…" }
            } else {
                div { class: "grid flex-1 place-items-center p-8 text-center text-sm text-muted-foreground",
                    "Open an HTML file in Files, then return to Preview."
                }
            }
        }
        if let Some(message) = error() {
            Toast { message, tone: Tone::Destructive, on_close: move |()| error.set(None) }
        }
    }
}

fn is_html(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, extension)| {
        matches!(extension.to_ascii_lowercase().as_str(), "html" | "htm")
    })
}
