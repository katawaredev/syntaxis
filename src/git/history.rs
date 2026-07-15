#[allow(unused_imports)] // Dioxus expands the parent glob for RSX hot-reload analysis.
use super::{
    component, dioxus_core, dioxus_elements, dioxus_signals, rsx, ActionCallback, AnyStorage,
    Button, ButtonExtension, ButtonKind, CommitDetail, ControlSize, Element, EventHandler,
    FieldsetExtension, GlobalAttributesExtension, History, InputExtension, LinkExtension,
    OptgroupExtension, OptionExtension, Props, RawPatch, ReadableExt, ReadableHashMapExt,
    ReadableHashSetExt, ReadableOptionExt, ReadableResultExt, ReadableStrExt, ReadableVecExt,
    Result, SelectExtension, ServerFnError, Storage, SvgAttributesExtension, TextareaExtension,
    TrackExtension, WritableExt,
};

#[component]
pub(super) fn HistoryDetail(
    detail: Option<Result<CommitDetail, ServerFnError>>,
    pending: bool,
    on_checkout: EventHandler<String>,
    on_revert: EventHandler<String>,
) -> Element {
    let Some(detail) = detail else {
        return rsx! {
            div { class: "grid h-full min-h-60 place-items-center p-8 text-center text-sm text-muted-foreground",
                "Select a commit to inspect its Git-generated patch."
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
            }
        }
    };
    rsx! {
        div { class: "min-h-full min-w-165",
            header { class: "flex items-start justify-between gap-4 border-b border-border bg-card px-4 py-3",
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
                    Button {
                        label: "Checkout",
                        kind: ButtonKind::Ghost,
                        size: ControlSize::Small,
                        disabled: pending,
                        onclick: {
                            let oid = detail.commit.oid.clone();
                            move |_| on_checkout.call(oid.clone())
                        },
                    }
                    Button {
                        label: "Revert",
                        kind: ButtonKind::Ghost,
                        size: ControlSize::Small,
                        disabled: pending,
                        onclick: {
                            let oid = detail.commit.oid.clone();
                            move |_| on_revert.call(oid.clone())
                        },
                    }
                }
            }
            if detail.patch.is_empty() {
                div { class: "grid min-h-48 place-items-center text-xs text-muted-foreground",
                    "This commit has no textual patch."
                }
            } else {
                RawPatch { patch: detail.patch }
            }
        }
    }
}
