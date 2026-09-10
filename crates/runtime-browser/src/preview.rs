use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use js_sys::{Array, Uint8Array};
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, RetryAdvice};
use syntaxis_module_preview::{
    PreviewCandidate, PreviewLease, PreviewPort, PreviewSession, PreviewTarget,
};
use syntaxis_workspace::{RelativePath, WorkspaceFiles, WorkspaceRecord};
use web_sys::{Blob, BlobPropertyBag, Url};

const MAX_HTML_BYTES: u64 = 2 * 1024 * 1024;
const MAX_ASSET_BYTES: u64 = 1024 * 1024;
const MAX_TOTAL_ASSET_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ASSETS: usize = 256;
const BLOCKED_REFERENCE: &str = "data:,";
const PREVIEW_CSP_META: &str = r#"<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src data: 'unsafe-inline'; font-src data:; media-src data:; object-src 'none'; frame-src 'none'; connect-src 'none'; base-uri 'none'; form-action 'none'">"#;

#[derive(Clone)]
pub struct BrowserPreviewAdapter<F> {
    files: F,
    leases: Arc<Mutex<HashMap<String, LeaseRecord>>>,
    next_id: Arc<AtomicU64>,
}

#[derive(Clone)]
struct LeaseRecord {
    path: RelativePath,
    urls: Vec<String>,
}

impl<F> BrowserPreviewAdapter<F> {
    pub fn new(files: F) -> Self {
        Self {
            files,
            leases: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    fn remove_lease(&self, id: &str) {
        let record = self
            .leases
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(id);
        if let Some(record) = record {
            revoke_urls(record.urls);
        }
    }
}

#[async_trait(?Send)]
impl<F> PreviewPort for BrowserPreviewAdapter<F>
where
    F: WorkspaceFiles + Clone + Send + Sync + 'static,
{
    async fn candidates(
        &self,
        _workspace: &WorkspaceRecord,
    ) -> Result<Vec<PreviewCandidate>, AppError> {
        Ok(Vec::new())
    }

    async fn open(
        &self,
        workspace: &WorkspaceRecord,
        target: PreviewTarget,
    ) -> Result<PreviewLease, AppError> {
        let PreviewTarget::StaticDocument { path } = target else {
            return Err(AppError::unsupported(
                "Browser Preview opens static HTML documents only.",
                ErrorSource::Preview,
            ));
        };
        let prepared = prepare_static_preview(&self.files, workspace, &path).await?;
        let id = format!(
            "browser-preview-{}",
            self.next_id.fetch_add(1, Ordering::Relaxed)
        );
        let lease = PreviewLease::ephemeral(
            id.clone(),
            prepared.document_url.clone(),
            prepared.dependencies.clone(),
        );
        self.leases
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                id,
                LeaseRecord {
                    path,
                    urls: prepared.urls,
                },
            );
        Ok(lease)
    }

    async fn resume(
        &self,
        _workspace: &WorkspaceRecord,
    ) -> Result<Option<PreviewSession>, AppError> {
        Ok(None)
    }

    async fn refresh(
        &self,
        workspace: &WorkspaceRecord,
        lease: &PreviewLease,
    ) -> Result<PreviewLease, AppError> {
        let path = self
            .leases
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&lease.id)
            .map(|record| record.path.clone())
            .ok_or_else(|| {
                AppError::new(
                    AppErrorCode::NotFound,
                    "The browser preview lease has expired.",
                    RetryAdvice::AfterUserAction,
                    ErrorSource::Preview,
                )
            })?;
        self.remove_lease(&lease.id);
        self.open(workspace, PreviewTarget::StaticDocument { path })
            .await
    }

    async fn close(
        &self,
        _workspace: &WorkspaceRecord,
        lease: PreviewLease,
    ) -> Result<(), AppError> {
        self.remove_lease(&lease.id);
        Ok(())
    }
}

struct PreparedPreview {
    document_url: String,
    dependencies: Vec<RelativePath>,
    urls: Vec<String>,
}

async fn prepare_static_preview<F>(
    files: &F,
    workspace: &WorkspaceRecord,
    html_path: &RelativePath,
) -> Result<PreparedPreview, AppError>
where
    F: WorkspaceFiles,
{
    let html = files
        .read_text(workspace, html_path, MAX_HTML_BYTES)
        .await
        .map_err(AppError::from)?;
    let mut source = html.content;
    let references = preview_references(&source);
    let mut replacements = Vec::new();
    let mut loaded = HashMap::<String, String>::new();
    let mut dependencies = vec![html_path.clone()];
    let mut urls = Vec::new();
    let mut total_bytes = 0_u64;

    for (start, end, reference) in references {
        if reference.is_empty() || reference.starts_with('#') || reference.starts_with("data:") {
            continue;
        }
        let Some(path) = resolve_preview_path(html_path.as_str(), &reference) else {
            replacements.push((start, end, BLOCKED_REFERENCE.to_owned()));
            continue;
        };
        if let Some(url) = loaded.get(path.as_str()) {
            replacements.push((start, end, url.clone()));
            continue;
        }
        if loaded.len() >= MAX_ASSETS || total_bytes >= MAX_TOTAL_ASSET_BYTES {
            replacements.push((start, end, BLOCKED_REFERENCE.to_owned()));
            continue;
        }
        let Ok(file) = files.read_binary(workspace, &path, MAX_ASSET_BYTES).await else {
            replacements.push((start, end, BLOCKED_REFERENCE.to_owned()));
            continue;
        };
        let size = u64::try_from(file.content.len()).unwrap_or(u64::MAX);
        if total_bytes.saturating_add(size) > MAX_TOTAL_ASSET_BYTES {
            replacements.push((start, end, BLOCKED_REFERENCE.to_owned()));
            continue;
        }
        total_bytes = total_bytes.saturating_add(size);
        // The static iframe intentionally has an opaque sandbox origin. Nested
        // blob URLs inherit the parent origin and are therefore blocked inside
        // that iframe, so embed bounded dependencies in the document blob.
        let url = data_url(&file.content, mime_for_path(path.as_str()));
        dependencies.push(path.clone());
        loaded.insert(path.as_str().to_owned(), url.clone());
        replacements.push((start, end, url));
    }

    for (start, end, replacement) in replacements.into_iter().rev() {
        source.replace_range(start..end, &replacement);
    }
    inject_preview_csp(&mut source);
    let document_url = match object_url(source.as_bytes(), "text/html;charset=utf-8") {
        Ok(url) => url,
        Err(error) => {
            revoke_urls(urls);
            return Err(error);
        }
    };
    urls.push(document_url.clone());
    Ok(PreparedPreview {
        document_url,
        dependencies,
        urls,
    })
}

fn object_url(bytes: &[u8], mime: &str) -> Result<String, AppError> {
    let parts = Array::new();
    parts.push(&Uint8Array::from(bytes));
    let options = BlobPropertyBag::new();
    options.set_type(mime);
    let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(|_| preview_error("Could not create a browser preview blob."))?;
    Url::create_object_url_with_blob(&blob)
        .map_err(|_| preview_error("Could not create a browser preview URL."))
}

fn data_url(bytes: &[u8], mime: &str) -> String {
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

fn inject_preview_csp(source: &mut String) {
    let lowercase = source.to_ascii_lowercase();
    let insertion = lowercase
        .find("<head")
        .and_then(|start| source[start..].find('>').map(|end| start + end + 1))
        .map(|index| (index, PREVIEW_CSP_META.to_owned()))
        .or_else(|| {
            lowercase
                .find("<html")
                .and_then(|start| source[start..].find('>').map(|end| start + end + 1))
                .map(|index| (index, format!("<head>{PREVIEW_CSP_META}</head>")))
        })
        .or_else(|| {
            lowercase
                .find("<!doctype")
                .and_then(|start| source[start..].find('>').map(|end| start + end + 1))
                .map(|index| (index, format!("<head>{PREVIEW_CSP_META}</head>")))
        })
        .unwrap_or_else(|| (0, format!("<head>{PREVIEW_CSP_META}</head>")));
    source.insert_str(insertion.0, &insertion.1);
}

fn revoke_urls(urls: Vec<String>) {
    for url in urls {
        let _ = Url::revoke_object_url(&url);
    }
}

fn preview_error(message: &str) -> AppError {
    AppError::new(
        AppErrorCode::Internal,
        message,
        RetryAdvice::Backoff,
        ErrorSource::Preview,
    )
}

fn preview_references(source: &str) -> Vec<(usize, usize, String)> {
    let mut references = Vec::new();
    for attribute in ["src", "href"] {
        for quote in ['"', '\''] {
            let needle = format!("{attribute}={quote}");
            let mut cursor = 0;
            while let Some(relative_start) = source[cursor..].find(&needle) {
                let value_start = cursor + relative_start + needle.len();
                let Some(relative_end) = source[value_start..].find(quote) else {
                    break;
                };
                let end = value_start + relative_end;
                references.push((value_start, end, source[value_start..end].to_owned()));
                cursor = end + quote.len_utf8();
            }
        }
    }
    references.sort_unstable_by_key(|(start, _, _)| *start);
    references
}

fn resolve_preview_path(document_path: &str, reference: &str) -> Option<RelativePath> {
    let reference = reference.split(['?', '#']).next()?.trim();
    if reference.is_empty()
        || reference.starts_with('/')
        || reference.starts_with('#')
        || reference.starts_with("data:")
        || reference.starts_with("blob:")
        || reference.starts_with("javascript:")
        || reference.contains("://")
    {
        return None;
    }
    let mut segments = document_path
        .rsplit_once('/')
        .map_or_else(Vec::new, |(parent, _)| {
            parent.split('/').map(str::to_owned).collect()
        });
    for segment in reference.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            segment => segments.push(segment.to_owned()),
        }
    }
    RelativePath::try_from(segments.join("/")).ok()
}

fn mime_for_path(path: &str) -> &'static str {
    match path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
    {
        Some(extension) => match extension.as_str() {
            "css" => "text/css;charset=utf-8",
            "js" | "mjs" => "text/javascript;charset=utf-8",
            "html" | "htm" => "text/html;charset=utf-8",
            "json" => "application/json",
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "woff" => "font/woff",
            "woff2" => "font/woff2",
            _ => "application/octet-stream",
        },
        None => "application/octet-stream",
    }
}
