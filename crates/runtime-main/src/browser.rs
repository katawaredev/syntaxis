use async_trait::async_trait;
#[cfg(not(target_arch = "wasm32"))]
use base64::{Engine as _, engine::general_purpose::STANDARD};
use dioxus::prelude::document;
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, RetryAdvice};
#[cfg(target_arch = "wasm32")]
use syntaxis_module_files::ImageSourceCleanup;
use syntaxis_module_files::{FilesClipboardPort, ImagePreviewPort, ImageSource};
use syntaxis_module_terminal::TerminalSessionPort;
use syntaxis_terminal::SessionId;
use syntaxis_workspace::WorkspaceId;

#[derive(Clone, Copy, Debug, Default)]
pub struct BrowserClipboard;

#[async_trait(?Send)]
impl FilesClipboardPort for BrowserClipboard {
    async fn copy_text(&self, text: &str) -> Result<(), AppError> {
        let eval = document::eval(
            r#"
            const text = await dioxus.recv();
            try {
                if (globalThis.navigator?.clipboard?.writeText) {
                    await globalThis.navigator.clipboard.writeText(text);
                } else {
                    const input = document.createElement("textarea");
                    input.value = text;
                    input.style.position = "fixed";
                    input.style.opacity = "0";
                    document.body.appendChild(input);
                    input.select();
                    const copied = document.execCommand("copy");
                    input.remove();
                    if (!copied) throw new Error("The browser rejected the copy command.");
                }
                return null;
            } catch (error) {
                return error instanceof Error ? error.message : String(error);
            }
            "#,
        );
        eval.send(text).map_err(files_bridge_error)?;
        match eval.join::<Option<String>>().await {
            Ok(None) => Ok(()),
            Ok(Some(message)) => Err(files_bridge_error(message)),
            Err(error) => Err(files_bridge_error(error)),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MainImagePreview;

impl ImagePreviewPort for MainImagePreview {
    fn create(&self, mime: &str, content: Vec<u8>) -> Result<ImageSource, AppError> {
        #[cfg(target_arch = "wasm32")]
        {
            use js_sys::{Array, Uint8Array};
            use syntaxis_app_contracts::PortHandle;
            use web_sys::{Blob, BlobPropertyBag, Url};

            let parts = Array::new();
            parts.push(&Uint8Array::from(content.as_slice()));
            let options = BlobPropertyBag::new();
            options.set_type(mime);
            let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &options)
                .map_err(|_| files_bridge_error("Could not create an image preview blob."))?;
            let url = Url::create_object_url_with_blob(&blob)
                .map_err(|_| files_bridge_error("Could not create an image preview URL."))?;
            return Ok(ImageSource::new(
                url,
                Some(PortHandle::new(MainImageCleanup)),
            ));
        }
        #[cfg(not(target_arch = "wasm32"))]
        Ok(ImageSource::new(
            format!("data:{mime};base64,{}", STANDARD.encode(content)),
            None,
        ))
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[cfg(target_arch = "wasm32")]
struct MainImageCleanup;

#[cfg(target_arch = "wasm32")]
impl ImageSourceCleanup for MainImageCleanup {
    fn release(&self, url: &str) {
        let _ = web_sys::Url::revoke_object_url(url);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BrowserTerminalSession;

#[async_trait(?Send)]
impl TerminalSessionPort for BrowserTerminalSession {
    async fn load(&self, workspace_id: &WorkspaceId) -> Result<Option<SessionId>, AppError> {
        storage_get(format!("syntaxis.terminal.active.{}", workspace_id.0))
            .await
            .map(|session| session.map(SessionId::new))
    }

    async fn save(
        &self,
        workspace_id: &WorkspaceId,
        session_id: &SessionId,
    ) -> Result<(), AppError> {
        storage_set(
            format!("syntaxis.terminal.active.{}", workspace_id.0),
            session_id.0.clone(),
        )
        .await
    }
}

async fn storage_get(key: String) -> Result<Option<String>, AppError> {
    let eval = document::eval(
        r"
        const key = await dioxus.recv();
        try {
            return globalThis.localStorage?.getItem(key) ?? null;
        } catch (error) {
            throw new Error(error instanceof Error ? error.message : String(error));
        }
        ",
    );
    eval.send(key).map_err(terminal_bridge_error)?;
    eval.join::<Option<String>>()
        .await
        .map_err(terminal_bridge_error)
}

async fn storage_set(key: String, value: String) -> Result<(), AppError> {
    let eval = document::eval(
        r"
        const [key, value] = await dioxus.recv();
        try {
            globalThis.localStorage?.setItem(key, value);
            return null;
        } catch (error) {
            return error instanceof Error ? error.message : String(error);
        }
        ",
    );
    eval.send((key, value)).map_err(terminal_bridge_error)?;
    match eval.join::<Option<String>>().await {
        Ok(None) => Ok(()),
        Ok(Some(message)) => Err(terminal_bridge_error(message)),
        Err(error) => Err(terminal_bridge_error(error)),
    }
}

fn files_bridge_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        AppErrorCode::Internal,
        error.to_string(),
        RetryAdvice::AfterUserAction,
        ErrorSource::Files,
    )
}

fn terminal_bridge_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        AppErrorCode::Internal,
        error.to_string(),
        RetryAdvice::AfterUserAction,
        ErrorSource::Terminal,
    )
}
