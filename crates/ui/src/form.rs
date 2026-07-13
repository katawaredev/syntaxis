use dioxus::prelude::*;

#[component]
pub fn DialogForm(children: Element) -> Element {
    rsx! {
        div { class: "flex flex-col gap-2.25 px-5 pt-3 pb-5", {children} }
    }
}

#[component]
pub fn DialogActions(children: Element) -> Element {
    rsx! {
        div { class: "mt-2.5 flex justify-end gap-1.75", {children} }
    }
}

#[component]
pub fn DangerNote(message: String) -> Element {
    rsx! {
        p { class: "rounded-md border border-destructive/35 bg-destructive/10 px-2.5 py-2.25 text-xs leading-snug text-destructive",
            {message}
        }
    }
}
