use dioxus::prelude::*;
use syntaxis_ui::prelude::{
    Button, ButtonKind, Checkbox, DialogActions, DialogForm, Modal, SlideToConfirm, Tone,
};

use super::RequestState;
use crate::workspace::{
    client::{clear_mise_tools, clear_runtime_caches, clear_runtime_tools},
    home::HomeDialog,
};

#[component]
pub(super) fn FreeRuntimeSpaceDialog(
    mut dialog: Signal<HomeDialog>,
    on_notice: EventHandler<String>,
) -> Element {
    let mut clear_caches = use_signal(|| true);
    let mut remove_mise = use_signal(|| false);
    let mut remove_runtime_tools = use_signal(|| false);
    let mut removal_confirmed = use_signal(|| false);
    let mut request = use_signal(|| RequestState::Idle);
    let pending = request() == RequestState::Pending;
    let removes_tools = remove_mise() || remove_runtime_tools();
    let nothing_selected = !clear_caches() && !removes_tools;

    rsx! {
        Modal {
            title: "Free up space",
            description: "Choose what Syntaxis should remove. Everything listed here can be downloaded or installed again later.",
            on_close: move |()| {
                if !pending {
                    dialog.set(HomeDialog::None);
                }
            },
            DialogForm {
                CleanupChoice {
                    checked: clear_caches(),
                    disabled: pending,
                    title: "Downloaded packages and build caches".to_owned(),
                    description: "Clears temporary files created by Bun, npm, Cargo, Gradle, and other development tools. Your projects and installed tools stay available."
                        .to_owned(),
                    on_checked_change: move |checked| {
                        clear_caches.set(checked);
                        request.set(RequestState::Idle);
                    },
                }
                CleanupChoice {
                    checked: remove_mise(),
                    disabled: pending,
                    title: "Tools installed with Mise".to_owned(),
                    description: "Removes Node.js, Rust, and other tool versions managed by Mise. Syntaxis can reinstall them from your project setup files."
                        .to_owned(),
                    on_checked_change: move |checked| {
                        remove_mise.set(checked);
                        removal_confirmed.set(false);
                        request.set(RequestState::Idle);
                    },
                }
                CleanupChoice {
                    checked: remove_runtime_tools(),
                    disabled: pending,
                    title: "Other installed developer tools".to_owned(),
                    description: "Removes Bun global packages, Deno commands, and Rustup toolchains. You'll need to reinstall them before using them again."
                        .to_owned(),
                    on_checked_change: move |checked| {
                        remove_runtime_tools.set(checked);
                        removal_confirmed.set(false);
                        request.set(RequestState::Idle);
                    },
                }
                if removes_tools {
                    div { class: "space-y-1.5",
                        SlideToConfirm {
                            disabled: pending,
                            tone: Tone::Destructive,
                            label: "Slide to confirm removing tools".to_owned(),
                            confirmed_label: "Tool removal confirmed".to_owned(),
                            on_confirmed: move |confirmed| removal_confirmed.set(confirmed),
                        }
                        small { class: "block px-1 text-[10px] text-muted-foreground",
                            "Applies to every project in Syntaxis. Your project files, settings, credentials, lockfiles, and AI sessions will not be deleted."
                        }
                    }
                }
                match request() {
                    RequestState::Idle => rsx! {},
                    RequestState::Pending => rsx! {
                        p {
                            class: "flex min-h-9 items-center gap-2 rounded-md border border-primary/30 bg-primary/10 px-2.5 py-2 text-[11px] text-primary",
                            role: "status",
                            span { class: "size-3.5 shrink-0 animate-spin rounded-full border-2 border-primary/30 border-t-primary" }
                            "Freeing up space…"
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
                        label: if pending { "Freeing up space…" } else { "Free up space" },
                        kind: ButtonKind::Danger,
                        disabled: pending || nothing_selected || (removes_tools && !removal_confirmed()),
                        onclick: move |_| {
                            let should_clear_caches = clear_caches();
                            let should_remove_mise = remove_mise();
                            let should_remove_runtime_tools = remove_runtime_tools();
                            request.set(RequestState::Pending);
                            spawn(async move {
                                let result = async {
                                    let mut completed = Vec::new();
                                    if should_clear_caches {
                                        clear_runtime_caches().await?;
                                        completed.push("caches cleared".to_owned());
                                    }
                                    if should_remove_mise {
                                        clear_mise_tools().await?;
                                        completed.push("Mise tools removed".to_owned());
                                    }
                                    if should_remove_runtime_tools {
                                        clear_runtime_tools().await?;
                                        completed.push("other developer tools removed".to_owned());
                                    }
                                    Ok::<String, String>(completed.join(", "))
                                }
                                    .await;
                                match result {
                                    Ok(completed) => {
                                        dialog.set(HomeDialog::None);
                                        on_notice.call(format!("Cleanup complete: {completed}"));
                                    }
                                    Err(_) => {
                                        request
                                            .set(
                                                RequestState::Error(
                                                    "Syntaxis couldn't finish everything you selected. Some items may already have been removed.",
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
fn CleanupChoice(
    checked: bool,
    disabled: bool,
    title: String,
    description: String,
    on_checked_change: EventHandler<bool>,
) -> Element {
    rsx! {
        label { class: "flex items-start gap-2.5 rounded-lg border border-border p-3",
            Checkbox {
                class: "mt-0.5",
                checked,
                disabled,
                aria_label: title.clone(),
                on_checked_change: move |checked| on_checked_change.call(checked),
            }
            span {
                strong { class: "block", {title} }
                small { class: "mt-1 block text-[11px] text-muted-foreground", {description} }
            }
        }
    }
}
