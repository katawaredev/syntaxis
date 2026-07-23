use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use dioxus::prelude::*;
use serde::Deserialize;
use syntaxis_agent::{
    ImageAttachment, PiCommand, MAX_IMAGE_BYTES, MAX_PROMPT_IMAGES, MAX_TOTAL_IMAGE_BYTES,
};
use syntaxis_ui::prelude::{AppIcon, Icon, IconButton};

#[derive(Clone)]
pub(crate) struct ComposerSubmission {
    pub(crate) text: String,
    pub(crate) images: Vec<ImageAttachment>,
}

#[component]
pub(crate) fn AgentComposer(
    mut draft: Signal<String>,
    mut attachments: Signal<Vec<ImageAttachment>>,
    mut composer_error: Signal<Option<String>>,
    connected: bool,
    working: bool,
    pending_messages: usize,
    commands: Vec<PiCommand>,
    accepts_images: bool,
    on_send: EventHandler<ComposerSubmission>,
    on_abort: EventHandler<()>,
) -> Element {
    let speech_active = use_speech_bridge(draft, composer_error);
    use_paste_bridge(attachments, composer_error);
    let images = attachments();
    let can_send = connected
        && (!draft().trim().is_empty() || !images.is_empty())
        && (images.is_empty() || accepts_images);
    let first_command = matching_commands(&commands, &draft()).first().cloned();
    let mut submit = move || {
        if can_send {
            on_send.call(ComposerSubmission {
                text: draft(),
                images: attachments(),
            });
            attachments.set(Vec::new());
        }
    };
    rsx! {
        footer { class: "bg-card px-2.5 pt-1 pb-[max(0.65rem,env(safe-area-inset-bottom))]",
            div { class: "relative mx-auto max-w-3xl",
                SlashCommandMenu { commands, draft }
                div { class: "overflow-hidden rounded-2xl border border-input bg-card shadow-[0_8px_30px_#0002] transition-[border,box-shadow] focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/20",
                    if !images.is_empty() {
                        ComposerAttachments {
                            images: images.clone(),
                            on_remove: move |index| {
                                attachments.write().remove(index);
                                composer_error.set(None);
                            },
                        }
                    }
                    div { class: "ai-composer-editor",
                        textarea {
                            id: "syntaxis-ai-composer",
                            class: "ai-composer-input",
                            rows: 3,
                            value: draft(),
                            disabled: !connected,
                            placeholder: if working { "Steer Pi while it works…" } else { "Ask Pi to change or inspect this project…" },
                            aria_label: "Message Pi",
                            "data-images-enabled": accepts_images && connected,
                            oninput: move |event| {
                                draft.set(event.value());
                                composer_error.set(None);
                            },
                            onkeydown: move |event: KeyboardEvent| {
                                if event.key() == Key::Enter && !event.modifiers().contains(Modifiers::SHIFT) {
                                    event.prevent_default();
                                    if let Some(command) = first_command.as_ref() {
                                        draft.set(format!("/{} ", command.name));
                                    } else {
                                        submit();
                                    }
                                }
                            },
                        }
                    }
                    div { class: "flex min-h-10 items-center gap-1 px-2 pb-2",
                        label {
                            class: if accepts_images && connected { "grid size-8 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" } else { "grid size-8 cursor-not-allowed place-items-center rounded-lg text-muted-foreground opacity-35" },
                            aria_label: if accepts_images { "Attach images" } else { "Selected model does not accept images" },
                            title: if accepts_images { "Attach images" } else { "Selected model does not accept images" },
                            input {
                                class: "hidden",
                                r#type: "file",
                                accept: "image/*",
                                multiple: true,
                                disabled: !accepts_images || !connected,
                                onchange: move |event: FormEvent| {
                                    spawn(load_images(event.files(), attachments, composer_error));
                                },
                            }
                            Icon { icon: AppIcon::Attachment, size: 15 }
                        }
                        IconButton {
                            label: if speech_active() { "Stop dictation" } else { "Dictate message" },
                            icon: AppIcon::Microphone,
                            pressed: speech_active(),
                            disabled: !connected,
                            onclick: move |_| toggle_speech(),
                        }
                        span { class: "min-w-0 flex-1 truncate px-1 text-[9px] text-muted-foreground max-[520px]:hidden",
                            if working {
                                if pending_messages > 0 {
                                    "Steer queued · {pending_messages} pending"
                                } else {
                                    "Enter steers · Shift+Enter adds a line"
                                }
                            } else {
                                "Markdown supported · Enter sends · Shift+Enter adds a line"
                            }
                        }
                        if working {
                            IconButton {
                                label: "Stop Pi",
                                icon: AppIcon::Stop,
                                danger: true,
                                onclick: move |_| on_abort.call(()),
                            }
                        }
                        button {
                            class: "grid size-8.5 place-items-center rounded-lg bg-primary text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-35",
                            disabled: !can_send,
                            aria_label: if working { "Steer Pi" } else { "Send message" },
                            title: if working { "Steer Pi" } else { "Send message" },
                            onclick: move |_| submit(),
                            Icon { icon: AppIcon::Send, size: 15 }
                        }
                    }
                }
                if !images.is_empty() && !accepts_images {
                    p { class: "px-2.5 pt-1.5 text-[10px] text-warning",
                        "Choose a vision-capable model to send these images."
                    }
                }
            }
        }
    }
}

#[component]
fn SlashCommandMenu(commands: Vec<PiCommand>, draft: Signal<String>) -> Element {
    let matches = matching_commands(&commands, &draft());
    rsx! {
        if !matches.is_empty() {
            div { class: "absolute right-0 bottom-[calc(100%+7px)] left-0 z-60 overflow-hidden rounded-xl border border-border bg-popover shadow-2xl",
                div { class: "flex items-center gap-2 border-b border-border px-3 py-2 text-[10px] text-muted-foreground",
                    Icon { icon: AppIcon::Command, size: 13 }
                    "Pi commands"
                    span { class: "ml-auto", "Enter to insert" }
                }
                div { class: "max-h-64 overflow-y-auto p-1.5",
                    for command in matches {
                        SlashCommandRow { key: "{command.name}", command, draft }
                    }
                }
            }
        }
    }
}

#[component]
fn SlashCommandRow(command: PiCommand, mut draft: Signal<String>) -> Element {
    let insertion = format!("/{} ", command.name);
    rsx! {
        button {
            class: "flex min-h-10 w-full items-center gap-3 rounded-lg px-2.5 py-2 text-left hover:bg-accent",
            onclick: move |_| draft.set(insertion.clone()),
            span { class: "grid size-6 shrink-0 place-items-center rounded-md bg-secondary font-mono text-[10px] text-primary",
                "/"
            }
            span { class: "min-w-0 flex-1",
                strong { class: "block truncate font-mono text-[11px]", "/{command.name}" }
                if !command.description.is_empty() {
                    small { class: "block truncate text-[9px] text-muted-foreground",
                        "{command.description}"
                    }
                }
            }
            span { class: "shrink-0 rounded bg-secondary px-1.5 py-0.5 text-[8px] text-muted-foreground",
                "{command.source}"
            }
        }
    }
}

#[component]
fn ComposerAttachments(images: Vec<ImageAttachment>, on_remove: EventHandler<usize>) -> Element {
    rsx! {
        div { class: "flex gap-2 overflow-x-auto border-b border-border/70 px-3 pt-3 pb-2",
            for (index, image) in images.iter().enumerate() {
                AttachmentPreview {
                    key: "{index}-{image.name}",
                    image: image.clone(),
                    on_remove: move |()| on_remove.call(index),
                }
            }
        }
    }
}

#[component]
fn AttachmentPreview(image: ImageAttachment, on_remove: EventHandler<()>) -> Element {
    rsx! {
        div { class: "group relative size-18 shrink-0 overflow-hidden rounded-xl border border-border bg-background",
            img {
                class: "size-full object-cover",
                src: image.data_url(),
                alt: image.name.clone(),
            }
            button {
                class: "touch-only-visible absolute top-1 right-1 grid size-7 place-items-center rounded-full bg-background/90 text-foreground opacity-0 shadow transition-opacity group-hover:opacity-100 focus-visible:opacity-100",
                aria_label: "Remove {image.name}",
                title: "Remove image",
                onclick: move |_| on_remove.call(()),
                Icon { icon: AppIcon::Close, size: 11 }
            }
            span { class: "absolute right-0 bottom-0 left-0 truncate bg-black/60 px-1.5 py-1 text-[8px] text-white",
                "{image.name}"
            }
        }
    }
}

pub(crate) async fn load_images(
    files: Vec<dioxus::html::FileData>,
    mut attachments: Signal<Vec<ImageAttachment>>,
    mut error: Signal<Option<String>>,
) {
    for file in files {
        if attachments().len() >= MAX_PROMPT_IMAGES {
            error.set(Some(format!("Attach up to {MAX_PROMPT_IMAGES} images.")));
            break;
        }
        let mime_type = file.content_type().unwrap_or_default();
        if !mime_type.starts_with("image/") {
            error.set(Some(format!("{} is not an image.", file.name())));
            continue;
        }
        let total = attachments().iter().map(|image| image.size).sum::<u64>();
        if file.size() > MAX_IMAGE_BYTES
            || total.saturating_add(file.size()) > MAX_TOTAL_IMAGE_BYTES
        {
            error.set(Some("Images can be 8 MiB each and 16 MiB total.".into()));
            continue;
        }
        match file.read_bytes().await {
            Ok(bytes) => attachments.write().push(ImageAttachment {
                name: file.name(),
                mime_type,
                size: file.size(),
                data: BASE64.encode(bytes),
            }),
            Err(_) => error.set(Some(format!("Could not read {}.", file.name()))),
        }
    }
}

#[derive(Deserialize)]
struct PasteBridgeEvent {
    kind: String,
    name: Option<String>,
    mime_type: Option<String>,
    data: Option<String>,
    message: Option<String>,
}

fn use_paste_bridge(attachments: Signal<Vec<ImageAttachment>>, error: Signal<Option<String>>) {
    let mut bridge = use_signal(|| None::<dioxus::document::Eval>);
    use_effect(move || {
        let mut events = document::eval(
            r#"
            const id = await dioxus.recv();
            const listener = event => {
                if (event.detail?.id === id) dioxus.send(event.detail);
            };
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

fn use_speech_bridge(draft: Signal<String>, error: Signal<Option<String>>) -> Signal<bool> {
    let active = use_signal(|| false);
    let mut bridge = use_signal(|| None::<dioxus::document::Eval>);
    use_effect(move || {
        let mut events = document::eval(
            r#"
            const id = await dioxus.recv();
            const listener = event => {
                if (event.detail?.id === id) dioxus.send(event.detail);
            };
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

fn toggle_speech() {
    let _ = document::eval(
        r#"
        window.SyntaxisAiChat?.toggleSpeech("syntaxis-ai-composer");
        "#,
    );
}

fn matching_commands(commands: &[PiCommand], draft: &str) -> Vec<PiCommand> {
    let Some(query) = draft.strip_prefix('/') else {
        return Vec::new();
    };
    if query.chars().any(char::is_whitespace) {
        return Vec::new();
    }
    let query = query.to_ascii_lowercase();
    commands
        .iter()
        .filter(|command| {
            query.is_empty()
                || command.name.to_ascii_lowercase().contains(&query)
                || command.description.to_ascii_lowercase().contains(&query)
        })
        .take(10)
        .cloned()
        .collect()
}
