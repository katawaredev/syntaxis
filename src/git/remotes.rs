#[allow(unused_imports)] // Dioxus expands the parent glob for RSX hot-reload analysis.
use super::{
    component, dioxus_core, dioxus_elements, dioxus_signals, display_remote_url, rsx, use_signal,
    ActionCallback, AnyStorage, AppIcon, ButtonExtension, DetailsExtension, DialogExtension,
    DropdownMenu, Element, EventHandler, FieldsetExtension, FormExtension,
    GlobalAttributesExtension, HasAttributes, History, Icon, IframeExtension, ImgExtension,
    InputExtension, LinkExtension, MapExtension, MenuButtonTrigger, MenuContent, MetaExtension,
    ObjectExtension, OutputExtension, ParamExtension, Props, ReadableExt, ReadableHashMapExt,
    ReadableHashSetExt, ReadableOptionExt, ReadableResultExt, ReadableStrExt, ReadableVecExt,
    RemoteInfo, SelectExtension, SlotExtension, Storage, StyleExtension, SvgAttributesExtension,
    TextareaExtension, TrackExtension, WritableExt,
};

#[component]
pub(super) fn RemoteManager(
    remotes: Vec<RemoteInfo>,
    upstream: String,
    loading: bool,
    pending: bool,
    on_add: EventHandler<()>,
    on_edit: EventHandler<RemoteInfo>,
    on_remove: EventHandler<RemoteInfo>,
    on_fetch: EventHandler<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut options = use_signal(|| None::<String>);
    if loading {
        return rsx! {
            button {
                class: "inline-flex h-7 shrink-0 items-center rounded-md px-2 text-[11px] text-muted-foreground opacity-60",
                disabled: true,
                "Remotes…"
            }
        };
    }
    if remotes.is_empty() {
        return rsx! {
            button {
                class: "inline-flex h-7 shrink-0 items-center gap-1 rounded-md px-2 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-50",
                disabled: pending,
                onclick: move |_| on_add.call(()),
                Icon { icon: AppIcon::Plus, size: 12 }
                "Add remote"
            }
        };
    }
    let label = if upstream == "No upstream" {
        format!(
            "{} {}",
            remotes.len(),
            if remotes.len() == 1 {
                "remote"
            } else {
                "remotes"
            },
        )
    } else {
        upstream
    };
    rsx! {
        DropdownMenu {
            open: open(),
            on_open_change: move |next: bool| {
                open.set(next);
                if !next {
                    options.set(None);
                }
            },
            div { class: "relative",
                MenuButtonTrigger {
                    class: "touch-target inline-flex h-7 max-w-44 items-center gap-1 rounded-md px-2 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground",
                    label: "Manage Git remotes",
                    title: "Manage Git remotes",
                    on_toggle: move |()| open.toggle(),
                    span { class: "truncate", "{label}" }
                }
                MenuContent { class: "right-0 w-80",
                    div { class: "px-2 py-1 text-[9px] font-medium uppercase tracking-wider text-muted-foreground",
                        "Remotes"
                    }
                    for remote in remotes {
                        div { class: "rounded-md",
                            div { class: "px-1",
                                button {
                                    class: "flex w-full min-w-0 items-center gap-2 rounded-sm px-1 py-1.5 text-left text-muted-foreground hover:bg-accent hover:text-foreground",
                                    "aria-expanded": options().as_deref() == Some(remote.name.as_str()),
                                    "aria-label": "Show actions for remote {remote.name}",
                                    title: "Actions for remote {remote.name}",
                                    onclick: {
                                        let name = remote.name.clone();
                                        move |_| {
                                            if options().as_deref() == Some(name.as_str()) {
                                                options.set(None);
                                            } else {
                                                options.set(Some(name.clone()));
                                            }
                                        }
                                    },
                                    span { class: "min-w-0 flex-1",
                                        strong { class: "block truncate text-xs font-medium text-foreground",
                                            "{remote.name}"
                                        }
                                        small { class: "block truncate text-[9px] text-muted-foreground",
                                            {display_remote_url(&remote.fetch_url)}
                                        }
                                    }
                                    Icon { icon: AppIcon::MoreVertical, size: 14 }
                                }
                            }
                            if options().as_deref() == Some(remote.name.as_str()) {
                                div { class: "mx-1 mb-1 grid gap-0.5 border-l border-border pl-2",
                                    button {
                                        class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground",
                                        disabled: pending,
                                        onclick: {
                                            let name = remote.name.clone();
                                            move |_| {
                                                open.set(false);
                                                options.set(None);
                                                on_fetch.call(name.clone());
                                            }
                                        },
                                        "Fetch"
                                    }
                                    button {
                                        class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground",
                                        disabled: pending,
                                        onclick: {
                                            let remote = remote.clone();
                                            move |_| {
                                                open.set(false);
                                                options.set(None);
                                                on_edit.call(remote.clone());
                                            }
                                        },
                                        "Edit remote"
                                    }
                                    button {
                                        class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-destructive hover:bg-destructive/10",
                                        disabled: pending,
                                        onclick: {
                                            let remote = remote.clone();
                                            move |_| {
                                                open.set(false);
                                                options.set(None);
                                                on_remove.call(remote.clone());
                                            }
                                        },
                                        "Remove remote"
                                    }
                                }
                            }
                        }
                    }
                    hr {}
                    button {
                        class: "flex min-h-8 w-full items-center gap-2 rounded-sm px-2 text-left text-xs text-muted-foreground hover:bg-accent hover:text-foreground",
                        disabled: pending,
                        onclick: move |_| {
                            open.set(false);
                            options.set(None);
                            on_add.call(());
                        },
                        Icon { icon: AppIcon::Plus, size: 13 }
                        "Add remote"
                    }
                }
            }
        }
    }
}
