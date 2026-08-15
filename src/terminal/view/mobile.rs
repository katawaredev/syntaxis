use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MobileTerminalKey {
    label: &'static str,
    accessible_label: &'static str,
    input: &'static str,
    wide: bool,
}

const MOBILE_TERMINAL_KEYS: [MobileTerminalKey; 8] = [
    MobileTerminalKey {
        label: "Esc",
        accessible_label: "Escape",
        input: "\u{1b}",
        wide: false,
    },
    MobileTerminalKey {
        label: "Tab",
        accessible_label: "Tab",
        input: "\t",
        wide: false,
    },
    MobileTerminalKey {
        label: "←",
        accessible_label: "Left arrow",
        input: "\u{1b}[D",
        wide: false,
    },
    MobileTerminalKey {
        label: "↑",
        accessible_label: "Up arrow",
        input: "\u{1b}[A",
        wide: false,
    },
    MobileTerminalKey {
        label: "↓",
        accessible_label: "Down arrow",
        input: "\u{1b}[B",
        wide: false,
    },
    MobileTerminalKey {
        label: "→",
        accessible_label: "Right arrow",
        input: "\u{1b}[C",
        wide: false,
    },
    MobileTerminalKey {
        label: "Space",
        accessible_label: "Space",
        input: " ",
        wide: true,
    },
    MobileTerminalKey {
        label: "↵",
        accessible_label: "Enter",
        input: "\r",
        wide: false,
    },
];

#[component]
pub(super) fn MobileTerminalKeys(
    mut ctrl: Signal<bool>,
    on_input: EventHandler<Vec<u8>>,
    on_focus: EventHandler<()>,
) -> Element {
    rsx! {
        nav {
            class: "terminal-mobile-keys min-h-11 shrink-0 items-center gap-1 overflow-x-auto border-t border-border bg-background px-1.5 pt-1 pb-[max(0.25rem,env(safe-area-inset-bottom))] [scrollbar-width:none]",
            "aria-label": "Terminal keys",
            button {
                r#type: "button",
                class: if ctrl() { "min-h-9 min-w-11 shrink-0 touch-manipulation rounded-md border border-primary bg-primary/15 px-2 font-mono text-xs font-semibold text-primary" } else { "min-h-9 min-w-11 shrink-0 touch-manipulation rounded-md border border-border bg-card px-2 font-mono text-xs font-medium text-foreground active:bg-accent" },
                "aria-label": "Control modifier for the next key",
                "aria-pressed": ctrl(),
                onpointerdown: move |event| event.prevent_default(),
                onclick: move |_| {
                    ctrl.toggle();
                    on_focus.call(());
                },
                "Ctrl"
            }
            for key in MOBILE_TERMINAL_KEYS {
                button {
                    r#type: "button",
                    class: if key.wide { "min-h-9 min-w-15 shrink-0 touch-manipulation rounded-md border border-border bg-card px-2 font-mono text-xs font-medium text-foreground active:bg-accent" } else { "min-h-9 min-w-10 shrink-0 touch-manipulation rounded-md border border-border bg-card px-2 font-mono text-xs font-medium text-foreground active:bg-accent" },
                    "aria-label": key.accessible_label,
                    onpointerdown: move |event| event.prevent_default(),
                    onclick: move |_| {
                        on_input.call(key.input.as_bytes().to_vec());
                        on_focus.call(());
                    },
                    {key.label}
                }
            }
        }
    }
}

pub(super) fn ctrl_modified_byte(data: &[u8]) -> Option<u8> {
    let [byte] = data else {
        return None;
    };
    match byte {
        b' ' | b'@' => Some(0),
        b'a'..=b'z' | b'A'..=b'Z' | b'['..=b'_' => Some(byte & 0x1f),
        b'?' => Some(0x7f),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobile_control_modifier_encodes_terminal_control_bytes() {
        assert_eq!(ctrl_modified_byte(b"c"), Some(3));
        assert_eq!(ctrl_modified_byte(b"D"), Some(4));
        assert_eq!(ctrl_modified_byte(b"z"), Some(26));
        assert_eq!(ctrl_modified_byte(b"["), Some(27));
        assert_eq!(ctrl_modified_byte(b" "), Some(0));
        assert_eq!(ctrl_modified_byte(b"?"), Some(127));
        assert_eq!(ctrl_modified_byte(b"\x1b[A"), None);
        assert_eq!(ctrl_modified_byte("é".as_bytes()), None);
    }
}
