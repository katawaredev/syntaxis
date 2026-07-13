use dioxus::prelude::*;

use crate::{AppIcon, Icon, Tone};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum PanelHeaderKind {
    #[default]
    Tabs,
    Repository,
}

#[component]
pub fn PanelHeader(#[props(default)] kind: PanelHeaderKind, children: Element) -> Element {
    let class = match kind {
        PanelHeaderKind::Tabs => {
            "relative flex h-10 min-h-10 items-center gap-1.5 border-b border-border bg-background px-1.75 max-md:h-13 max-md:min-h-13 max-md:gap-1.75 max-[420px]:gap-0.75 max-[420px]:px-1"
        }
        PanelHeaderKind::Repository => {
            "flex min-h-13 items-center justify-between gap-2.5 border-b border-border bg-card py-1.75 pr-2 pl-3.25 max-md:pl-1.5"
        }
    };
    rsx! {
        header { class, {children} }
    }
}

#[component]
pub fn PanelTabList(children: Element) -> Element {
    rsx! {
        div {
            class: "flex h-8.5 min-w-0 flex-1 gap-0.5 overflow-x-auto bg-background [scrollbar-width:none] max-md:hidden",
            role: "tablist",
            {children}
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum PanelTabWidth {
    #[default]
    Content,
    Session,
}

impl PanelTabWidth {
    const fn class(self) -> &'static str {
        match self {
            Self::Content => "min-w-max",
            Self::Session => "min-w-33 max-w-47.5",
        }
    }

    const fn label_class(self) -> &'static str {
        match self {
            Self::Content => "",
            Self::Session => "flex-1 truncate text-left",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum PanelTabIndicator {
    Glyph(String),
    Dot(Tone),
}

#[component]
pub fn PanelTab(
    label: String,
    active: bool,
    #[props(default)] width: PanelTabWidth,
    #[props(default)] indicator: Option<PanelTabIndicator>,
    #[props(default = false)] dirty: bool,
    on_select: EventHandler<MouseEvent>,
    on_close: EventHandler<()>,
) -> Element {
    let active_class = if active {
        "border-transparent bg-muted text-foreground"
    } else {
        "border-border bg-background text-muted-foreground"
    };
    let label_for_close = label.clone();
    rsx! {
        div { class: "flex h-8.5 items-center gap-0.5 rounded-md border pr-0.75 text-[11px] {width.class()} {active_class}",
            button {
                class: "flex h-full min-w-0 flex-1 items-center gap-1.75 bg-transparent pr-1.25 pl-2.5 text-inherit",
                role: "tab",
                "aria-selected": active,
                onclick: move |event| on_select.call(event),
                if let Some(indicator) = indicator {
                    match indicator {
                        PanelTabIndicator::Glyph(glyph) => rsx! {
                            span { class: "text-[9px] font-extrabold text-primary", {glyph} }
                        },
                        PanelTabIndicator::Dot(tone) => rsx! {
                            span { class: "size-1.75 shrink-0 rounded-full {tone.dot_class()}" }
                        },
                    }
                }
                span { class: width.label_class(), {label} }
            }
            if dirty {
                span {
                    class: "size-1.75 rounded-full bg-foreground",
                    title: "Unsaved changes",
                    "aria-label": "Unsaved changes",
                }
            }
            button {
                class: "grid size-5.75 shrink-0 place-items-center rounded-sm bg-transparent text-muted-foreground hover:bg-accent hover:text-foreground",
                "aria-label": "Close {label_for_close}",
                title: "Close {label_for_close}",
                onclick: move |_| on_close.call(()),
                Icon { icon: AppIcon::Close, size: 12 }
            }
        }
    }
}
