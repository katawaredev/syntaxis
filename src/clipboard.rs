use dioxus::prelude::document;

/// Copies text using the clipboard implementation available to the current UI target.
///
/// Callers depend only on this API so native targets can replace the WebView
/// implementation without changing feature code.
pub(crate) async fn copy_text(text: String) -> Result<(), String> {
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
    eval.send(text).map_err(|error| error.to_string())?;
    match eval.join::<Option<String>>().await {
        Ok(None) => Ok(()),
        Ok(Some(message)) => Err(message),
        Err(error) => Err(error.to_string()),
    }
}
