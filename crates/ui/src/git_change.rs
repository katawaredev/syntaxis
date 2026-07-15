use dioxus::prelude::*;
use syntaxis_git::ChangeKind;

#[component]
pub fn GitChangeBadge(kind: Option<ChangeKind>) -> Element {
    let label = match kind {
        Some(ChangeKind::Modified) => "M",
        Some(ChangeKind::TypeChanged) => "T",
        Some(ChangeKind::Added) => "A",
        Some(ChangeKind::Deleted) => "D",
        Some(ChangeKind::Renamed) => "R",
        Some(ChangeKind::Copied) => "C",
        Some(ChangeKind::Untracked) => "U",
        Some(ChangeKind::Unmerged) => "!",
        None => "",
    };
    let tone = match kind {
        Some(ChangeKind::Added | ChangeKind::Untracked) => "border-emerald-400 text-emerald-400",
        Some(ChangeKind::Deleted | ChangeKind::Unmerged) => "border-red-400 text-red-400",
        Some(ChangeKind::Renamed | ChangeKind::Copied) => "border-sky-400 text-sky-400",
        Some(ChangeKind::Modified | ChangeKind::TypeChanged) => "border-amber-400 text-amber-400",
        None => "border-muted-foreground text-muted-foreground",
    };
    rsx! {
        span {
            class: "grid size-4 shrink-0 place-items-center rounded-[5px] border text-[8px] font-bold {tone}",
            "aria-label": "Git status {label}",
            title: "Git status {label}",
            "{label}"
        }
    }
}
