//! Message bubbles and their actions. Conversation orchestration stays in the view.

use dioxus::prelude::*;
use syntaxis_module_files::{render_markdown, render_markdown_preserving_newlines};
use syntaxis_ui::prelude::{AppIcon, IconButton};

use crate::{AiImageAttachment, AiMessage, AiMessageStatus, AiRole};

#[component]
pub(crate) fn ConversationMessage(
    message: AiMessage,
    can_edit: bool,
    read_aloud_available: bool,
    speaking: bool,
    on_image: EventHandler<AiImageAttachment>,
    on_edit: EventHandler<AiMessage>,
    on_copy: EventHandler<String>,
    on_read: EventHandler<String>,
) -> Element {
    match message.role {
        AiRole::User => {
            let rendered = render_markdown_preserving_newlines(&message.content);
            let copy_content = message.content.clone();
            rsx! {
                div { class: "group/message ml-auto mb-3 flex max-w-[88%] flex-col items-end",
                    article {
                        class: "max-w-full rounded-xl rounded-br-sm border border-border bg-secondary px-3.5 py-2.5 text-[13px] leading-relaxed text-secondary-foreground shadow-sm",
                        dir: "auto",
                        if !message.images.is_empty() {
                            div { class: "mb-2 grid max-w-lg grid-cols-2 gap-1.5",
                                for image in message.images.clone() {
                                    button {
                                        class: "cursor-zoom-in rounded-lg focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
                                        r#type: "button",
                                        aria_label: "Open {image.name}",
                                        onclick: {
                                            let image = image.clone();
                                            move |_| on_image.call(image.clone())
                                        },
                                        img {
                                            class: "max-h-52 min-h-20 w-full rounded-lg bg-black/10 object-cover",
                                            src: image.data_url(), alt: image.name,
                                            width: "320", height: "208",
                                        }
                                    }
                                }
                            }
                        }
                        if !message.content.is_empty() {
                            div { class: "ai-markdown ai-user-markdown", dangerous_inner_html: rendered }
                        }
                    }
                    if !copy_content.is_empty() || message.entry_id.is_some() {
                        div { class: "flex min-h-9 items-center gap-0.5 pt-0.5 opacity-0 transition-opacity group-hover/message:opacity-100 focus-within:opacity-100 max-[520px]:opacity-100",
                            if !copy_content.is_empty() {
                                IconButton {
                                    label: "Copy message", icon: AppIcon::Copy,
                                    onclick: move |_| on_copy.call(copy_content.clone()),
                                }
                            }
                            if message.entry_id.is_some() {
                                IconButton {
                                    label: "Edit this prompt and branch from here",
                                    icon: AppIcon::Split,
                                    icon_class: "rotate-180",
                                    disabled: !can_edit,
                                    onclick: move |_| on_edit.call(message.clone()),
                                }
                            }
                        }
                    }
                }
            }
        }
        AiRole::Assistant => {
            let rendered = render_markdown(&message.content);
            rsx! {
                article { class: "group/message mb-3 max-w-full py-1 pr-2 text-[13px] leading-relaxed text-foreground", dir: "auto",
                    if !message.thinking.trim().is_empty() {
                        details { class: "mb-2 rounded-lg border border-border bg-background/60 text-[11px] text-muted-foreground",
                            summary { class: "cursor-pointer px-3 py-2 select-none", "Reasoning" }
                            div { class: "max-h-60 overflow-auto border-t border-border px-3 py-2 font-mono text-[10px] leading-relaxed whitespace-pre-wrap", "{message.thinking}" }
                        }
                    }
                    if !message.content.is_empty() {
                        div { class: "ai-markdown", "data-agent-response": message.id.clone(), dangerous_inner_html: rendered }
                    }
                    if matches!(message.status, AiMessageStatus::Failed | AiMessageStatus::Stopped) {
                        small { class: "mt-1 block text-[10px] text-destructive",
                            if message.status == AiMessageStatus::Stopped { "Stopped" } else { "Response failed" }
                        }
                    }
                    if message.truncated {
                        small {
                            class: "mt-2 block rounded-md border border-warning/30 bg-warning/8 px-2.5 py-2 text-[10px] leading-relaxed text-warning",
                            role: "status",
                            "This response reached the model's output limit and may be incomplete."
                        }
                    }
                    if !message.content.is_empty() && message.status != AiMessageStatus::Streaming {
                        div { class: "flex min-h-9 items-center pt-0.5 opacity-0 transition-opacity group-hover/message:opacity-100 focus-within:opacity-100 max-[520px]:opacity-100",
                            if read_aloud_available {
                                IconButton {
                                    label: if speaking { "Stop reading response" } else { "Read response aloud" },
                                    icon: if speaking { AppIcon::Stop } else { AppIcon::Volume2 },
                                    pressed: speaking,
                                    onclick: { let id = message.id.clone(); move |_| on_read.call(id.clone()) },
                                }
                            }
                            IconButton {
                                label: "Copy response", icon: AppIcon::Copy,
                                onclick: { let content = message.content.clone(); move |_| on_copy.call(content.clone()) },
                            }
                        }
                    }
                }
            }
        }
        AiRole::System => rsx! {
            p { class: "mb-3 rounded-lg border border-border bg-muted/30 px-3 py-2 text-xs text-muted-foreground whitespace-pre-wrap", "{message.content}" }
        },
    }
}
