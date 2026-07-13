use dioxus::prelude::*;

#[component]
pub fn EmptyState(icon: String, title: String, description: String) -> Element {
    rsx! {
        div { class: "flex size-full flex-col items-center justify-center p-7 text-center",
            div {
                class: "mb-3.5 grid size-13.5 place-items-center rounded-2xl border border-border bg-card text-2xl text-muted-foreground",
                "aria-hidden": "true",
                {icon}
            }
            h2 { class: "text-lg font-semibold text-foreground", {title} }
            p { class: "mt-2 max-w-96 leading-relaxed text-muted-foreground", {description} }
        }
    }
}
