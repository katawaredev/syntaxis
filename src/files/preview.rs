#[allow(unused_imports)] // Dioxus expands the parent glob for RSX hot-reload analysis.
use super::*;
use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag};

const MARKDOWN_PREVIEW_CSS: Asset = asset!("/assets/files/markdown-preview.css");
const DIFF_TITLEBAR_CLASS: &str = "sticky top-0 z-10 flex min-h-14 min-w-165 items-center justify-between gap-3 border-b border-border bg-background/95 p-3 font-sans backdrop-blur-sm max-md:min-h-13 max-md:px-2.5 max-md:py-2";
const CHECKERBOARD_STYLE: &str = "background-image: linear-gradient(45deg,#aaa 25%,transparent 25%),linear-gradient(-45deg,#aaa 25%,transparent 25%),linear-gradient(45deg,transparent 75%,#aaa 75%),linear-gradient(-45deg,transparent 75%,#aaa 75%); background-size: 20px 20px; background-position: 0 0,0 10px,10px -10px,-10px 0";
const MAX_CSV_PREVIEW_ROWS: usize = 500;
const MAX_CSV_PREVIEW_COLUMNS: usize = 100;

#[component]
pub(super) fn EditorStatus(
    path: Option<String>,
    buffer: Option<ActiveBufferMeta>,
    selection: Signal<EditorSelection>,
) -> Element {
    let selection = selection();
    let state = buffer.as_ref().map_or_else(
        || if path.is_some() { "Saved" } else { "No file" },
        |buffer| match buffer.status {
            BufferStatus::Clean => "Saved",
            BufferStatus::Dirty => "Unsaved",
            BufferStatus::Conflict => "Conflict",
        },
    );
    let indicator_class = match state {
        "Conflict" => "size-2 shrink-0 rounded-full bg-warning",
        "Unsaved" => "size-2 shrink-0 rounded-full bg-amber-400",
        "Saved" => "size-2 shrink-0 rounded-full bg-success",
        _ => "size-2 shrink-0 rounded-full bg-muted-foreground",
    };
    let path = path.unwrap_or_else(|| "No file open".into());
    let language = buffer
        .as_ref()
        .map(|buffer| language_label_for_path(&buffer.path));
    rsx! {
        footer { class: "flex h-6.25 min-h-6.25 items-center justify-between gap-3 border-t border-border bg-background px-2.5 text-[9px] text-muted-foreground",
            div { class: "flex min-w-0 flex-1 items-center gap-2",
                span {
                    class: indicator_class,
                    role: "img",
                    "aria-label": state,
                    title: state,
                }
                span { class: "truncate", "{path}" }
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
                header { class: DIFF_TITLEBAR_CLASS, "Git working tree diff" }
                pre { class: "overflow-auto p-4 text-[11px] leading-5 whitespace-pre",
                    for line in diff.patch.lines() {
                        div { class: if line.starts_with('+') && !line.starts_with("+++") { "text-success bg-success/8" } else if line.starts_with('-') && !line.starts_with("---") { "text-destructive bg-destructive/8" } else { "text-muted-foreground" },
                            "{line}"
                        }
                    }
                }
            }
            section { class: "min-w-0",
                header { class: DIFF_TITLEBAR_CLASS, "Current buffer" }
                pre { class: "overflow-auto p-4 text-[11px] leading-5 whitespace-pre",
                    "{current}"
                }
            }
        }
    }
}

#[component]
pub(super) fn MarkdownPreview(source: String) -> Element {
    let rendered = render_markdown(&source);
    rsx! {
        document::Stylesheet { href: MARKDOWN_PREVIEW_CSS }
        div {
            class: "min-h-full bg-card p-4",
            role: "region",
            "aria-label": "Markdown preview",
            article { class: "markdown-preview", dangerous_inner_html: rendered }
        }
    }
}

#[component]
pub(super) fn CsvPreview(source: String, path: String) -> Element {
    let table = parse_csv(&source);
    rsx! {
        document::Stylesheet { href: MARKDOWN_PREVIEW_CSS }
        div {
            class: "min-h-full bg-card p-4",
            role: "region",
            "aria-label": "CSV preview",
            article { class: "markdown-preview !max-w-none",
                p { class: "text-[10px] font-[750] tracking-[.14em] text-primary",
                    "CSV PREVIEW · {path}"
                }
                match table {
                    Ok(table) if table.headers.is_empty() => rsx! {
                        p { class: "text-muted-foreground", "This CSV file is empty." }
                    },
                    Ok(table) => rsx! {
                        div { class: "overflow-auto",
                            table {
                                thead {
                                    tr {
                                        for header in table.headers {
                                            th { "{header}" }
                                        }
                                    }
                                }
                                tbody {
                                    for row in table.rows {
                                        tr {
                                            for cell in row {
                                                td { "{cell}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if table.truncated {
                            p { class: "text-muted-foreground",
                                "Preview limited to {MAX_CSV_PREVIEW_ROWS} data rows and {MAX_CSV_PREVIEW_COLUMNS} columns."
                            }
                        }
                    },
                    Err(message) => rsx! {
                        p { class: "text-destructive", "Could not parse CSV: {message}" }
                    },
                }
            }
        }
    }
}

struct CsvTable {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    truncated: bool,
}

fn parse_csv(source: &str) -> Result<CsvTable, String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(source.as_bytes());
    let mut records = reader.records();
    let Some(headers) = records.next() else {
        return Ok(CsvTable {
            headers: Vec::new(),
            rows: Vec::new(),
            truncated: false,
        });
    };
    let headers = headers.map_err(|error| error.to_string())?;
    let mut truncated = headers.len() > MAX_CSV_PREVIEW_COLUMNS;
    let headers = headers
        .iter()
        .take(MAX_CSV_PREVIEW_COLUMNS)
        .map(str::to_owned)
        .collect();
    let mut rows = Vec::new();
    for record in records {
        if rows.len() == MAX_CSV_PREVIEW_ROWS {
            truncated = true;
            break;
        }
        let record = record.map_err(|error| error.to_string())?;
        truncated |= record.len() > MAX_CSV_PREVIEW_COLUMNS;
        rows.push(
            record
                .iter()
                .take(MAX_CSV_PREVIEW_COLUMNS)
                .map(str::to_owned)
                .collect(),
        );
    }
    Ok(CsvTable {
        headers,
        rows,
        truncated,
    })
}

pub(crate) fn render_markdown(source: &str) -> String {
    let options = Options::ENABLE_GFM
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS;
    let events = Parser::new_ext(source, options).map(|event| match event {
        // Markdown is workspace-controlled input. Keep source HTML visible as text and only
        // inject the HTML generated by pulldown-cmark itself.
        Event::Html(value) | Event::InlineHtml(value) => Event::Text(value),
        Event::Start(tag) => Event::Start(sanitize_link_destination(tag)),
        event => event,
    });
    let mut rendered = String::new();
    html::push_html(&mut rendered, events);
    rendered
}

fn sanitize_link_destination(tag: Tag<'_>) -> Tag<'_> {
    match tag {
        Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        } => Tag::Link {
            link_type,
            dest_url: safe_destination(dest_url),
            title,
            id,
        },
        Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        } => Tag::Image {
            link_type,
            dest_url: safe_destination(dest_url),
            title,
            id,
        },
        tag => tag,
    }
}

fn safe_destination(destination: CowStr<'_>) -> CowStr<'_> {
    let normalized = destination.trim().to_ascii_lowercase();
    let scheme = normalized.split_once(':').map(|(scheme, _)| scheme);
    if scheme.is_none_or(|scheme| matches!(scheme, "http" | "https" | "mailto")) {
        destination
    } else {
        CowStr::Borrowed("")
    }
}

#[component]
pub(super) fn SafeSvgPreview(source: String, path: String) -> Element {
    let data_url = format!("data:image/svg+xml;base64,{}", BASE64.encode(source));
    rsx! {
        div { class: "flex min-h-full flex-col items-center justify-center gap-4 p-6",
            p { class: "text-[10px] font-[750] tracking-[.14em] text-primary",
                "SVG PREVIEW · {path}"
            }
            div {
                class: "grid aspect-[16/10] w-[min(460px,85%)] place-items-center rounded-lg border border-border bg-[#ccc]",
                style: CHECKERBOARD_STYLE,
                img {
                    class: "block size-full max-h-[70svh] max-w-full object-contain",
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
        div { class: "flex min-h-full flex-col items-center justify-center gap-4 p-6",
            p { class: "text-[10px] font-[750] tracking-[.14em] text-primary",
                "IMAGE PREVIEW · {path} · {size} bytes"
            }
            div {
                class: "grid aspect-[16/10] w-[min(460px,85%)] place-items-center rounded-lg border border-border bg-[#ccc]",
                style: CHECKERBOARD_STYLE,
                img {
                    class: "block size-full max-h-[70svh] max-w-full object-contain",
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
        div { class: "flex min-h-full flex-col items-center justify-center p-6 text-center",
            div { class: "mb-3.5 grid size-13.5 place-items-center rounded-[14px] border border-border bg-card text-[25px] text-muted-foreground",
                "?"
            }
            h2 { class: "text-lg text-[#e2e2e2]", "{title}" }
            p { class: "mt-1.75 max-w-97.5 leading-6 text-muted-foreground", "{reason}" }
            div { class: "mt-4 flex gap-2",
                span { class: "rounded-md border border-border bg-card px-2 py-1.25 text-[10px] text-muted-foreground",
                    "{path}"
                }
                span { class: "rounded-md border border-border bg-card px-2 py-1.25 text-[10px] text-muted-foreground",
                    "{size} bytes"
                }
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
pub(super) fn is_csv(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".csv")
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

#[cfg(test)]
mod tests {
    use super::{parse_csv, render_markdown};

    #[test]
    fn renders_commonmark_and_gfm() {
        let rendered = render_markdown(
            "# Heading\n\n**strong** and ~~removed~~\n\n| A | B |\n| - | - |\n| 1 | 2 |",
        );

        assert!(rendered.contains("<h1>Heading</h1>"));
        assert!(rendered.contains("<strong>strong</strong>"));
        assert!(rendered.contains("<del>removed</del>"));
        assert!(rendered.contains("<table>"));
    }

    #[test]
    fn does_not_inject_source_html_or_unsafe_links() {
        let rendered =
            render_markdown("<script>alert('xss')</script>\n\n[bad](javascript:alert(1))");

        assert!(!rendered.contains("<script>"));
        assert!(rendered.contains("&lt;script&gt;"));
        assert!(!rendered.contains("javascript:"));
    }

    #[test]
    fn csv_uses_first_record_as_headers_and_handles_quoted_cells() {
        let table = parse_csv("name,note\nAda,\"hello, world\"\n").unwrap();

        assert_eq!(table.headers, ["name", "note"]);
        assert_eq!(table.rows, [["Ada", "hello, world"]]);
        assert!(!table.truncated);
    }
}
