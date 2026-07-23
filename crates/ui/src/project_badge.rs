use dioxus::prelude::*;
use dioxus_devicons::devicons::{color, monochrome};
use dioxus_icons::lucide::Code;
use syntaxis_workspace::{WorkspaceLanguage, WorkspaceTechnology};

#[component]
pub fn ProjectTechnologyBadge(
    technology: WorkspaceTechnology,
    #[props(default = false)] large: bool,
    #[props(default)] class: String,
) -> Element {
    let label = technology.label();
    let size_class = if large { "size-7" } else { "size-5.5" };
    let icon_size = if large { 16 } else { 13 };
    rsx! {
        span {
            class: "project-badge inline-grid {size_class} shrink-0 place-items-center rounded-md border border-border/60 bg-muted/35 {class}",
            title: label,
            "aria-label": label,
            {technology_icon(technology, icon_size)}
        }
    }
}

#[component]
pub fn ProjectLanguageBadge(
    language: WorkspaceLanguage,
    total_bytes: u64,
    #[props(default = false)] large: bool,
    #[props(default)] class: String,
) -> Element {
    let permille = language
        .bytes
        .saturating_mul(1_000)
        .checked_div(total_bytes)
        .unwrap_or_default();
    let label = format!("{} · {}.{}%", language.name, permille / 10, permille % 10);
    let size_class = if large { "size-7" } else { "size-5.5" };
    let icon_size = if large { 16 } else { 13 };
    rsx! {
        span {
            class: "project-badge inline-grid {size_class} shrink-0 place-items-center rounded-md border border-border/60 bg-muted/35 {class}",
            title: label.clone(),
            "aria-label": label,
            {language_icon(&language.name, icon_size)}
        }
    }
}

#[expect(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    reason = "the technology-to-icon catalog is clearer as one declarative match"
)]
fn technology_icon(technology: WorkspaceTechnology, size: u32) -> Element {
    match technology {
        WorkspaceTechnology::Angular => rsx! {
            color::AngularIcon { size }
        },
        WorkspaceTechnology::Astro => rsx! {
            monochrome::AstroIcon { size }
        },
        WorkspaceTechnology::Biome => rsx! {
            color::Biomejs { size }
        },
        WorkspaceTechnology::Bun => rsx! {
            monochrome::Bun { size }
        },
        WorkspaceTechnology::Composer => rsx! {
            color::Composer { size }
        },
        WorkspaceTechnology::Deno => rsx! {
            monochrome::Deno { size }
        },
        WorkspaceTechnology::Django => rsx! {
            color::DjangoIcon { size }
        },
        WorkspaceTechnology::Dioxus => rsx! {
            img {
                class: "object-contain",
                width: size,
                height: size,
                src: asset!("/assets/dioxus_color.svg"),
                alt: "",
            }
        },
        WorkspaceTechnology::Docker => rsx! {
            color::DockerIcon { size }
        },
        WorkspaceTechnology::Eslint => rsx! {
            color::Eslint { size }
        },
        WorkspaceTechnology::Fastapi => rsx! {
            color::FastapiIcon { size }
        },
        WorkspaceTechnology::Firebase => rsx! {
            color::FirebaseIcon { size }
        },
        WorkspaceTechnology::GithubActions => rsx! {
            color::GithubActions { size }
        },
        WorkspaceTechnology::Graphql => rsx! {
            color::Graphql { size }
        },
        WorkspaceTechnology::Jest => rsx! {
            color::Jest { size }
        },
        WorkspaceTechnology::Just => rsx! {
            JustBadgeIcon {}
        },
        WorkspaceTechnology::Mongodb => rsx! {
            color::MongodbIcon { size }
        },
        WorkspaceTechnology::Nextjs => rsx! {
            monochrome::NextjsIcon { size }
        },
        WorkspaceTechnology::Nginx => rsx! {
            color::Nginx { size }
        },
        WorkspaceTechnology::Nodejs => rsx! {
            color::NodejsIcon { size }
        },
        WorkspaceTechnology::Npm => rsx! {
            color::NpmIcon { size }
        },
        WorkspaceTechnology::Playwright => rsx! {
            color::Playwright { size }
        },
        WorkspaceTechnology::Pnpm => rsx! {
            color::Pnpm { size }
        },
        WorkspaceTechnology::Postgresql => rsx! {
            color::Postgresql { size }
        },
        WorkspaceTechnology::Prettier => rsx! {
            color::Prettier { size }
        },
        WorkspaceTechnology::Prisma => rsx! {
            monochrome::Prisma { size }
        },
        WorkspaceTechnology::React => rsx! {
            color::React { size }
        },
        WorkspaceTechnology::Redis => rsx! {
            color::RedisIcon { size }
        },
        WorkspaceTechnology::Rollup => rsx! {
            color::Rollupjs { size }
        },
        WorkspaceTechnology::Sqlite => rsx! {
            color::SqliteIcon { size }
        },
        WorkspaceTechnology::Storybook => rsx! {
            color::StorybookIcon { size }
        },
        WorkspaceTechnology::Supabase => rsx! {
            color::SupabaseIcon { size }
        },
        WorkspaceTechnology::Svelte => rsx! {
            color::SvelteIcon { size }
        },
        WorkspaceTechnology::Tailwind => rsx! {
            color::TailwindIcon { size }
        },
        WorkspaceTechnology::Terraform => rsx! {
            color::TerraformIcon { size }
        },
        WorkspaceTechnology::Vercel => rsx! {
            monochrome::VercelIcon { size }
        },
        WorkspaceTechnology::Vite => rsx! {
            ViteBadgeIcon { size }
        },
        WorkspaceTechnology::Vitest => rsx! {
            color::Vitest { size }
        },
        WorkspaceTechnology::Vue => rsx! {
            color::Vue { size }
        },
        WorkspaceTechnology::Webpack => rsx! {
            color::Webpack { size }
        },
        WorkspaceTechnology::Yarn => rsx! {
            color::Yarn { size }
        },
    }
}

#[component]
fn JustBadgeIcon() -> Element {
    rsx! {
        span {
            class: "inline-flex h-2.75 min-w-4.5 items-center justify-center rounded-[2px] bg-[#62666d] px-0.5 font-mono text-[6px] font-bold leading-none tracking-[-.04em] text-[#f4f4f5]",
            "aria-hidden": "true",
            "just"
        }
    }
}

// The bundled Vite icon uses gradients with unusually large coordinates. Some browsers fail to
// rasterize them at the 13–16 px badge size, although zooming makes the same SVG appear. This
// solid-color version keeps the recognizable silhouette and remains reliable at small sizes.
#[component]
fn ViteBadgeIcon(size: u32) -> Element {
    rsx! {
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
    }
}

#[expect(
    clippy::cognitive_complexity,
    reason = "the language-to-icon catalog is clearer as one declarative match"
)]
fn language_icon(language: &str, size: u32) -> Element {
    match language.to_ascii_lowercase().as_str() {
        "c" => rsx! {
            color::C { size }
        },
        "c++" => rsx! {
            color::CPlusplus { size }
        },
        "c#" => rsx! {
            color::CSharp { size }
        },
        "css" | "scss" | "sass" => rsx! {
            color::Css3 { size }
        },
        "dart" => rsx! {
            color::Dart { size }
        },
        "elixir" => rsx! {
            color::Elixir { size }
        },
        "go" => rsx! {
            color::Go { size }
        },
        "haskell" => rsx! {
            color::Haskell { size }
        },
        "html" => rsx! {
            monochrome::Html5 { size }
        },
        "java" => rsx! {
            color::Java { size }
        },
        "javascript" => rsx! {
            color::Javascript { size }
        },
        "kotlin" => rsx! {
            color::Kotlin { size }
        },
        "lua" => rsx! {
            color::Lua { size }
        },
        "objective-c" | "objective-c++" => rsx! {
            color::C { size }
        },
        "ocaml" => rsx! {
            color::Ocaml { size }
        },
        "perl" => rsx! {
            color::Perl { size }
        },
        "php" => rsx! {
            color::Php { size }
        },
        "python" => rsx! {
            color::Python { size }
        },
        "r" => rsx! {
            color::RLang { size }
        },
        "ruby" => rsx! {
            color::Ruby { size }
        },
        "rust" => rsx! {
            monochrome::Rust { size }
        },
        "scala" => rsx! {
            color::Scala { size }
        },
        "shell" => rsx! {
            color::Bash { size }
        },
        "solidity" => rsx! {
            monochrome::Solidity { size }
        },
        "swift" => rsx! {
            color::Swift { size }
        },
        "typescript" => rsx! {
            color::TypescriptIcon { size }
        },
        "webassembly" => rsx! {
            color::Webassembly { size }
        },
        "zig" => rsx! {
            color::Zig { size }
        },
        _ => rsx! {
            Code { size, stroke_width: 1.8 }
        },
    }
}
