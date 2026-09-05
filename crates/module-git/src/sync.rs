use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::DropdownMenuItem;
use syntaxis_git::RemoteInfo;
use syntaxis_ui::prelude::{AppIcon, ComboButton, Icon};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum GitSyncAction {
    AddRemote,
    Publish(String),
    Pull,
    PullRebase,
    Push,
    Fetch,
    MergeUpstream(String),
    AbortMerge,
}

#[component]
pub(super) fn GitSyncButton(
    current_branch: Option<String>,
    upstream: Option<String>,
    remotes: Vec<RemoteInfo>,
    has_upstream: bool,
    ahead: u32,
    behind: u32,
    conflicts: usize,
    pending: bool,
    refreshing: bool,
    on_action: EventHandler<GitSyncAction>,
) -> Element {
    let mut open = use_signal(|| false);
    let preferred_remote = remotes
        .iter()
        .find(|remote| remote.name == "origin")
        .or_else(|| remotes.first())
        .map(|remote| remote.name.clone());
    let diverged = has_upstream && ahead > 0 && behind > 0;
    let primary = if remotes.is_empty() {
        Some(GitSyncAction::AddRemote)
    } else if !has_upstream {
        current_branch
            .as_ref()
            .map_or(Some(GitSyncAction::Fetch), |_| {
                preferred_remote.clone().map(GitSyncAction::Publish)
            })
    } else if diverged {
        None
    } else if behind > 0 {
        Some(GitSyncAction::Pull)
    } else if ahead > 0 {
        Some(GitSyncAction::Push)
    } else {
        Some(GitSyncAction::Fetch)
    };
    let (label, title, icon, count) = if pending {
        (
            "Working",
            "Git operation in progress",
            AppIcon::Refresh,
            None,
        )
    } else {
        match &primary {
            Some(GitSyncAction::AddRemote) => {
                ("Add remote", "Add a Git remote", AppIcon::Plus, None)
            }
            Some(GitSyncAction::Publish(remote)) => (
                "Publish branch",
                "Publish this branch and set its upstream",
                AppIcon::Push,
                Some(remote.clone()),
            ),
            Some(GitSyncAction::Pull) => (
                "Pull",
                "Pull changes from the upstream branch",
                AppIcon::Fetch,
                Some(behind.to_string()),
            ),
            Some(GitSyncAction::Push) => (
                "Push",
                "Push commits to the upstream branch",
                AppIcon::Push,
                Some(ahead.to_string()),
            ),
            Some(GitSyncAction::Fetch) => (
                if refreshing { "Fetching" } else { "Fetch" },
                "Fetch changes from all remotes",
                AppIcon::Refresh,
                None,
            ),
            Some(
                GitSyncAction::PullRebase
                | GitSyncAction::MergeUpstream(_)
                | GitSyncAction::AbortMerge,
            )
            | None => (
                "Diverged",
                "The local and upstream branches have diverged",
                AppIcon::Fetch,
                None,
            ),
        }
    };
    let recommended_class = |action: &GitSyncAction| {
        if primary.as_ref() == Some(action) {
            "!bg-accent !text-foreground"
        } else {
            ""
        }
    };

    rsx! {
        ComboButton {
            label,
            title,
            icon,
            count,
            primary_action: primary.is_some(),
            open: open(),
            disabled: pending,
            menu_label: "Git actions",
            on_click: {
                let primary = primary.clone();
                move |()| {
                    if let Some(action) = primary.clone() {
                        on_action.call(action);
                    }
                }
            },
            on_open_change: move |next| open.set(next),
            if diverged {
                div { class: "px-2 py-1.5 text-[10px] leading-relaxed text-muted-foreground",
                    "Local and upstream commits have diverged. Choose how to bring in the upstream commits."
                }
                hr {}
                DropdownMenuItem::<GitSyncAction> {
                    value: GitSyncAction::PullRebase,
                    index: 0_usize,
                    on_select: move |action| {
                        open.set(false);
                        on_action.call(action);
                    },
                    span { class: "flex min-w-0 items-center gap-2",
                        Icon { icon: AppIcon::Fetch, size: 14 }
                        span { "Pull with rebase…" }
                    }
                }
                if let Some(upstream) = upstream.clone() {
                    DropdownMenuItem::<GitSyncAction> {
                        value: GitSyncAction::MergeUpstream(upstream.clone()),
                        index: 1_usize,
                        on_select: move |action| {
                            open.set(false);
                            on_action.call(action);
                        },
                        span { class: "flex min-w-0 items-center gap-2",
                            Icon { icon: AppIcon::GitBranch, size: 14 }
                            span { class: "truncate", "Merge upstream…" }
                        }
                    }
                    hr {}
                }
            }
            if remotes.is_empty() {
                DropdownMenuItem::<GitSyncAction> {
                    class: recommended_class(&GitSyncAction::AddRemote),
                    value: GitSyncAction::AddRemote,
                    index: 0_usize,
                    on_select: move |action| {
                        open.set(false);
                        on_action.call(action);
                    },
                    span { class: "flex items-center gap-2",
                        Icon { icon: AppIcon::Plus, size: 14 }
                        "Add remote"
                    }
                }
            } else if !has_upstream && current_branch.is_some() {
                for (index, remote) in remotes.iter().enumerate() {
                    {
                        let action = GitSyncAction::Publish(remote.name.clone());
                        rsx! {
                            DropdownMenuItem::<GitSyncAction> {
                                class: recommended_class(&action),
                                value: action,
                                index,
                                on_select: move |action| {
                                    open.set(false);
                                    on_action.call(action);
                                },
                                span { class: "flex min-w-0 items-center gap-2",
                                    Icon { icon: AppIcon::Push, size: 14 }
                                    span { class: "truncate", "Publish to {remote.name}" }
                                }
                            }
                        }
                    }
                }
            } else if !diverged {
                DropdownMenuItem::<GitSyncAction> {
                    class: recommended_class(&GitSyncAction::Pull),
                    value: GitSyncAction::Pull,
                    index: 0_usize,
                    disabled: behind == 0 || diverged,
                    on_select: move |action| {
                        open.set(false);
                        on_action.call(action);
                    },
                    span { class: "flex items-center gap-2",
                        Icon { icon: AppIcon::Fetch, size: 14 }
                        "Pull"
                    }
                    span { class: "tabular-nums text-[10px] text-muted-foreground", "{behind}" }
                }
                DropdownMenuItem::<GitSyncAction> {
                    class: recommended_class(&GitSyncAction::Push),
                    value: GitSyncAction::Push,
                    index: 1_usize,
                    disabled: ahead == 0 || diverged,
                    on_select: move |action| {
                        open.set(false);
                        on_action.call(action);
                    },
                    span { class: "flex items-center gap-2",
                        Icon { icon: AppIcon::Push, size: 14 }
                        "Push"
                    }
                    span { class: "tabular-nums text-[10px] text-muted-foreground", "{ahead}" }
                }
            }
            if !remotes.is_empty() {
                DropdownMenuItem::<GitSyncAction> {
                    class: recommended_class(&GitSyncAction::Fetch),
                    value: GitSyncAction::Fetch,
                    index: remotes.len() + 2,
                    on_select: move |action| {
                        open.set(false);
                        on_action.call(action);
                    },
                    span { class: "flex items-center gap-2",
                        Icon { icon: AppIcon::Refresh, size: 14 }
                        if refreshing {
                            "Fetching…"
                        } else {
                            "Fetch"
                        }
                    }
                }
            }
            if conflicts > 0 {
                hr {}
                DropdownMenuItem::<GitSyncAction> {
                    class: "!text-destructive",
                    value: GitSyncAction::AbortMerge,
                    index: remotes.len() + 3,
                    on_select: move |action| {
                        open.set(false);
                        on_action.call(action);
                    },
                    "Abort merge"
                }
            }
        }
    }
}
