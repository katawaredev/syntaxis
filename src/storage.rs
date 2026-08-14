use dioxus::prelude::document;

/// Reads a value from persistent UI storage.
pub(crate) async fn get(key: String) -> Result<Option<String>, String> {
    let eval = document::eval(
        r#"
        const key = await dioxus.recv();
        try {
            return globalThis.localStorage?.getItem(key) ?? null;
        } catch (error) {
            throw new Error(error instanceof Error ? error.message : String(error));
        }
        "#,
    );
    eval.send(key).map_err(|error| error.to_string())?;
    eval.join::<Option<String>>()
        .await
        .map_err(|error| error.to_string())
}

/// Writes a value to persistent UI storage.
pub(crate) async fn set(key: String, value: String) -> Result<(), String> {
    run_mutation(
        r#"
        const [key, value] = await dioxus.recv();
        try {
            globalThis.localStorage?.setItem(key, value);
            return null;
        } catch (error) {
            return error instanceof Error ? error.message : String(error);
        }
        "#,
        (key, value),
    )
    .await
}

/// Removes a value from persistent UI storage.
pub(crate) async fn remove(key: String) -> Result<(), String> {
    run_mutation(
        r#"
        const key = await dioxus.recv();
        try {
            globalThis.localStorage?.removeItem(key);
            return null;
        } catch (error) {
            return error instanceof Error ? error.message : String(error);
        }
        "#,
        key,
    )
    .await
}

async fn run_mutation<T>(script: &'static str, message: T) -> Result<(), String>
where
    T: serde::Serialize,
{
    let eval = document::eval(script);
    eval.send(message).map_err(|error| error.to_string())?;
    match eval.join::<Option<String>>().await {
        Ok(None) => Ok(()),
        Ok(Some(message)) => Err(message),
        Err(error) => Err(error.to_string()),
    }
}
