use async_trait::async_trait;
use dioxus::prelude::document;
use serde_json::{Value, json};
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, RetryAdvice};
use syntaxis_app_shell::{AndroidShellPort, AndroidState};

pub(crate) struct AndroidShell;

fn bridge_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        AppErrorCode::InvalidInput,
        error.to_string(),
        RetryAdvice::Never,
        ErrorSource::Workspace,
    )
}

async fn request(value: Value) -> Result<Value, AppError> {
    let eval = document::eval(
        r"
        const request = await dioxus.recv();
        if (!globalThis.SyntaxisAndroid) return null;
        const bridge = globalThis.SyntaxisAndroid;
        const id = crypto.randomUUID?.() || `${Date.now()}-${Math.random()}`;
        return await new Promise((resolve, reject) => {
            const listener = event => {
                const message = JSON.parse(event.data);
                if (message.id !== id) return;
                cleanup();
                if (message.error) reject(new Error(message.error)); else resolve(message.result);
            };
            const cleanup = () => { clearTimeout(timer); bridge.removeEventListener('message', listener); };
            const timer = setTimeout(() => { cleanup(); reject(new Error('Android connection timed out.')); }, 20000);
            bridge.addEventListener('message', listener);
            bridge.postMessage(JSON.stringify({id, ...request}));
        });
    ",
    );
    eval.send(value).map_err(bridge_error)?;
    eval.join::<Value>().await.map_err(bridge_error)
}

#[async_trait(?Send)]
impl AndroidShellPort for AndroidShell {
    async fn state(&self, projects: bool) -> Result<Option<AndroidState>, AppError> {
        let value = request(json!({"action": "state", "projects": projects})).await?;
        serde_json::from_value(value).map_err(bridge_error)
    }
    async fn open(&self, remote: bool, path: &str) -> Result<(), AppError> {
        request(json!({"action": "open", "remote": remote, "path": path})).await?;
        Ok(())
    }
    async fn configure_remote(&self) -> Result<(), AppError> {
        request(json!({"action": "configure"})).await?;
        Ok(())
    }
}
