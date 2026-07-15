#[allow(unused_imports)] // Dioxus expands the parent glob for RSX hot-reload analysis.
use super::*;

#[component]
pub(super) fn EditorStatus(
    buffer: Option<ActiveBufferMeta>,
    selection: Signal<EditorSelection>,
) -> Element {
    let selection = selection();
    let state = buffer
        .as_ref()
        .map_or("No buffer", |buffer| match buffer.status {
            BufferStatus::Clean => "Saved",
            BufferStatus::Dirty => "Unsaved",
            BufferStatus::Conflict => "Conflict",
        });
    let language = buffer
        .as_ref()
        .map(|buffer| language_label_for_path(&buffer.path));
    rsx! {
        footer { class: "flex h-6.25 min-h-6.25 items-center justify-between border-t border-border bg-background px-2.5 text-[9px] text-muted-foreground",
            div { class: "flex items-center gap-2",
                span { class: if state == "Conflict" { "size-2 rounded-full bg-warning" } else { "size-2 rounded-full bg-success" } }
                "{state}"
            }
            div { class: "flex items-center gap-3",
                if let Some(language) = language {
                    span { "Ln {selection.line.max(1)}, Col {selection.column.max(1)}" }
                    if selection.selection_count > 1 {
                        span { "{selection.selection_count} cursors" }
                    }
                    span { "UTF-8" }
                    span { "{language}" }
                }
            }
        }
    }
}

#[component]
pub(super) fn EmptyEditor(loading: Option<String>) -> Element {
    rsx! {
        div { class: "flex size-full flex-col items-center justify-center p-7 text-center",
            h2 { class: "text-lg text-foreground",
                if let Some(label) = loading.as_ref() {
                    "{label}"
                } else {
                    "No open files"
                }
            }
            p { class: "mt-1.75 max-w-97.5 text-muted-foreground",
                if loading.is_some() {
                    "Reading the remote workspace."
                } else {
                    "Choose a file from the explorer to open it."
                }
            }
        }
    }
}

#[component]
pub(super) fn DiffEditor(diff: UnifiedDiff, current: String) -> Element {
    rsx! {
        div { class: "grid min-h-full grid-cols-2 max-lg:grid-cols-1",
            section { class: "min-w-0 border-r border-border max-lg:border-r-0 max-lg:border-b",
                header { class: "diff-titlebar", "Git working tree diff" }
                pre { class: "overflow-auto p-4 text-[11px] leading-5 whitespace-pre",
                    for line in diff.patch.lines() {
                        div { class: if line.starts_with('+') && !line.starts_with("+++") { "text-success bg-success/8" } else if line.starts_with('-') && !line.starts_with("---") { "text-destructive bg-destructive/8" } else { "text-muted-foreground" },
                            "{line}"
                        }
                    }
                }
            }
            section { class: "min-w-0",
                header { class: "diff-titlebar", "Current buffer" }
                pre { class: "overflow-auto p-4 text-[11px] leading-5 whitespace-pre",
                    "{current}"
                }
            }
        }
    }
}

#[component]
pub(super) fn MarkdownPreview(source: String) -> Element {
    let lines = source.lines().map(str::to_owned).collect::<Vec<_>>();
    rsx! {
        article { class: "preview markdown-preview",
            p { class: "preview-label", "MARKDOWN PREVIEW" }
            for line in lines {
                if let Some(text) = line.strip_prefix("# ") {
                    h1 { "{text}" }
                } else if let Some(text) = line.strip_prefix("## ") {
                    h2 { "{text}" }
                } else if let Some(text) = line.strip_prefix("### ") {
                    h3 { "{text}" }
                } else if let Some(text) = line.strip_prefix("- ") {
                    ul {
                        li { "{text}" }
                    }
                } else if line.starts_with("```") {
                    hr {}
                } else if line.is_empty() {
                    br {}
                } else {
                    p { "{line}" }
                }
            }
        }
    }
}

#[component]
pub(super) fn SafeSvgPreview(source: String, path: String) -> Element {
    let data_url = format!("data:image/svg+xml;base64,{}", BASE64.encode(source));
    rsx! {
        div { class: "preview media-preview",
            p { class: "preview-label", "SVG PREVIEW · {path}" }
            div { class: "checkerboard",
                img {
                    class: "max-h-[70svh] max-w-full",
                    src: data_url,
                    alt: "Preview of {path}",
                }
            }
        }
    }
}

#[component]
pub(super) fn ImagePreview(path: String, data_url: String, size: u64) -> Element {
    rsx! {
        div { class: "preview media-preview",
            p { class: "preview-label", "IMAGE PREVIEW · {path} · {size} bytes" }
            div { class: "checkerboard",
                img {
                    class: "max-h-[70svh] max-w-full object-contain",
                    src: data_url,
                    alt: "Preview of {path}",
                }
            }
        }
    }
}

#[component]
pub(super) fn UnsupportedPreview(
    path: String,
    size: u64,
    title: String,
    reason: String,
) -> Element {
    rsx! {
        div { class: "preview unsupported-preview",
            div { class: "empty-icon", "?" }
            h2 { "{title}" }
            p { "{reason}" }
            div { class: "file-facts",
                span { "{path}" }
                span { "{size} bytes" }
            }
        }
    }
}

pub(super) fn image_mime(path: &str) -> Option<&'static str> {
    match path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("webp") => Some("image/webp"),
        Some("bmp") => Some("image/bmp"),
        Some("ico") => Some("image/x-icon"),
        _ => None,
    }
}
pub(super) fn is_markdown(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".md") || path.to_ascii_lowercase().ends_with(".markdown")
}
pub(super) fn is_svg(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".svg")
}
pub(super) fn file_label(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
pub(super) fn file_glyph(path: &str) -> &'static str {
    match path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("rs") => "R",
        Some("md" | "markdown") => "M",
        _ => "·",
    }
}
