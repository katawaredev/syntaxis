use dioxus::prelude::*;
use syntaxis_git::{BranchRequest, RemoteRequest};
use syntaxis_module_files::FilesPorts;
use syntaxis_ui::prelude::{AppIcon, Icon, Tone};

pub(super) fn diff_line_class(line: &str) -> &'static str {
    if line.starts_with('+') && !line.starts_with("+++") {
        "grid min-h-[23px] grid-cols-[50px_1fr] bg-success/15"
    } else if line.starts_with('-') && !line.starts_with("---") {
        "grid min-h-[23px] grid-cols-[50px_1fr] bg-destructive/15"
    } else if line.starts_with("@@") {
        "grid min-h-[23px] grid-cols-[50px_1fr] bg-secondary text-primary"
    } else {
        "grid min-h-[23px] grid-cols-[50px_1fr]"
    }
}

pub(super) fn short_oid(oid: &str) -> &str {
    oid.get(..7).unwrap_or(oid)
}

pub(super) fn copy_commit_hash(
    value: String,
    files: FilesPorts,
    mut toast: Signal<Option<(String, Tone)>>,
) {
    let clipboard = files.clipboard().cloned();
    spawn(async move {
        let result = match clipboard {
            Some(clipboard) => clipboard.copy_text(&value).await,
            None => {
                toast.set(Some(("Clipboard access is unavailable.".into(), Tone::Warning)));
                return;
            }
        };
        match result {
            Ok(()) => toast.set(Some(("Commit hash copied".into(), Tone::Success))),
            Err(error) => toast.set(Some((
                format!("Could not copy commit hash: {error}"),
                Tone::Destructive,
            ))),
        }
    });
}

#[component]
pub(super) fn RepositoryWelcome(pending: bool, on_initialize: EventHandler<MouseEvent>) -> Element {
    rsx! {
        section { class: "flex size-full items-center justify-center overflow-auto bg-background p-8",
            div { class: "flex max-w-md flex-col items-center text-center",
                div { class: "mb-5 grid size-16 place-items-center rounded-2xl border border-primary/20 bg-primary/10 text-primary shadow-sm",
                    Icon { icon: AppIcon::GitBranch, size: 28 }
                }
                p { class: "mb-1 text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground",
                    "Version control"
                }
                h1 { class: "text-2xl font-semibold tracking-tight text-foreground",
                    "Welcome to Git"
                }
                p { class: "mt-3 max-w-sm text-sm leading-6 text-muted-foreground",
                    "This workspace is not under version control yet. Initialize a repository to track changes, create commits, and connect remotes."
                }
                button {
                    class: "mt-6 inline-flex h-9 items-center gap-2 rounded-md bg-primary px-4 text-sm font-semibold text-primary-foreground shadow-sm transition-colors hover:bg-primary/90 disabled:cursor-wait disabled:opacity-60",
                    disabled: pending,
                    onclick: move |event| on_initialize.call(event),
                    Icon { icon: AppIcon::GitBranch, size: 16 }
                    if pending {
                        "Initializing…"
                    } else {
                        "Initialize repository"
                    }
                }
                p { class: "mt-3 text-[11px] text-muted-foreground/75",
                    "Creates a .git repository with main as the initial branch."
                }
            }
        }
    }
}

pub(super) fn branch_request(name: String, start_point: Option<String>) -> BranchRequest {
    BranchRequest { name, start_point }
}

pub(super) fn remote_request(
    name: String,
    fetch_url: String,
    push_url: Option<String>,
) -> RemoteRequest {
    RemoteRequest {
        name,
        fetch_url,
        push_url,
    }
}

pub(super) fn display_remote_url(url: &str) -> String {
    if let Some((scheme, remainder)) = url.split_once("://") {
        let visible = remainder
            .split_once('@')
            .map_or(remainder, |(_, visible)| visible);
        return format!("{scheme}://{visible}");
    }
    if let Some((credentials, visible)) = url.split_once('@')
        && !credentials.contains('/')
    {
        return visible.to_owned();
    }
    url.to_owned()
}
