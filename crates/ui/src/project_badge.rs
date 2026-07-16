use dioxus::prelude::*;
use dioxus_devicons::devicons::{color, monochrome};
use dioxus_icons::lucide::{Atom, Code};
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

#[allow(clippy::too_many_lines)]
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
            Atom { size, stroke_width: 1.8 }
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
            Code { size, stroke_width: 1.8 }
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
            color::Vite { size }
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

#[allow(clippy::too_many_lines)]
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
