use std::rc::Rc;

use dioxus::prelude::*;
pub use syntaxis_editor::SearchOptions;

use crate::{AppIcon, Icon};

/// Canonical in-editor find and replace strip.
#[component]
pub fn SearchPanel(
    mut query: Signal<String>,
    mut current: Signal<usize>,
    mut options: Signal<SearchOptions>,
    mut replacement: Signal<String>,
    mut replace_open: Signal<bool>,
    mut search_input: Signal<Option<Rc<MountedData>>>,
    count: usize,
    error: Option<String>,
    on_next: EventHandler<i8>,
    on_replace: EventHandler<()>,
    on_replace_all: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    use_drop(move || search_input.set(None));
    let active = options();
    let group_class = if error.is_some() {
        "flex min-w-0 flex-1 items-center overflow-hidden rounded-md border border-destructive bg-card/70 shadow-xs"
    } else {
        "flex min-w-0 flex-1 items-center overflow-hidden rounded-md border border-input bg-card/70 shadow-xs focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/25"
    };

    rsx! {
        div { class: "flex shrink-0 flex-col border-b border-border bg-background",
            div { class: "flex min-h-10 items-center gap-1 px-1.5",
                div { class: group_class,
                    input {
                        class: "h-7.5 min-w-0 flex-1 bg-transparent px-2 text-xs text-foreground outline-none placeholder:text-muted-foreground/70",
                        r#type: "text",
                        name: "editor-search-query",
                        autocomplete: "off",
                        value: query(),
                        placeholder: "Find in file",
                        aria_label: "Find in file",
                        aria_invalid: error.is_some(),
                        onmounted: move |event| {
                            let input = event.data();
                            search_input.set(Some(Rc::clone(&input)));
                            spawn(async move {
                                let _ = input.set_focus(true).await;
                            });
                        },
                        oninput: move |event: FormEvent| {
                            query.set(event.value());
                            current.set(0);
                        },
                        onkeydown: move |event: KeyboardEvent| match event.key() {
                            Key::Enter => {
                                event.prevent_default();
                                on_next.call(if event.modifiers().contains(Modifiers::SHIFT) { -1 } else { 1 });
                            }
                            Key::Escape => {
                                event.prevent_default();
                                on_close.call(());
                            }
                            _ => {}
                        },
                    }
                    SearchModeButton {
                        label: "Match case",
                        icon: AppIcon::MatchCase,
                        active: active.case_sensitive,
                        onclick: move |()| {
                            options.write().case_sensitive = !active.case_sensitive;
                            current.set(0);
                        },
                    }
                    SearchModeButton {
                        label: "Match whole word",
                        icon: AppIcon::MatchWholeWord,
                        active: active.whole_word,
                        onclick: move |()| {
                            options.write().whole_word = !active.whole_word;
                            current.set(0);
                        },
                    }
                    SearchModeButton {
                        label: "Use regular expression",
                        icon: AppIcon::Regex,
                        active: active.regex,
                        onclick: move |()| {
                            options.write().regex = !active.regex;
                            current.set(0);
                        },
                    }
                    SearchModeButton {
                        label: if replace_open() { "Hide replace" } else { "Show replace" },
                        icon: AppIcon::ToggleReplace,
                        active: replace_open(),
                        onclick: move |()| replace_open.toggle(),
                    }
                }
                span {
                    class: if error.is_some() { "min-w-10 shrink-0 text-center text-[10px] text-destructive" } else { "min-w-10 shrink-0 text-center text-[10px] tabular-nums text-muted-foreground" },
                    title: error.clone().unwrap_or_default(),
                    if error.is_some() {
                        "Invalid"
                    } else if count == 0 {
                        "0/0"
                    } else {
                        {format!("{}/{}", current().min(count - 1) + 1, count)}
                    }
                }
                SearchControlButton { label: "Previous match", title: "Previous match (Shift Enter)", icon: AppIcon::Previous, disabled: count == 0 || error.is_some(), onclick: move |()| on_next.call(-1) }
                SearchControlButton { label: "Next match", title: "Next match (Enter)", icon: AppIcon::Next, disabled: count == 0 || error.is_some(), onclick: move |()| on_next.call(1) }
                SearchControlButton { label: "Close search", title: "Close search (Escape)", icon: AppIcon::Close, onclick: move |()| on_close.call(()) }
            }
            if replace_open() {
                div { class: "flex min-h-10 items-center gap-1 border-t border-border/60 px-1.5",
                    div { class: "flex min-w-0 flex-1 items-center overflow-hidden rounded-md border border-input bg-card/70 shadow-xs focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/25",
                        input {
                            class: "h-7.5 min-w-0 flex-1 bg-transparent px-2 text-xs text-foreground outline-none placeholder:text-muted-foreground/70",
                            r#type: "text",
                            name: "editor-search-replacement",
                            autocomplete: "off",
                            value: replacement(),
                            placeholder: "Replace with…",
                            aria_label: "Replace with",
                            autofocus: true,
                            oninput: move |event: FormEvent| replacement.set(event.value()),
                            onkeydown: move |event: KeyboardEvent| match event.key() {
                                Key::Enter => {
                                    event.prevent_default();
                                    if event.modifiers().intersects(Modifiers::CONTROL | Modifiers::META) {
                                        on_replace_all.call(());
                                    } else {
                                        on_replace.call(());
                                    }
                                }
                                Key::Escape => {
                                    event.prevent_default();
                                    on_close.call(());
                                }
                                _ => {}
                            },
                        }
                    }
                    SearchControlButton { label: "Replace current match", title: "Replace current match (Enter)", icon: AppIcon::ReplaceNext, disabled: count == 0 || error.is_some(), onclick: move |()| on_replace.call(()) }
                    SearchControlButton { label: "Replace all matches", title: "Replace all matches (Mod Enter)", icon: AppIcon::ReplaceAll, disabled: count == 0 || error.is_some(), onclick: move |()| on_replace_all.call(()) }
                }
            }
        }
    }
}

#[component]
fn SearchModeButton(
    label: String,
    icon: AppIcon,
    active: bool,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            class: if active { "grid size-7 shrink-0 place-items-center rounded-sm bg-accent text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring" } else { "grid size-7 shrink-0 place-items-center rounded-sm text-muted-foreground outline-none hover:bg-accent/70 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring" },
            r#type: "button",
            title: label.clone(),
            aria_label: label,
            aria_pressed: active,
            onclick: move |_| onclick.call(()),
            Icon { icon, size: 14 }
        }
    }
}

#[component]
fn SearchControlButton(
    label: String,
    title: String,
    icon: AppIcon,
    #[props(default = false)] disabled: bool,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            class: "grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground outline-none hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-35",
            r#type: "button",
            disabled,
            aria_label: label,
            title,
            onclick: move |_| onclick.call(()),
            Icon { icon, size: 14 }
        }
    }
}
