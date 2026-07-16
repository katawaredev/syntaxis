use dioxus::prelude::*;
use dioxus_devicons::devicons::{color, monochrome};
use dioxus_icons::lucide::{Atom, FileCode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectTemplateIcon {
    Empty,
    Rust,
    Dioxus,
    Dotnet,
    Vite,
    VitePlus,
    React,
    Tanstack,
    ReactRouter,
    Nextjs,
    Astro,
    Vue,
    Svelte,
    Solid,
    Nuxt,
    Hono,
    Fresh,
    Expo,
    Tauri,
    Python,
    Django,
    Go,
    Deno,
    Bun,
    Nodejs,
}

#[component]
pub fn TemplateIcon(icon: ProjectTemplateIcon, #[props(default = 24)] size: u32) -> Element {
    match icon {
        ProjectTemplateIcon::Empty => rsx! {
            FileCode { size, stroke_width: 1.7 }
        },
        ProjectTemplateIcon::Rust => rsx! {
            monochrome::Rust { size }
        },
        ProjectTemplateIcon::Dioxus => rsx! {
            Atom { size, stroke_width: 1.7 }
        },
        ProjectTemplateIcon::Dotnet => rsx! {
            color::Dotnet { size }
        },
        ProjectTemplateIcon::Vite | ProjectTemplateIcon::VitePlus => rsx! {
            color::Vite { size }
        },
        ProjectTemplateIcon::React => rsx! {
            color::React { size }
        },
        ProjectTemplateIcon::Tanstack => rsx! {
            color::ReactQuery { size }
        },
        ProjectTemplateIcon::ReactRouter => rsx! {
            color::ReactRouter { size }
        },
        ProjectTemplateIcon::Nextjs => rsx! {
            monochrome::NextjsIcon { size }
        },
        ProjectTemplateIcon::Astro => rsx! {
            monochrome::AstroIcon { size }
        },
        ProjectTemplateIcon::Vue => rsx! {
            color::Vue { size }
        },
        ProjectTemplateIcon::Svelte => rsx! {
            color::Svelte { size }
        },
        ProjectTemplateIcon::Solid => rsx! {
            color::Solidjs { size }
        },
        ProjectTemplateIcon::Nuxt => rsx! {
            color::Nuxt { size }
        },
        ProjectTemplateIcon::Hono => rsx! {
            color::Hono { size }
        },
        ProjectTemplateIcon::Fresh => rsx! {
            color::Fresh { size }
        },
        ProjectTemplateIcon::Expo => rsx! {
            monochrome::Expo { size }
        },
        ProjectTemplateIcon::Tauri => rsx! {
            color::Tauri { size }
        },
        ProjectTemplateIcon::Python => rsx! {
            color::Python { size }
        },
        ProjectTemplateIcon::Django => rsx! {
            color::Django { size }
        },
        ProjectTemplateIcon::Go => rsx! {
            color::Go { size }
        },
        ProjectTemplateIcon::Deno => rsx! {
            monochrome::Deno { size }
        },
        ProjectTemplateIcon::Bun => rsx! {
            monochrome::Bun { size }
        },
        ProjectTemplateIcon::Nodejs => rsx! {
            color::NodejsIcon { size }
        },
    }
}
