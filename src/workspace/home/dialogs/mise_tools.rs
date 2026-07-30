use dioxus::prelude::*;
use syntaxis_ui::prelude::{Button, ButtonKind, DialogActions, DialogForm, Modal};

use super::RequestState;
use crate::workspace::{
    client::{clear_mise_tools, clear_runtime_caches},
    home::HomeDialog,
};

#[component]
pub(super) fn ClearRuntimeCachesDialog(
    mut dialog: Signal<HomeDialog>,
    on_notice: EventHandler<String>,
) -> Element {
    let mut request = use_signal(|| RequestState::Idle);
    let pending = request() == RequestState::Pending;

    rsx! {
        Modal {
            title: "Clear all runtime caches?",
            description: "Permanently delete package downloads, extracted sources, indexes, build caches, and installer leftovers from the runtime home.",
            on_close: move |()| {
                if !pending {
                    dialog.set(HomeDialog::None);
                }
            },
            DialogForm {
                p { class: "rounded-md border border-destructive/35 bg-destructive/10 px-2.5 py-2 text-xs leading-relaxed text-destructive",
                    "This affects every project. Stop active installs and builds first. Installed tools, project files, lockfiles, credentials, and Pi sessions are preserved; future work may need to download dependencies again."
                }
                match request() {
                    RequestState::Idle => rsx! {},
                    RequestState::Pending => rsx! {
                        p {
                            class: "flex min-h-9 items-center gap-2 rounded-md border border-primary/30 bg-primary/10 px-2.5 py-2 text-[11px] text-primary",
                            role: "status",
                            span { class: "size-3.5 shrink-0 animate-spin rounded-full border-2 border-primary/30 border-t-primary" }
                            "Permanently clearing runtime caches…"
                        }
                    },
                    RequestState::Error(message) => rsx! {
                        p {
                            class: "rounded-md border border-destructive/35 bg-destructive/10 px-2.5 py-2 text-xs leading-relaxed text-destructive",
                            role: "alert",
                            {message}
                        }
                    },
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| dialog.set(HomeDialog::None),
                    }
                    Button {
                        label: if pending { "Clearing…" } else { "Clear all caches" },
                        kind: ButtonKind::Danger,
                        disabled: pending,
                        onclick: move |_| {
                            request.set(RequestState::Pending);
                            spawn(async move {
                                match clear_runtime_caches().await {
                                    Ok(count) => {
                                        dialog.set(HomeDialog::None);
                                        let message = if count == 0 {
                                            "Runtime caches were already empty".into()
                                        } else {
                                            format!("Cleared {count} runtime cache locations")
                                        };
                                        on_notice.call(message);
                                    }
                                    Err(_) => {
                                        request
                                            .set(
                                                RequestState::Error(
                                                    "The runtime could not clear every cache location.",
                                                ),
                                            );
                                    }
                                }
                            });
                        },
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn ClearMiseToolsDialog(
    mut dialog: Signal<HomeDialog>,
    on_notice: EventHandler<String>,
) -> Element {
    let mut request = use_signal(|| RequestState::Idle);
    let pending = request() == RequestState::Pending;

    rsx! {
        Modal {
            title: "Remove all mise tools",
            description: "Delete every tool version installed by mise for this runtime user.",
            on_close: move |()| {
                if !pending {
                    dialog.set(HomeDialog::None);
                }
            },
            DialogForm {
                p { class: "rounded-md border border-destructive/35 bg-destructive/10 px-2.5 py-2 text-xs leading-relaxed text-destructive",
                    "This affects every project in Syntaxis. Project configuration is preserved, so you can bootstrap again later."
                }
                match request() {
                    RequestState::Idle => rsx! {},
                    RequestState::Pending => rsx! {
                        p {
                            class: "flex min-h-9 items-center gap-2 rounded-md border border-primary/30 bg-primary/10 px-2.5 py-2 text-[11px] text-primary",
                            role: "status",
                            span { class: "size-3.5 shrink-0 animate-spin rounded-full border-2 border-primary/30 border-t-primary" }
                            "Removing installed tools and clearing the mise cache…"
                        }
                    },
                    RequestState::Error(message) => rsx! {
                        p {
                            class: "rounded-md border border-destructive/35 bg-destructive/10 px-2.5 py-2 text-xs leading-relaxed text-destructive",
                            role: "alert",
                            {message}
                        }
                    },
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| dialog.set(HomeDialog::None),
                    }
                    Button {
                        label: if pending { "Removing…" } else { "Remove all tools" },
                        kind: ButtonKind::Danger,
                        disabled: pending,
                        onclick: move |_| {
                            request.set(RequestState::Pending);
                            spawn(async move {
                                match clear_mise_tools().await {
                                    Ok(()) => {
                                        dialog.set(HomeDialog::None);
                                        on_notice.call("All mise-installed tools were removed".into());
                                    }
                                    Err(_) => {
                                        request
                                            .set(
                                                RequestState::Error(
                                                    "The runtime could not remove the mise tool installations.",
                                                ),
                                            );
                                    }
                                }
                            });
                        },
                    }
                }
            }
        }
    }
}
