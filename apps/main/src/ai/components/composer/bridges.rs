use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use dioxus::prelude::*;
use serde::Deserialize;
use syntaxis_agent::{ImageAttachment, MAX_IMAGE_BYTES, MAX_PROMPT_IMAGES, MAX_TOTAL_IMAGE_BYTES};

#[derive(Deserialize)]
struct PasteBridgeEvent {
    kind: String,
    name: Option<String>,
    mime_type: Option<String>,
    data: Option<String>,
    message: Option<String>,
}

pub(super) fn use_paste_bridge(
    attachments: Signal<Vec<ImageAttachment>>,
    error: Signal<Option<String>>,
) {
    let mut bridge = use_signal(|| None::<dioxus::document::Eval>);
    use_effect(move || {
        let mut events = document::eval(
            r#"
            const id = await dioxus.recv();
            const listener = event => { if (event.detail?.id === id) dioxus.send(event.detail); };
            window.addEventListener("syntaxis-ai-paste", listener);
            await dioxus.recv();
            window.removeEventListener("syntaxis-ai-paste", listener);
        "#,
        );
        let _ = events.send("syntaxis-ai-composer");
        bridge.set(Some(events));
        spawn(async move {
            while let Ok(event) = events.recv::<PasteBridgeEvent>().await {
                apply_paste_event(event, attachments, error);
            }
        });
    });
    use_drop(move || {
        if let Some(events) = bridge() {
            let _ = events.send(true);
        }
    });
}

fn apply_paste_event(
    event: PasteBridgeEvent,
    mut attachments: Signal<Vec<ImageAttachment>>,
    mut error: Signal<Option<String>>,
) {
    if event.kind == "error" {
        error.set(event.message);
        return;
    }
    let Some(data) = event.data else {
        return;
    };
    let mime_type = event.mime_type.unwrap_or_default();
    if !mime_type.starts_with("image/") {
        return;
    }
    if attachments().len() >= MAX_PROMPT_IMAGES {
        error.set(Some(format!("Attach up to {MAX_PROMPT_IMAGES} images.")));
        return;
    }
    let max_encoded_size = usize::try_from(MAX_IMAGE_BYTES)
        .unwrap_or(usize::MAX)
        .saturating_mul(4)
        / 3
        + 4;
    if data.len() > max_encoded_size {
        error.set(Some("Images can be 8 MiB each and 16 MiB total.".into()));
        return;
    }
    let Ok(bytes) = BASE64.decode(&data) else {
        error.set(Some("Could not read the pasted image.".into()));
        return;
    };
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let total = attachments().iter().map(|image| image.size).sum::<u64>();
    if size > MAX_IMAGE_BYTES || total.saturating_add(size) > MAX_TOTAL_IMAGE_BYTES {
        error.set(Some("Images can be 8 MiB each and 16 MiB total.".into()));
        return;
    }
    attachments.write().push(ImageAttachment {
        name: event.name.unwrap_or_else(|| "Pasted image".into()),
        mime_type,
        size,
        data,
    });
    error.set(None);
}

#[derive(Deserialize)]
struct SpeechBridgeEvent {
    kind: String,
    text: Option<String>,
    message: Option<String>,
}

pub(super) fn use_speech_bridge(
    draft: Signal<String>,
    error: Signal<Option<String>>,
) -> Signal<bool> {
    let active = use_signal(|| false);
    let mut bridge = use_signal(|| None::<dioxus::document::Eval>);
    use_effect(move || {
        let mut events = document::eval(
            r#"
            const id = await dioxus.recv();
            const listener = event => { if (event.detail?.id === id) dioxus.send(event.detail); };
            window.addEventListener("syntaxis-ai-speech", listener);
            await dioxus.recv();
            window.removeEventListener("syntaxis-ai-speech", listener);
        "#,
        );
        let _ = events.send("syntaxis-ai-composer");
        bridge.set(Some(events));
        spawn(async move {
            while let Ok(event) = events.recv::<SpeechBridgeEvent>().await {
                apply_speech_event(event, draft, active, error);
            }
        });
    });
    use_drop(move || {
        if let Some(events) = bridge() {
            let _ = events.send(true);
        }
    });
    active
}

fn apply_speech_event(
    event: SpeechBridgeEvent,
    mut draft: Signal<String>,
    mut active: Signal<bool>,
    mut error: Signal<Option<String>>,
) {
    match event.kind.as_str() {
        "start" => {
            active.set(true);
            error.set(None);
        }
        "end" => active.set(false),
        "transcript" => {
            if let Some(text) = event.text {
                let mut value = draft.write();
                if !value.is_empty() && !value.ends_with(char::is_whitespace) {
                    value.push(' ');
                }
                value.push_str(text.trim());
            }
        }
        "error" => {
            active.set(false);
            error.set(Some(event.message.unwrap_or_else(|| {
                "Speech recognition is unavailable in this browser.".into()
            })));
        }
        _ => {}
    }
}

pub(super) fn toggle_speech() {
    let _ = document::eval(r#"window.SyntaxisAiChat?.toggleSpeech("syntaxis-ai-composer");"#);
}
