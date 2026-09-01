use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use syntaxis_ui::prelude::{
    AiChatHeader, AiSendButton, AiSidebarTabs, AppIcon, Button, ButtonKind, ControlSize, Field, Icon, IconButton,
    TextInput, TextInputType,
};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};

const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";
const DEFAULT_MODEL: &str = "gpt-4.1-mini";
const MAX_PROMPT_BYTES: usize = 64 * 1024;
const MAX_CONTEXT_BYTES: usize = 128 * 1024;
const MAX_CONVERSATION_BYTES: usize = 512 * 1024;

#[derive(Clone, Copy)]
pub(super) struct GuestAiConfig {
    endpoint: Signal<String>,
    model: Signal<String>,
    api_key: Signal<String>,
    prompt: Signal<String>,
    messages: Signal<Vec<ChatMessage>>,
}

pub(super) fn provide_guest_ai_config() {
    let endpoint = use_signal(|| DEFAULT_ENDPOINT.to_owned());
    let model = use_signal(|| DEFAULT_MODEL.to_owned());
    let api_key = use_signal(String::new);
    let prompt = use_signal(String::new);
    let messages = use_signal(Vec::new);
    use_context_provider(move || GuestAiConfig {
        endpoint,
        model,
        api_key,
        prompt,
        messages,
    });
}
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[component]
pub(super) fn GuestAi(
    slug: String,
    active_path: Option<String>,
    active_contents: Option<String>,
) -> Element {
    let navigator = use_navigator();
    let GuestAiConfig {
        endpoint,
        model,
        api_key,
        mut prompt,
        mut messages,
    } = use_context();
    let mut sidebar_open = use_signal(|| true);
    let mut include_file = use_signal(|| false);
    let mut pending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let submit_path = active_path.clone();
    let submit_contents = active_contents.clone();

    rsx! {
        section {
            class: if sidebar_open() { "guest-ai" } else { "guest-ai guest-ai-sidebar-hidden" },
            "aria-label": "Browser AI assistant",
            nav { class: "guest-ai-sidebar", "aria-label": "Agent chats",
                AiSidebarTabs {
                    settings_active: false,
                    on_chat: move |()| {},
                    on_settings: {
                        let slug = slug.clone();
                        move |()| {
                            navigator.push(super::GuestRoute::AiSettingsSection {
                                slug: slug.clone(),
                                section: "provider-accounts".to_owned(),
                            });
                        }
                    },
                }
                div { class: "guest-ai-sidebar-actions",
                    Button {
                        label: "New chat",
                        kind: ButtonKind::Primary,
                        disabled: pending(),
                        onclick: move |_| {
                            messages.write().clear();
                            prompt.set(String::new());
                            error.set(None);
                        },
                    }
                }
                div { class: "guest-ai-session-list",
                    if messages().is_empty() {
                        div { class: "guest-ai-session-empty",
                            div { class: "guest-ai-session-icon",
                                Icon { icon: AppIcon::Sparkles, size: 17 }
                            }
                            strong { "No chats yet" }
                            p { "Start a local chat for this workspace." }
                        }
                    } else {
                        button {
                            class: "guest-ai-session guest-ai-session-active",
                            r#type: "button",
                            "Current chat"
                            small { "{messages().len()} messages" }
                        }
                    }
                }

            }
            main { class: "guest-ai-main",
                AiChatHeader {
                    title: "New chat",
                    connected: !api_key().trim().is_empty(),
                    sidebar_open: sidebar_open(),
                    on_toggle_sidebar: move |()| sidebar_open.toggle(),
                    on_open_sidebar: move |()| sidebar_open.set(true),
                    actions: rsx! {
                        IconButton {
                            label: "Workspace selection requires the Syntaxis server",
                            icon: AppIcon::GitBranch,
                            disabled: true,
                            onclick: move |_| {},
                        }
                        IconButton {
                            label: "Model settings",
                            icon: AppIcon::Bot,
                            onclick: {
                                let slug = slug.clone();
                                move |_| {
                                    navigator.push(super::GuestRoute::AiSettingsSection {
                                        slug: slug.clone(),
                                        section: "provider-accounts".to_owned(),
                                    });
                                }
                            },
                        }
                        IconButton {
                            label: "Usage details require the Syntaxis server",
                            icon: AppIcon::MoreVertical,
                            disabled: true,
                            onclick: move |_| {},
                        }
                    },
                }
                div {
                    class: "guest-ai-timeline",
                    role: "log",
                    "aria-live": "polite",
                    if messages().is_empty() {
                        div { class: "guest-module-empty",
                            h3 { "Ask about your project" }
                            p {
                                "Open a text file to optionally attach it as context, then enter a request below."
                            }
                        }
                    }
                    for (index, message) in messages().into_iter().enumerate() {
                        article {
                            key: "{index}",
                            class: if message.role == "user" { "guest-ai-message guest-ai-user" } else { "guest-ai-message guest-ai-assistant" },
                            strong {
                                if message.role == "user" {
                                    "You"
                                } else {
                                    "Assistant"
                                }
                            }
                            pre { "{message.content}" }
                        }
                    }
                    if pending() {
                        p { class: "guest-module-note", "Waiting for the provider…" }
                    }
                }
                if let Some(message) = error() {
                    p { class: "guest-ai-error", role: "alert", "{message}" }
                }
                form {
                    class: "guest-ai-composer",
                    onsubmit: move |event| {
                        event.prevent_default();
                        let request = prompt().trim().to_owned();
                        if request.is_empty() || pending() {
                            return;
                        }
                        if api_key().trim().is_empty() {
                            error.set(Some("Enter an API key in Provider settings.".to_owned()));
                            return;
                        }
                        if request.len() > MAX_PROMPT_BYTES {
                            error.set(Some("The prompt exceeds the 64 KiB browser limit.".to_owned()));
                            return;
                        }
                        let mut outgoing = messages();
                        let content = if include_file() {
                            match (submit_path.clone(), submit_contents.clone()) {
                                (Some(path), Some(contents)) if contents.len() <= MAX_CONTEXT_BYTES => {
                                    format!(
                                        "Active file: {path}\n\n```\n{contents}\n```\n\nRequest: {request}",
                                    )
                                }
                                (Some(_), Some(_)) => {
                                    error
                                        .set(
                                            Some(
                                                "The active file exceeds the 128 KiB AI context limit."
                                                    .to_owned(),
                                            ),
                                        );
                                    return;
                                }
                                _ => request.clone(),
                            }
                        } else {
                            request.clone()
                        };
                        outgoing
                            .push(ChatMessage {
                                role: "user".to_owned(),
                                content,
                            });
                        while outgoing.iter().map(|message| message.content.len()).sum::<usize>()
                            > MAX_CONVERSATION_BYTES && outgoing.len() > 1
                        {
                            outgoing.remove(0);
                        }
                        if outgoing.iter().map(|message| message.content.len()).sum::<usize>()
                            > MAX_CONVERSATION_BYTES
                        {
                            error
                                .set(
                                    Some(
                                        "The AI conversation exceeds the 512 KiB browser limit."
                                            .to_owned(),
                                    ),
                                );
                            return;
                        }
                        messages
                            .write()
                            .push(ChatMessage {
                                role: "user".to_owned(),
                                content: request,
                            });
                        prompt.set(String::new());
                        error.set(None);
                        pending.set(true);
                        let endpoint_value = endpoint().trim().to_owned();
                        let model_value = model().trim().to_owned();
                        let key_value = api_key().trim().to_owned();
                        spawn(async move {
                            match send_chat(&endpoint_value, &model_value, &key_value, &outgoing).await {
                                Ok(response) => messages.write().push(response),
                                Err(message) => error.set(Some(message)),
                            }
                            pending.set(false);
                        });
                    },
                    textarea {
                        value: prompt,
                        rows: 4,
                        maxlength: MAX_PROMPT_BYTES,
                        placeholder: "Ask Syntaxis…",
                        aria_label: "AI message",
                        oninput: move |event| prompt.set(event.value()),
                    }
                    div { class: "guest-ai-composer-toolbar",
                        IconButton {
                            label: "Attach files (unavailable in browser chat)",
                            icon: AppIcon::Attach,
                            size: ControlSize::Small,
                            disabled: true,
                            onclick: move |_| {},
                        }
                        IconButton {
                            label: if let Some(path) = active_path.as_deref() {
                                format!("Reference {path}")
                            } else {
                                "Open a file to reference it".to_owned()
                            },
                            icon: AppIcon::LineNumbers,
                            size: ControlSize::Small,
                            pressed: include_file(),
                            disabled: active_path.is_none(),
                            onclick: move |_| include_file.toggle(),
                        }
                        IconButton {
                            label: "Dictation unavailable in browser chat",
                            icon: AppIcon::Microphone,
                            size: ControlSize::Small,
                            disabled: true,
                            onclick: move |_| {},
                        }
                        span { class: "guest-ai-composer-hint",
                            "Markdown supported · Enter sends"
                        }
                        AiSendButton {
                            disabled: pending() || prompt().trim().is_empty(),
                            submit: true,
                            onclick: move |_| {},
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn GuestAiSettings(slug: String) -> Element {
    let navigator = use_navigator();
    let GuestAiConfig {
        mut endpoint,
        mut model,
        mut api_key,
        ..
    } = use_context();

    rsx! {
        section { class: "guest-ai guest-ai-settings-view", "aria-label": "AI settings",
            nav { class: "guest-ai-sidebar", "aria-label": "AI settings sections",
                AiSidebarTabs {
                    settings_active: true,
                    on_chat: {
                        let slug = slug.clone();
                        move |()| navigator.push(super::GuestRoute::Ai { slug: slug.clone() })
                    },
                    on_settings: move |()| {},
                }
                div { class: "guest-ai-settings-sections",
                    button { class: "guest-ai-settings-section guest-ai-settings-section-active",
                        r#type: "button",
                        "Provider accounts"
                    }
                    for label in ["General", "Global instructions", "Prompt templates", "Skills", "Extensions"] {
                        button {
                            class: "guest-ai-settings-section",
                            r#type: "button",
                            disabled: true,
                            title: "Requires the Syntaxis server",
                            "{label}"
                        }
                    }
                }
            }
            main { class: "guest-ai-main",
                header { class: "guest-module-header",
                    div {
                        h2 { "Provider accounts" }
                        p { "Configure the provider used by browser-only chat." }
                    }
                }
                div { class: "guest-ai-settings-panel",
                    section { class: "guest-ai-settings-card",
                        h3 { "OpenAI-compatible provider" }
                        p {
                            "Credentials remain in memory and are cleared when this browser tab closes."
                        }
                        Field {
                            control_id: "guest-ai-endpoint",
                            label: "Endpoint",
                            TextInput {
                                value: endpoint(),
                                placeholder: DEFAULT_ENDPOINT,
                                oninput: move |event: FormEvent| endpoint.set(event.value()),
                            }
                        }
                        Field {
                            control_id: "guest-ai-model",
                            label: "Model",
                            TextInput {
                                value: model(),
                                placeholder: DEFAULT_MODEL,
                                oninput: move |event: FormEvent| model.set(event.value()),
                            }
                        }
                        Field {
                            control_id: "guest-ai-api-key",
                            label: "API key",
                            TextInput {
                                input_type: TextInputType::Password,
                                value: api_key(),
                                placeholder: "sk-…",
                                autocomplete: "off",
                                oninput: move |event: FormEvent| api_key.set(event.value()),
                            }
                        }
                        p { class: "guest-module-note",
                            "Direct requests require an HTTPS endpoint that permits browser CORS."
                        }
                    }
                }
            }
        }
    }
}

async fn send_chat(
    endpoint: &str,
    model: &str,
    api_key: &str,
    messages: &[ChatMessage],
) -> Result<ChatMessage, String> {
    if !endpoint.starts_with("https://") {
        return Err("The provider endpoint must use HTTPS.".to_owned());
    }
    if model.is_empty() {
        return Err("Enter a model name.".to_owned());
    }
    let body = serde_json::to_string(&ChatRequest { model, messages })
        .map_err(|error| format!("Could not encode the AI request: {error}"))?;
    let options = RequestInit::new();
    options.set_method("POST");
    options.set_mode(RequestMode::Cors);
    options.set_body(&JsValue::from_str(&body));
    let request = Request::new_with_str_and_init(endpoint, &options)
        .map_err(|value| browser_error("Could not create the AI request", value))?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|value| browser_error("Could not set the request content type", value))?;
    request
        .headers()
        .set("Authorization", &format!("Bearer {api_key}"))
        .map_err(|value| browser_error("Could not authorize the AI request", value))?;
    let window =
        web_sys::window().ok_or_else(|| "The browser window is unavailable.".to_owned())?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|value| browser_error("The provider request failed", value))?
        .dyn_into::<Response>()
        .map_err(|value| browser_error("The provider returned an invalid response", value))?;
    let status = response.status();
    let text = JsFuture::from(
        response
            .text()
            .map_err(|value| browser_error("Could not read the provider response", value))?,
    )
    .await
    .map_err(|value| browser_error("Could not read the provider response", value))?
    .as_string()
    .ok_or_else(|| "The provider response was not text.".to_owned())?;
    if text.len() > MAX_RESPONSE_BYTES {
        return Err("The provider response exceeds the 1 MiB browser limit.".to_owned());
    }
    if !response.ok() {
        let detail = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|value| {
                value
                    .pointer("/error/message")
                    .and_then(|message| message.as_str())
                    .map(str::to_owned)
            })
            .unwrap_or(text);
        return Err(format!("Provider returned HTTP {status}: {detail}"));
    }
    let parsed: ChatResponse = serde_json::from_str(&text)
        .map_err(|error| format!("The provider response was not compatible: {error}"))?;
    parsed
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message)
        .ok_or_else(|| "The provider returned no assistant message.".to_owned())
}

fn browser_error(context: &str, value: JsValue) -> String {
    let detail = value.as_string().unwrap_or_else(|| format!("{value:?}"));
    format!("{context}: {detail}")
}
