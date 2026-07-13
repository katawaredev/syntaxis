use dioxus::prelude::*;
use syntaxis_workspace::{WorkspaceIcon, WorkspaceIconSymbol};

#[component]
pub fn ProjectIcon(icon: WorkspaceIcon, #[props(default = false)] compact: bool) -> Element {
    let class = if compact {
        "grid size-7 shrink-0 place-items-center overflow-hidden rounded-md bg-linear-to-br from-primary to-primary/60 text-[9px] font-bold text-primary-foreground"
    } else {
        "grid size-10 shrink-0 place-items-center overflow-hidden rounded-lg bg-linear-to-br from-primary to-primary/60 text-[10px] font-bold text-primary-foreground shadow-md"
    };
    rsx! {
        span { class,
            match icon {
                WorkspaceIcon::Image { data_url: Some(source), .. } => rsx! {
                    img { class: "size-full object-cover", src: source, alt: "" }
                },
                WorkspaceIcon::Image { data_url: None, .. } => rsx! { "IMG" },
                WorkspaceIcon::Symbol { name } => rsx! {
                    {symbol_label(name)}
                },
            }
        }
    }
}

fn symbol_label(symbol: WorkspaceIconSymbol) -> &'static str {
    match symbol {
        WorkspaceIconSymbol::Docker => "DK",
        WorkspaceIconSymbol::Folder => "DIR",
        WorkspaceIconSymbol::Git => "GIT",
        WorkspaceIconSymbol::Go => "GO",
        WorkspaceIconSymbol::Javascript => "JS",
        WorkspaceIconSymbol::Nextjs => "NXT",
        WorkspaceIconSymbol::Node => "NOD",
        WorkspaceIconSymbol::Python => "PY",
        WorkspaceIconSymbol::React => "RCT",
        WorkspaceIconSymbol::Rust => "RS",
        WorkspaceIconSymbol::Storybook => "SB",
        WorkspaceIconSymbol::Svelte => "SV",
        WorkspaceIconSymbol::Typescript => "TS",
        WorkspaceIconSymbol::Vercel => "VCL",
        WorkspaceIconSymbol::Vite => "VIT",
        WorkspaceIconSymbol::Vue => "VUE",
    }
}
