use dioxus::prelude::*;
use syntaxis_ui::prelude::{AppIcon, Icon};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct FileMention {
    pub(super) start: usize,
    pub(super) query: String,
}

pub(super) fn mention_query(text: &str) -> Option<FileMention> {
    let start = text
        .rfind(|character: char| character.is_whitespace())
        .map_or(0, |index| index + 1);
    text[start..].strip_prefix('@').map(|query| FileMention {
        start,
        query: query.to_owned(),
    })
}

pub(super) fn insert_file_mention(mut draft: Signal<String>, mention: &FileMention, path: &str) {
    let current = draft();
    let mut updated = current[..mention.start].to_owned();
    updated.push('@');
    if path.chars().any(char::is_whitespace) {
        updated.push('"');
        updated.push_str(path);
        updated.push('"');
    } else {
        updated.push_str(path);
    }
    if !path.ends_with('/') {
        updated.push(' ');
    }
    draft.set(updated);
    crate::ai::agent_view::focus_ai_composer();
}

pub(super) fn append_file_reference(mut draft: Signal<String>, path: &str) {
    let mut value = draft();
    if !value.is_empty() && !value.ends_with(char::is_whitespace) {
        value.push(' ');
    }
    value.push('@');
    if path.chars().any(char::is_whitespace) {
        value.push('"');
        value.push_str(path);
        value.push('"');
    } else {
        value.push_str(path);
    }
    value.push(' ');
    draft.set(value);
    crate::ai::agent_view::focus_ai_composer();
}

pub(super) fn append_text_reference(mut draft: Signal<String>, reference: &str) {
    let mut value = draft();
    if !value.is_empty() && !value.ends_with(char::is_whitespace) {
        value.push(' ');
    }
    value.push_str(reference);
    value.push(' ');
    draft.set(value);
    crate::ai::agent_view::focus_ai_composer();
}

#[component]
pub(super) fn FileMentionMenu(
    paths: Vec<String>,
    mut draft: Signal<String>,
    mention: Option<FileMention>,
    selected: usize,
    status: Option<String>,
) -> Element {
    rsx! {
        if let Some(mention) = mention {
            if !paths.is_empty() || status.is_some() {
                div { class: "absolute right-0 bottom-[calc(100%+7px)] left-0 z-60 overflow-hidden rounded-xl border border-border bg-popover shadow-2xl",
                    div { class: "flex items-center gap-2 border-b border-border px-3 py-2 text-[10px] text-muted-foreground",
                        Icon { icon: AppIcon::Code, size: 13 }
                        "Project files"
                        span { class: "ml-auto max-[520px]:hidden", "Enter to reference" }
                        span { class: "ml-auto hidden max-[520px]:inline", "Tap to reference" }
                    }
                    div { class: "max-h-[min(16rem,35dvh)] overflow-y-auto p-1.5",
                        if let Some(status) = status {
                            p {
                                class: "px-2.5 py-4 text-center text-[10px] text-muted-foreground",
                                role: "status",
                                "{status}"
                            }
                        }
                        for (index, path) in paths.into_iter().enumerate() {
                            button {
                                key: "{path}",
                                class: if index == selected { "flex min-h-9 w-full items-center gap-2 rounded-lg bg-accent px-2.5 py-2 text-left max-[520px]:min-h-11" } else { "flex min-h-9 w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left hover:bg-accent max-[520px]:min-h-11" },
                                onclick: {
                                    let mention = mention.clone();
                                    let path = path.clone();
                                    move |_| insert_file_mention(draft, &mention, &path)
                                },
                                Icon { icon: AppIcon::Code, size: 13 }
                                span { class: "truncate font-mono text-[10px]", "{path}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FileMention, mention_query};
    #[test]
    fn mention_query_uses_the_final_token() {
        assert_eq!(
            mention_query("@"),
            Some(FileMention {
                start: 0,
                query: String::new()
            })
        );
        assert_eq!(
            mention_query("Review @src/com"),
            Some(FileMention {
                start: 7,
                query: "src/com".into()
            })
        );
    }
    #[test]
    fn mention_query_ignores_completed_references() {
        assert_eq!(mention_query("Review @src/main.rs please"), None);
        assert_eq!(mention_query("plain text"), None);
    }
}
