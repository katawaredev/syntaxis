use dioxus::prelude::*;
use dioxus_code_editor::{DiffLayout, UnifiedDiffView};
use syntaxis_editor::language_slug_for_path;
use syntaxis_git::parse_diff_hunks;
use syntaxis_ui::prelude::FileIcon;

use super::changes::hunk_sources;
#[allow(
    unused_imports,
    reason = "Dioxus expands the parent glob for RSX hot-reload analysis"
)]
use super::{
    ActionCallback, AnyStorage, AppError, Button, ButtonExtension, ButtonKind, CommitDetail,
    ControlSize, Element, EventHandler, FieldsetExtension, GlobalAttributesExtension, History,
    InputExtension, LinkExtension, OptgroupExtension, OptionExtension, Props, RawPatch,
    ReadableExt, ReadableHashMapExt, ReadableHashSetExt, ReadableOptionExt, ReadableResultExt,
    ReadableStrExt, ReadableVecExt, Result, SelectExtension, Storage, SvgAttributesExtension,
    TextareaExtension, TrackExtension, WritableExt, component, dioxus_core, dioxus_elements,
    dioxus_signals, rsx,
};

#[component]
pub(super) fn HistoryDetail(
    detail: Option<Result<CommitDetail, AppError>>,
    selected: bool,
    pending: bool,
    can_checkout: bool,
    can_revert: bool,
    on_checkout: EventHandler<String>,
    on_revert: EventHandler<String>,
) -> Element {
    let Some(detail) = detail else {
        return if selected {
            rsx! {
                div {
                    class: "flex h-full min-h-60 items-center justify-center gap-2 p-8 text-center text-sm text-muted-foreground",
                    role: "status",
                    span {
                        class: "size-5 shrink-0 animate-spin rounded-full border-2 border-border border-t-primary",
                        aria_hidden: "true",
                    }
                    "Loading commit details…"
                }
            }
        } else {
            rsx! {
                div { class: "grid h-full min-h-60 place-items-center p-8 text-center text-sm text-muted-foreground",
                    "Select a commit to inspect its Git-generated patch."
                }
            }
        };
    };
    let detail = match detail {
        Ok(detail) => detail,
        Err(error) => {
            return rsx! {
                div { class: "m-4 rounded-md border border-destructive/40 bg-destructive/10 p-3 text-xs text-destructive",
                    "Could not load commit: {error}"
                }
            };
        }
    };
    rsx! {
        div { class: "min-h-full min-w-0",
            header { class: "flex flex-wrap items-start justify-between gap-4 border-b border-border bg-card px-4 py-3",
                div { class: "min-w-0",
                    p { class: "font-mono text-[9px] tracking-wider text-primary",
                        {format!("COMMIT {}", detail.commit.short_oid)}
                    }
                    h2 { class: "mt-1 text-base font-semibold", {detail.commit.subject.clone()} }
                    p { class: "mt-1 text-[11px] text-muted-foreground",
                        {format!("{} <{}>", detail.commit.author_name, detail.commit.author_email)}
                    }
                    div { class: "mt-2 flex gap-3 text-[10px] text-muted-foreground",
                        span { {format!("{} files", detail.files_changed)} }
                        span { class: "text-success", {format!("+{}", detail.additions)} }
                        span { class: "text-destructive", {format!("−{}", detail.deletions)} }
                    }
                }
                div { class: "flex shrink-0 gap-1",
                    if can_checkout { Button {
                        label: "Checkout",
                        kind: ButtonKind::Ghost,
                        size: ControlSize::Small,
                        disabled: pending,
                        onclick: {
                            let oid = detail.commit.oid.clone();
                            move |_| on_checkout.call(oid.clone())
                        },
                    } }
                    if can_revert { Button {
                        label: "Revert",
                        kind: ButtonKind::Ghost,
                        size: ControlSize::Small,
                        disabled: pending,
                        onclick: {
                            let oid = detail.commit.oid.clone();
                            move |_| on_revert.call(oid.clone())
                        },
                    } }
                }
            }
            if detail.patch.is_empty() {
                div { class: "grid min-h-48 place-items-center text-xs text-muted-foreground",
                    "This commit has no textual patch."
                }
            } else {
                div { class: "space-y-3 p-3",
                    for (index, patch) in file_patches(&detail.patch).into_iter().enumerate() {
                        HistoryFileDiff {
                            key: "{detail.commit.oid}-{index}",
                            patch,
                            initially_open: index == 0,
                        }
                    }
                }
            }
        }
    }
}

/// Split only at Git file boundaries, never at diff-like text inside a hunk.
fn file_patches(patch: &str) -> Vec<String> {
    let mut files = Vec::new();
    let mut current = String::new();
    for line in patch.split_inclusive('\n') {
        if (line.starts_with("diff --git ")
            || line.starts_with("diff --cc ")
            || line.starts_with("diff --combined "))
            && !current.is_empty()
        {
            files.push(std::mem::take(&mut current));
        }
        current.push_str(line);
    }
    if !current.is_empty() {
        files.push(current);
    }
    files
}

fn patch_path(patch: &str) -> String {
    // Headers are before the first hunk; added source lines can also start +++.
    let headers = patch
        .lines()
        .take_while(|line| !line.starts_with("@@"))
        .collect::<Vec<_>>();
    for prefix in ["+++ ", "--- "] {
        if let Some(path) = headers.iter().find_map(|line| line.strip_prefix(prefix))
            && path != "/dev/null"
        {
            return path
                .trim_matches('"')
                .strip_prefix("a/")
                .or_else(|| path.trim_matches('"').strip_prefix("b/"))
                .unwrap_or(path)
                .to_owned();
        }
    }
    headers
        .first()
        .and_then(|line| line.strip_prefix("diff --git "))
        .and_then(|line| line.rsplit_once(" b/").map(|(_, path)| path.to_owned()))
        .unwrap_or_else(|| "File changes".to_owned())
}

#[component]
fn HistoryFileDiff(patch: String, initially_open: bool) -> Element {
    let mut open = use_signal(|| initially_open);
    let path = patch_path(&patch);
    let hunks = parse_diff_hunks(&patch).unwrap_or_default();
    let additions: u32 = hunks.iter().map(|hunk| hunk.additions).sum();
    let deletions: u32 = hunks.iter().map(|hunk| hunk.deletions).sum();
    rsx! {
        section { class: "min-w-0 overflow-hidden rounded-md border border-border bg-card",
            button {
                class: "flex w-full min-w-0 items-center gap-2 bg-muted/45 px-3 py-2.5 text-left hover:bg-muted focus-visible:outline-2 focus-visible:outline-primary",
                "aria-expanded": open(),
                onclick: move |_| open.toggle(),
                span { class: "w-3 shrink-0 text-muted-foreground", if open() { "▾" } else { "▸" } }
                FileIcon { path: path.clone(), size: 16 }
                span { class: "min-w-0 flex-1 break-all text-xs font-medium", "{path}" }
                if !hunks.is_empty() {
                    span { class: "shrink-0 text-[10px] text-emerald-400", "+{additions}" }
                    span { class: "shrink-0 text-[10px] text-red-400", "−{deletions}" }
                }
            }
            if open() {
                if hunks.is_empty() {
                    // Preserve binary, rename, mode-only and unusual patch metadata.
                    RawPatch { patch }
                } else {
                    for hunk in hunks {
                        div { key: "{hunk.index}", class: "min-w-0 border-t border-border",
                            div { class: "bg-muted/25 px-3 py-1 font-mono text-[10px] text-muted-foreground", "{hunk.header}" }
                            {
                                let (original, current) = hunk_sources(&hunk.body);
                                rsx! {
                                    UnifiedDiffView {
                                        original,
                                        current,
                                        language: language_slug_for_path(&path).to_owned(),
                                        filename: path.clone(),
                                        collapse_unchanged: false,
                                        layout: DiffLayout::Embedded,
                                        old_line_offset: hunk.old_start.saturating_sub(1),
                                        new_line_offset: hunk.new_start.saturating_sub(1),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{file_patches, patch_path};

    #[test]
    fn separates_files_without_splitting_source_lines() {
        let first = "diff --git a/a.js b/a.js\n--- a/a.js\n+++ b/a.js\n@@ -1 +1 @@\n-old\n+diff --git a/fake b/fake\n";
        let second =
            "diff --git a/b.js b/b.js\n--- a/b.js\n+++ /dev/null\n@@ -1 +0,0 @@\n-deleted\n";
        assert_eq!(
            file_patches(&format!("{first}{second}")),
            vec![first, second]
        );
        assert_eq!(patch_path(first), "a.js");
        assert_eq!(patch_path(second), "b.js");
        assert!(file_patches("").is_empty());
    }

    #[test]
    fn names_added_binary_and_spaced_paths() {
        assert_eq!(
            patch_path("diff --git a/new.js b/new.js\n--- /dev/null\n+++ b/new.js\n"),
            "new.js"
        );
        assert_eq!(
            patch_path("diff --git a/icon.png b/icon.png\nBinary files differ\n"),
            "icon.png"
        );
        assert_eq!(
            patch_path("diff --git a/a b.js b/a b.js\n--- a/a b.js\n+++ b/a b.js\n"),
            "a b.js"
        );
    }
}
