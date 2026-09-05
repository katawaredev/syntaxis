use dioxus::prelude::*;
use syntaxis_ui::prelude::{AppIcon, Button, ButtonKind, Icon, InteractivePopover, Tone};

use super::{api, management::ManagementSidebarButton};

const MAX_INSTRUCTIONS_BYTES: usize = 512 * 1024;

#[component]
pub(super) fn GlobalInstructionsPanel(
    workspace_id: String,
    mut toast: Signal<Option<(String, Tone)>>,
    sidebar_open: bool,
    on_toggle_sidebar: EventHandler<()>,
    on_open_sidebar: EventHandler<()>,
) -> Element {
    let load_workspace = workspace_id.clone();
    let instructions = use_resource(move || {
        let workspace_id = load_workspace.clone();
        async move { api::pi_global_instructions(workspace_id).await }
    });
    let mut draft = use_signal(String::new);
    let mut saved = use_signal(|| None::<String>);
    let mut saving = use_signal(|| false);

    use_effect(move || {
        if saved().is_none()
            && let Some(Ok(content)) = instructions()
        {
            draft.set(content.clone());
            saved.set(Some(content));
        }
    });

    let load_error = instructions()
        .and_then(Result::err)
        .map(|error| error.to_string());
    let loaded = saved().is_some();
    let changed = saved().is_some_and(|content| content != draft());
    let draft_bytes = draft().len();
    let too_large = draft_bytes > MAX_INSTRUCTIONS_BYTES;

    rsx! {
        header { class: "flex min-h-12 items-center gap-3 border-b border-border bg-background px-4",
            ManagementSidebarButton {
                sidebar_open,
                on_toggle_sidebar,
                on_open_sidebar,
            }
            div { class: "min-w-0 flex-1",
                strong { class: "block text-xs", "Global instructions" }
                small { class: "text-[9px] text-muted-foreground", "Instance-wide Pi policy" }
            }
        }
        div { class: "min-h-0 flex-1 overflow-y-auto p-5",
            div { class: "mx-auto max-w-3xl max-md:max-w-none",
                if let Some(error) = load_error {
                    p { class: "rounded-lg bg-destructive/10 p-3 text-xs text-destructive",
                        "{error}"
                    }
                } else if !loaded {
                    p { class: "text-xs text-muted-foreground", "Loading global instructions…" }
                } else {
                    section { class: "overflow-hidden rounded-xl border border-input bg-background shadow-xs transition-[border,box-shadow] focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/20",
                        div { class: "flex min-h-12 items-center gap-3 border-b border-border px-3.5 py-2",
                            div { class: "min-w-0 flex-1",
                                label {
                                    class: "block text-xs font-semibold text-foreground/85",
                                    r#for: "pi-global-instructions",
                                    "AGENTS.md"
                                }
                                p { class: "mt-0.5 text-[10px] text-muted-foreground",
                                    "Loaded automatically for every workspace"
                                }
                            }
                            div { class: "flex shrink-0 items-center gap-0.5",
                                InstructionNote {
                                    id: "global-instructions-application-note",
                                    icon: AppIcon::Info,
                                    label: "How global instructions apply",
                                    title: "Shared by every workspace",
                                    message: "New chats use saved changes. Active chats retain their current instructions until reloaded or replaced.",
                                    warning: false,
                                }
                                InstructionNote {
                                    id: "global-instructions-enforcement-note",
                                    icon: AppIcon::ShieldAlert,
                                    label: "Instruction enforcement limitations",
                                    title: "Guidance, not enforcement",
                                    message: "Use deployment resource limits or a blocking Pi extension for rules that must be technically enforced.",
                                    warning: true,
                                }
                            }
                        }
                        textarea {
                            id: "pi-global-instructions",
                            class: "touch-input block min-h-72 w-full resize-none border-0 bg-transparent px-3.5 py-3 font-mono text-xs leading-relaxed text-foreground outline-none placeholder:text-muted-foreground/65 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50",
                            value: draft(),
                            rows: 18,
                            disabled: saving(),
                            placeholder: "Add instance-wide operating constraints and preferences for Pi…",
                            aria_invalid: too_large,
                            aria_describedby: "pi-global-instructions-status",
                            oninput: move |event: FormEvent| draft.set(event.value()),
                        }
                        div { class: "flex min-h-13 flex-wrap items-center justify-between gap-3 border-t border-border px-3.5 py-2",
                            div {
                                small {
                                    id: "pi-global-instructions-status",
                                    class: if too_large { "block text-[10px] text-destructive" } else { "block text-[10px] text-muted-foreground" },
                                    if too_large {
                                        "Instructions exceed the 512 KiB limit"
                                    } else {
                                        "{draft_bytes} bytes · 512 KiB maximum"
                                    }
                                }
                                small { class: "mt-0.5 block text-[9px] text-muted-foreground/75",
                                    "Saving an empty file clears the instructions."
                                }
                            }
                            Button {
                                label: if saving() { "Saving…" } else { "Save" },
                                kind: ButtonKind::Primary,
                                disabled: saving() || !changed || too_large,
                                onclick: move |_| {
                                    saving.set(true);
                                    let workspace_id = workspace_id.clone();
                                    let content = draft();
                                    spawn(async move {
                                        match api::save_pi_global_instructions(workspace_id, content).await {
                                            Ok(saved_content) => {
                                                saved.set(Some(saved_content.clone()));
                                                draft.set(saved_content);
                                            }
                                            Err(error) => {
                                                toast.set(Some((error.to_string(), Tone::Destructive)));
                                            }
                                        }
                                        saving.set(false);
                                    });
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn InstructionNote(
    id: String,
    icon: AppIcon,
    label: &'static str,
    title: &'static str,
    message: &'static str,
    warning: bool,
) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        InteractivePopover {
            id,
            label,
            title: label,
            open: open(),
            on_open_change: move |next| open.set(next),
            trigger_class: if open() { "grid size-7 place-items-center rounded-md bg-accent text-foreground" } else if warning { "grid size-7 place-items-center rounded-md text-warning/80 hover:bg-warning/10 hover:text-warning" } else { "grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" },
            content_class: "absolute top-[calc(100%+4px)] right-0 z-80 w-[min(300px,calc(100vw-1rem))] rounded-xl border border-border bg-popover p-3 shadow-2xl",
            trigger: rsx! {
                Icon { icon, size: 14 }
            },
            strong { class: if warning { "block text-xs text-warning" } else { "block text-xs" },
                "{title}"
            }
            p { class: "mt-1 text-[10px] leading-relaxed text-muted-foreground", "{message}" }
        }
    }
}
