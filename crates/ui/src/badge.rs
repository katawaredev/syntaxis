use dioxus::prelude::*;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Tone {
    #[default]
    Neutral,
    Success,
    Warning,
    Destructive,
}

impl Tone {
    pub(crate) const fn badge_class(self) -> &'static str {
        match self {
            Self::Neutral => "bg-secondary text-muted-foreground",
            Self::Success => "bg-success/10 text-success",
            Self::Warning => "bg-warning/10 text-warning",
            Self::Destructive => "bg-destructive/10 text-destructive",
        }
    }

    pub(crate) const fn dot_class(self) -> &'static str {
        match self {
            Self::Neutral => "bg-muted-foreground",
            Self::Success => "bg-success",
            Self::Warning => "bg-warning",
            Self::Destructive => "bg-destructive",
        }
    }
}

#[component]
pub fn StatusBadge(label: String, #[props(default)] tone: Tone) -> Element {
    rsx! {
        span { class: "inline-flex min-h-5 items-center whitespace-nowrap rounded-full border border-border px-2 py-px text-[10px] font-semibold tracking-wide {tone.badge_class()}",
            {label}
        }
    }
}
