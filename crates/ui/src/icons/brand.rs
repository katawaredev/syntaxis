//! Application-owned brand marks that are unavailable or unsuitable in the shared icon crates.

use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrandIcon {
    Dioxus,
    OpenAi,
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
        BrandIcon::OpenAi => rsx! {
            svg {
                width: size,
                height: size,
                view_box: "0 0 256 260",
                fill: "currentColor",
                "aria-hidden": "true",
                path {
                    d: "M239.184 106.203a64.716 64.716 0 0 0-5.576-53.103C219.452 28.459 191 15.784 163.213 21.74A65.586 65.586 0 0 0 52.096 45.22a64.716 64.716 0 0 0-43.23 31.36c-14.31 24.602-11.061 55.634 8.033 76.74a64.665 64.665 0 0 0 5.525 53.102c14.174 24.65 42.644 37.324 70.446 31.36a64.72 64.72 0 0 0 48.754 21.744c28.481.025 53.714-18.361 62.414-45.481a64.767 64.767 0 0 0 43.229-31.36c14.137-24.558 10.875-55.423-8.083-76.483Zm-97.56 136.338a48.397 48.397 0 0 1-31.105-11.255l53.205-30.695a8.595 8.595 0 0 0 4.247-7.367v-72.85l21.845 12.636v60.93c-.056 26.818-21.783 48.545-48.601 48.601Zm-104.466-44.61a48.345 48.345 0 0 1-5.781-32.589l53.256 30.747a8.339 8.339 0 0 0 8.441 0l63.181-36.425v25.221l-52.693 30.849c-23.257 13.398-52.97 5.431-66.404-17.803ZM23.549 85.38a48.499 48.499 0 0 1 25.58-21.333v61.39a8.288 8.288 0 0 0 4.195 7.316l62.874 36.272-21.845 12.636-53-30.131c-23.211-13.454-31.171-43.144-17.804-66.405Zm179.466 41.695-63.08-36.63 21.795-12.585 53.001 30.184a48.6 48.6 0 0 1-7.316 87.635v-61.391a8.544 8.544 0 0 0-4.4-7.213Zm21.742-32.69-53.154-31.003a8.39 8.39 0 0 0-8.492 0L99.98 99.808V74.587l52.54-30.798a48.652 48.652 0 0 1 72.236 50.391ZM88.061 139.097l-21.845-12.585V65.685a48.652 48.652 0 0 1 79.757-37.346l-53.615 30.695a8.595 8.595 0 0 0-4.246 7.367l-.051 72.697Zm11.868-25.58 28.138-16.217 28.188 16.218v32.434l-28.086 16.218-28.188-16.218-.052-32.434Z",
                }
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
