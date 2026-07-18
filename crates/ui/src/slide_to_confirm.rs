use dioxus::prelude::*;

use crate::Tone;

impl Tone {
    const fn slide_container_class(self, disabled: bool) -> &'static str {
        match (self, disabled) {
            (Self::Neutral, true) => "border-border bg-muted/10 opacity-50",
            (Self::Neutral, false) => {
                "border-border bg-muted/20 focus-within:ring-muted-foreground/40"
            }
            (Self::Success, true) => "border-success/25 bg-success/5 opacity-50",
            (Self::Success, false) => "border-success/40 bg-success/8 focus-within:ring-success/50",
            (Self::Warning, true) => "border-warning/25 bg-warning/5 opacity-50",
            (Self::Warning, false) => "border-warning/40 bg-warning/8 focus-within:ring-warning/50",
            (Self::Destructive, true) => "border-destructive/25 bg-destructive/5 opacity-50",
            (Self::Destructive, false) => {
                "border-destructive/40 bg-destructive/8 focus-within:ring-destructive/50"
            }
        }
    }

    const fn slide_fill_class(self) -> &'static str {
        match self {
            Self::Neutral => "bg-muted-foreground/20",
            Self::Success => "bg-success/25",
            Self::Warning => "bg-warning/25",
            Self::Destructive => "bg-destructive/25",
        }
    }

    const fn slide_text_class(self) -> &'static str {
        match self {
            Self::Neutral => "text-muted-foreground",
            Self::Success => "text-success",
            Self::Warning => "text-warning",
            Self::Destructive => "text-destructive",
        }
    }

    const fn slide_thumb_class(self, confirmed: bool) -> &'static str {
        match (self, confirmed) {
            (Self::Neutral, false) => "border-border bg-background text-muted-foreground",
            (Self::Neutral, true) => "border-muted-foreground bg-muted-foreground text-background",
            (Self::Success, false) => "border-success/50 bg-background text-success",
            (Self::Success, true) => "border-success bg-success text-white",
            (Self::Warning, false) => "border-warning/50 bg-background text-warning",
            (Self::Warning, true) => "border-warning bg-warning text-background",
            (Self::Destructive, false) => "border-destructive/50 bg-background text-destructive",
            (Self::Destructive, true) => "border-destructive bg-destructive text-white",
        }
    }
}

fn reset_incomplete(
    mut progress: Signal<u8>,
    mut reset_key: Signal<u32>,
    on_confirmed: EventHandler<bool>,
) {
    if progress() < 100 {
        let next_reset_key = reset_key().wrapping_add(1);
        progress.set(0);
        reset_key.set(next_reset_key);
        on_confirmed.call(false);
    }
}

#[component]
pub fn SlideToConfirm(
    label: String,
    confirmed_label: String,
    #[props(default)] tone: Tone,
    #[props(default)] disabled: bool,
    #[props(default)] class: String,
    on_confirmed: EventHandler<bool>,
) -> Element {
    let mut progress = use_signal(|| 0_u8);
    let reset_key = use_signal(|| 0_u32);
    let confirmed = progress() == 100;
    let thumb_offset = f32::from(progress()) * 0.48 - 4.0;

    rsx! {
        div { class: "relative h-13 overflow-hidden rounded-lg border focus-within:ring-2 {tone.slide_container_class(disabled)} {class}",
            div {
                class: "pointer-events-none absolute inset-y-0 left-0 transition-[width] duration-75 {tone.slide_fill_class()}",
                width: "{progress}%",
            }
            span { class: "pointer-events-none absolute inset-0 flex items-center justify-center px-13 text-xs font-medium {tone.slide_text_class()}",
                if confirmed {
                    {confirmed_label.clone()}
                } else {
                    {label.clone()}
                }
            }
            span { class: "pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 font-mono text-[10px] {tone.slide_text_class()}",
                "{progress}%"
            }
            span {
                class: "pointer-events-none absolute top-1 left-0 grid size-11 place-items-center rounded-md border font-bold shadow-sm transition-[left] duration-75 {tone.slide_thumb_class(confirmed)}",
                left: "calc({progress}% - {thumb_offset}px)",
                "aria-hidden": "true",
                if confirmed {
                    "✓"
                } else {
                    "→"
                }
            }
            input {
                key: "slide-to-confirm-{reset_key}",
                class: "absolute inset-0 z-10 size-full cursor-ew-resize opacity-0 disabled:cursor-not-allowed",
                r#type: "range",
                min: "0",
                max: "100",
                step: "1",
                initial_value: "0",
                disabled,
                aria_label: label.clone(),
                "aria-valuetext": if confirmed { confirmed_label.clone() } else { label.clone() },
                oninput: move |event: FormEvent| {
                    let next = event.value().parse::<u8>().unwrap_or_default().min(100);
                    let was_confirmed = progress() == 100;
                    progress.set(next);
                    if (next == 100) != was_confirmed {
                        on_confirmed.call(next == 100);
                    }
                },
                onchange: move |_| reset_incomplete(progress, reset_key, on_confirmed),
                onpointercancel: move |_| reset_incomplete(progress, reset_key, on_confirmed),
                ontouchcancel: move |_| reset_incomplete(progress, reset_key, on_confirmed),
                onblur: move |_| reset_incomplete(progress, reset_key, on_confirmed),
            }
        }
    }
}
