//! Application-owned brand marks that are unavailable or unsuitable in the shared icon crates.

use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrandIcon {
    Dioxus,
    Shadcn,
    Tailwind,
    Vite,
}

#[component]
pub fn BrandMark(icon: BrandIcon, #[props(default = 24)] size: u32) -> Element {
    match icon {
        BrandIcon::Dioxus => rsx! {
            img {
                class: "object-contain",
                width: size,
                height: size,
                src: asset!("/assets/brands/dioxus_color.svg"),
                alt: "",
            }
        },
        BrandIcon::Shadcn => rsx! {
            svg {
                width: size,
                height: size,
                view_box: "0 0 24 24",
                fill: "currentColor",
                "aria-hidden": "true",
                path { d: "m19.01 11.55-7.46 7.46c-.46.46-.46 1.19 0 1.65a1.16 1.16 0 0 0 1.64 0l7.46-7.46c.46-.46.46-1.19 0-1.65s-1.19-.46-1.65 0ZM19.17 3.34c-.46-.46-1.19-.46-1.65 0L3.34 17.52c-.46.46-.46 1.19 0 1.65a1.16 1.16 0 0 0 1.64 0L19.16 4.99c.46-.46.46-1.19 0-1.65Z" }
            }
        },
        // The bundled devicon uses an SVG gradient reference that can fail to resolve when
        // several technology badges are rendered together. A solid mark stays crisp at 13 px.
        BrandIcon::Tailwind => rsx! {
            svg {
                width: size,
                height: size,
                view_box: "0 0 600 600",
                fill: "none",
                "aria-hidden": "true",
                path {
                    fill: "#38bdf8",
                    d: "M300 120q-120 0-150 120 45-60 105-45c22.8 5.7 39.1 22.3 57.2 40.6C341.6 265.4 375.6 300 450 300q120 0 150-120-45 60-105 45c-22.8-5.7-39.1-22.3-57.2-40.6C408.4 154.6 374.4 120 300 120M150 300Q30 300 0 420q45-60 105-45c22.8 5.7 39.1 22.3 57.2 40.6C191.6 445.4 225.6 480 300 480q120 0 150-120-45 60-105 45c-22.8-5.7-39.1-22.3-57.2-40.6C258.4 334.6 224.4 300 150 300",
                }
            }
        },
        // The bundled devicon uses gradients with unusually large coordinates. Some browsers
        // fail to rasterize it at badge sizes, so this solid version keeps the silhouette legible.
        BrandIcon::Vite => rsx! {
            svg {
                width: size,
                height: size,
                view_box: "0 0 600 600",
                fill: "none",
                "aria-hidden": "true",
                path {
                    fill: "#646cff",
                    d: "M597.6 88.8 316.1 592.2a15.3 15.3 0 0 1-26.6 0L2.5 89a15.3 15.3 0 0 1 16-22.7l281.7 50.4q2.7.5 5.4 0l276-50.3a15.3 15.3 0 0 1 16 22.5",
                }
                path {
                    fill: "#ffdd35",
                    d: "M434.4.1 226.1 41c-3.4.6-6 3.5-6.1 7l-13 216.4a7.8 7.8 0 0 0 9.4 8l58-13.4a7.6 7.6 0 0 1 9.2 9l-17.2 84.3a7.7 7.7 0 0 0 9.7 8.9l35.8-10.9a7.7 7.7 0 0 1 9.7 8.9l-27.3 132.5c-1.8 8.3 9.3 12.8 13.9 5.7l3-4.7L481 153.9a7.6 7.6 0 0 0-8.3-11L413 154.6a7.7 7.7 0 0 1-8.8-9.7l39-135a7.7 7.7 0 0 0-8.9-9.7",
                }
            }
        },
    }
}
