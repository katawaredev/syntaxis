use dioxus::prelude::*;
use dioxus_devicons::devicons::{color, monochrome};
use dioxus_icons::lucide::{
    Atom, File, FileArchive, FileCode, FileCog, FileImage, FileLock, FileMusic, FileSymlink,
    FileTerminal, FileText, FileVideoCamera, Folder, FolderOpen,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FileIconGlyph {
    Folder,
    FolderOpen,
    Symlink,
    Generic,
    Text,
    Code,
    Image,
    Audio,
    Video,
    Archive,
    Database,
    Lock,
    Config,
    Terminal,
    Dioxus,
    Angular,
    Astro,
    Biome,
    Bun,
    C,
    Cpp,
    CSharp,
    Composer,
    Css,
    Deno,
    Docker,
    EditorConfig,
    Eslint,
    Firebase,
    Git,
    GithubActions,
    Go,
    Graphql,
    Html,
    Java,
    JavaScript,
    Jest,
    Json,
    Kotlin,
    Less,
    Lua,
    Markdown,
    Mongodb,
    Nextjs,
    Nginx,
    Nodejs,
    Npm,
    Php,
    Playwright,
    Pnpm,
    Postgresql,
    Prettier,
    Prisma,
    Python,
    R,
    React,
    Redis,
    Rollup,
    Ruby,
    Rust,
    Sass,
    Scala,
    Solidity,
    Sqlite,
    Storybook,
    Supabase,
    Svelte,
    Svg,
    Swift,
    Tailwind,
    Terraform,
    Toml,
    TypeScript,
    Vercel,
    Vite,
    Vitest,
    Vue,
    Wasm,
    Webpack,
    Yaml,
    Yarn,
}

/// A compact, path-aware icon for workspace and Git file lists.
#[component]
pub fn FileIcon(
    path: String,
    #[props(default = false)] directory: bool,
    #[props(default = false)] expanded: bool,
    #[props(default = false)] symlink: bool,
    #[props(default = 16)] size: u32,
) -> Element {
    let glyph = if directory {
        if expanded {
            FileIconGlyph::FolderOpen
        } else {
            FileIconGlyph::Folder
        }
    } else if symlink {
        FileIconGlyph::Symlink
    } else {
        file_icon_glyph(&path)
    };
    let tone = glyph_tone(glyph);

    rsx! {
        span {
            class: "inline-grid shrink-0 place-items-center {tone}",
            style: "width: {size}px; height: {size}px",
            "aria-hidden": true,
            {render_glyph(glyph, size)}
        }
    }
}

// This is intentionally one exhaustive visual catalog: keeping every glyph-to-component
// association together makes missing and duplicate brand mappings obvious during review.
#[expect(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    reason = "the exhaustive glyph catalog is clearer as one declarative match"
)]
fn render_glyph(glyph: FileIconGlyph, size: u32) -> Element {
    match glyph {
        FileIconGlyph::Folder => rsx! {
            Folder { size, stroke_width: 1.75 }
        },
        FileIconGlyph::FolderOpen => rsx! {
            FolderOpen { size, stroke_width: 1.75 }
        },
        FileIconGlyph::Symlink => rsx! {
            FileSymlink { size, stroke_width: 1.75 }
        },
        FileIconGlyph::Generic => rsx! {
            File { size, stroke_width: 1.75 }
        },
        FileIconGlyph::Text => rsx! {
            FileText { size, stroke_width: 1.75 }
        },
        FileIconGlyph::Code => rsx! {
            FileCode { size, stroke_width: 1.75 }
        },
        FileIconGlyph::Image => rsx! {
            FileImage { size, stroke_width: 1.75 }
        },
        FileIconGlyph::Audio => rsx! {
            FileMusic { size, stroke_width: 1.75 }
        },
        FileIconGlyph::Video => rsx! {
            FileVideoCamera { size, stroke_width: 1.75 }
        },
        FileIconGlyph::Archive => rsx! {
            FileArchive { size, stroke_width: 1.75 }
        },
        FileIconGlyph::Database => {
            rsx! {
                dioxus_icons::lucide::Database { size, stroke_width: 1.75 }
            }
        }
        FileIconGlyph::Lock => rsx! {
            FileLock { size, stroke_width: 1.75 }
        },
        FileIconGlyph::Config => rsx! {
            FileCog { size, stroke_width: 1.75 }
        },
        FileIconGlyph::Terminal => rsx! {
            FileTerminal { size, stroke_width: 1.75 }
        },
        FileIconGlyph::Dioxus => rsx! {
            Atom { size, stroke_width: 1.75 }
        },
        FileIconGlyph::Angular => rsx! {
            color::AngularIcon { size }
        },
        FileIconGlyph::Astro => rsx! {
            monochrome::AstroIcon { size }
        },
        FileIconGlyph::Biome => rsx! {
            color::Biomejs { size }
        },
        FileIconGlyph::Bun => rsx! {
            monochrome::Bun { size }
        },
        FileIconGlyph::C => rsx! {
            color::C { size }
        },
        FileIconGlyph::Cpp => rsx! {
            color::CPlusplus { size }
        },
        FileIconGlyph::CSharp => rsx! {
            color::CSharp { size }
        },
        FileIconGlyph::Composer => rsx! {
            color::Composer { size }
        },
        FileIconGlyph::Css => rsx! {
            color::Css3 { size }
        },
        FileIconGlyph::Deno => rsx! {
            monochrome::Deno { size }
        },
        FileIconGlyph::Docker => rsx! {
            color::DockerIcon { size }
        },
        FileIconGlyph::EditorConfig => rsx! {
            color::Editorconfig { size }
        },
        FileIconGlyph::Eslint => rsx! {
            color::Eslint { size }
        },
        FileIconGlyph::Firebase => rsx! {
            color::FirebaseIcon { size }
        },
        FileIconGlyph::Git => rsx! {
            color::GitIcon { size }
        },
        FileIconGlyph::GithubActions => rsx! {
            color::GithubActions { size }
        },
        FileIconGlyph::Go => rsx! {
            color::Go { size }
        },
        FileIconGlyph::Graphql => rsx! {
            color::Graphql { size }
        },
        FileIconGlyph::Html => rsx! {
            monochrome::Html5 { size }
        },
        FileIconGlyph::Java => rsx! {
            color::Java { size }
        },
        FileIconGlyph::JavaScript => rsx! {
            color::Javascript { size }
        },
        FileIconGlyph::Jest => rsx! {
            color::Jest { size }
        },
        FileIconGlyph::Json => rsx! {
            monochrome::Json { size }
        },
        FileIconGlyph::Kotlin => rsx! {
            color::KotlinIcon { size }
        },
        FileIconGlyph::Less => rsx! {
            color::Less { size }
        },
        FileIconGlyph::Lua => rsx! {
            color::Lua { size }
        },
        FileIconGlyph::Markdown => rsx! {
            monochrome::Markdown { size }
        },
        FileIconGlyph::Mongodb => rsx! {
            color::MongodbIcon { size }
        },
        FileIconGlyph::Nextjs => rsx! {
            monochrome::NextjsIcon { size }
        },
        FileIconGlyph::Nginx => rsx! {
            color::Nginx { size }
        },
        FileIconGlyph::Nodejs => rsx! {
            color::NodejsIcon { size }
        },
        FileIconGlyph::Npm => rsx! {
            color::NpmIcon { size }
        },
        FileIconGlyph::Php => rsx! {
            color::Php { size }
        },
        FileIconGlyph::Playwright => rsx! {
            color::Playwright { size }
        },
        FileIconGlyph::Pnpm => rsx! {
            color::Pnpm { size }
        },
        FileIconGlyph::Postgresql => rsx! {
            color::Postgresql { size }
        },
        FileIconGlyph::Prettier => rsx! {
            color::Prettier { size }
        },
        FileIconGlyph::Prisma => rsx! {
            monochrome::Prisma { size }
        },
        FileIconGlyph::Python => rsx! {
            color::Python { size }
        },
        FileIconGlyph::R => rsx! {
            color::RLang { size }
        },
        FileIconGlyph::React => rsx! {
            color::React { size }
        },
        FileIconGlyph::Redis => rsx! {
            color::RedisIcon { size }
        },
        FileIconGlyph::Rollup => rsx! {
            color::Rollupjs { size }
        },
        FileIconGlyph::Ruby => rsx! {
            color::Ruby { size }
        },
        FileIconGlyph::Rust => rsx! {
            monochrome::Rust { size }
        },
        FileIconGlyph::Sass => rsx! {
            color::Sass { size }
        },
        FileIconGlyph::Scala => rsx! {
            color::Scala { size }
        },
        FileIconGlyph::Solidity => rsx! {
            monochrome::Solidity { size }
        },
        FileIconGlyph::Sqlite => rsx! {
            color::SqliteIcon { size }
        },
        FileIconGlyph::Storybook => rsx! {
            color::StorybookIcon { size }
        },
        FileIconGlyph::Supabase => rsx! {
            color::SupabaseIcon { size }
        },
        FileIconGlyph::Svelte => rsx! {
            color::SvelteIcon { size }
        },
        FileIconGlyph::Svg => rsx! {
            color::Svg { size }
        },
        FileIconGlyph::Swift => rsx! {
            color::Swift { size }
        },
        FileIconGlyph::Tailwind => rsx! {
            color::TailwindIcon { size }
        },
        FileIconGlyph::Terraform => rsx! {
            color::TerraformIcon { size }
        },
        FileIconGlyph::Toml => rsx! {
            monochrome::Toml { size }
        },
        FileIconGlyph::TypeScript => rsx! {
            color::TypescriptIcon { size }
        },
        FileIconGlyph::Vercel => rsx! {
            monochrome::VercelIcon { size }
        },
        FileIconGlyph::Vite => rsx! {
            color::Vite { size }
        },
        FileIconGlyph::Vitest => rsx! {
            color::Vitest { size }
        },
        FileIconGlyph::Vue => rsx! {
            color::Vue { size }
        },
        FileIconGlyph::Wasm => rsx! {
            color::Webassembly { size }
        },
        FileIconGlyph::Webpack => rsx! {
            color::Webpack { size }
        },
        FileIconGlyph::Yaml => rsx! {
            color::Yaml { size }
        },
        FileIconGlyph::Yarn => rsx! {
            color::Yarn { size }
        },
    }
}

fn glyph_tone(glyph: FileIconGlyph) -> &'static str {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only generic glyphs need a fallback color; branded glyphs carry their own colors"
    )]
    match glyph {
        FileIconGlyph::Folder | FileIconGlyph::FolderOpen => "text-warning",
        FileIconGlyph::Symlink | FileIconGlyph::Code => "text-sky-400",
        FileIconGlyph::Generic | FileIconGlyph::Text => "text-muted-foreground",
        FileIconGlyph::Image => "text-fuchsia-400",
        FileIconGlyph::Audio => "text-violet-400",
        FileIconGlyph::Video => "text-rose-400",
        FileIconGlyph::Archive | FileIconGlyph::Lock => "text-amber-400",
        FileIconGlyph::Database | FileIconGlyph::Dioxus => "text-cyan-400",
        FileIconGlyph::Config | FileIconGlyph::Solidity => "text-slate-400",
        FileIconGlyph::Terminal => "text-emerald-400",
        FileIconGlyph::Astro | FileIconGlyph::Bun | FileIconGlyph::Json | FileIconGlyph::Toml => {
            "text-amber-300"
        }
        FileIconGlyph::Deno | FileIconGlyph::Markdown | FileIconGlyph::Prisma => "text-slate-300",
        FileIconGlyph::Html | FileIconGlyph::Rust => "text-orange-400",
        FileIconGlyph::Nextjs | FileIconGlyph::Vercel => "text-foreground",
        _ => "",
    }
}

fn file_icon_glyph(path: &str) -> FileIconGlyph {
    let path = path.replace('\\', "/").to_ascii_lowercase();
    let name = path.rsplit('/').next().unwrap_or(path.as_str());

    if path.starts_with(".github/workflows/") || path.contains("/.github/workflows/") {
        return FileIconGlyph::GithubActions;
    }

    if let Some(glyph) = special_filename_glyph(name) {
        return glyph;
    }

    let extension = name.rsplit_once('.').map(|(_, extension)| extension);
    extension_glyph(extension).unwrap_or(FileIconGlyph::Generic)
}

fn special_filename_glyph(name: &str) -> Option<FileIconGlyph> {
    let exact = match name {
        "dioxus.toml" => FileIconGlyph::Dioxus,
        "cargo.toml"
        | "cargo.lock"
        | "rust-toolchain"
        | "rust-toolchain.toml"
        | "rustfmt.toml"
        | "clippy.toml" => FileIconGlyph::Rust,
        "package.json" | "package-lock.json" | "npm-shrinkwrap.json" | ".npmrc" => {
            FileIconGlyph::Npm
        }
        ".node-version" | ".nvmrc" => FileIconGlyph::Nodejs,
        "pnpm-lock.yaml" | "pnpm-workspace.yaml" | ".pnpmfile.cjs" => FileIconGlyph::Pnpm,
        "yarn.lock" | ".yarnrc" | ".yarnrc.yml" => FileIconGlyph::Yarn,
        "bun.lock" | "bun.lockb" | "bunfig.toml" => FileIconGlyph::Bun,
        "deno.json" | "deno.jsonc" | "deno.lock" => FileIconGlyph::Deno,
        "biome.json" | "biome.jsonc" => FileIconGlyph::Biome,
        ".eslintrc" | ".eslintignore" => FileIconGlyph::Eslint,
        ".prettierrc" | ".prettierignore" => FileIconGlyph::Prettier,
        "angular.json" => FileIconGlyph::Angular,
        "composer.json" | "composer.lock" => FileIconGlyph::Composer,
        "dockerfile"
        | ".dockerignore"
        | "compose.yaml"
        | "compose.yml"
        | "docker-compose.yaml"
        | "docker-compose.yml" => FileIconGlyph::Docker,
        ".editorconfig" => FileIconGlyph::EditorConfig,
        ".gitignore" | ".gitattributes" | ".gitmodules" | ".gitconfig" => FileIconGlyph::Git,
        "firebase.json" | ".firebaserc" => FileIconGlyph::Firebase,
        "vercel.json" => FileIconGlyph::Vercel,
        "netlify.toml" => FileIconGlyph::Config,
        "nginx.conf" => FileIconGlyph::Nginx,
        "schema.prisma" => FileIconGlyph::Prisma,
        "justfile" | "makefile" | "gnumakefile" => FileIconGlyph::Terminal,
        ".env" | ".env.local" | ".env.development" | ".env.production" | ".env.test" => {
            FileIconGlyph::Config
        }
        "license" | "licence" | "authors" | "contributors" => FileIconGlyph::Text,
        _ => return special_config_glyph(name),
    };
    Some(exact)
}

fn special_config_glyph(name: &str) -> Option<FileIconGlyph> {
    let mappings = [
        ("vite.config", FileIconGlyph::Vite),
        ("vitest.config", FileIconGlyph::Vitest),
        ("eslint.config", FileIconGlyph::Eslint),
        ("tailwind.config", FileIconGlyph::Tailwind),
        ("prettier.config", FileIconGlyph::Prettier),
        ("webpack.config", FileIconGlyph::Webpack),
        ("rollup.config", FileIconGlyph::Rollup),
        ("babel.config", FileIconGlyph::JavaScript),
        ("jest.config", FileIconGlyph::Jest),
        ("playwright.config", FileIconGlyph::Playwright),
        ("storybook.config", FileIconGlyph::Storybook),
        ("astro.config", FileIconGlyph::Astro),
        ("next.config", FileIconGlyph::Nextjs),
        ("svelte.config", FileIconGlyph::Svelte),
        ("vue.config", FileIconGlyph::Vue),
    ];
    for (prefix, glyph) in mappings {
        if name == prefix
            || name
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('.'))
        {
            return Some(glyph);
        }
    }

    if name.starts_with(".eslintrc.") {
        Some(FileIconGlyph::Eslint)
    } else if name.starts_with(".prettierrc.") {
        Some(FileIconGlyph::Prettier)
    } else if name.starts_with("tsconfig") && has_extension(name, "json") {
        Some(FileIconGlyph::TypeScript)
    } else if name.starts_with("jsconfig") && has_extension(name, "json") {
        Some(FileIconGlyph::JavaScript)
    } else if name.starts_with("readme")
        || name.starts_with("changelog")
        || name.starts_with("contributing")
    {
        Some(FileIconGlyph::Markdown)
    } else {
        None
    }
}

fn has_extension(name: &str, extension: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

fn extension_glyph(extension: Option<&str>) -> Option<FileIconGlyph> {
    let glyph = match extension? {
        "rs" => FileIconGlyph::Rust,
        "ts" | "mts" | "cts" => FileIconGlyph::TypeScript,
        "js" | "mjs" | "cjs" => FileIconGlyph::JavaScript,
        "tsx" | "jsx" => FileIconGlyph::React,
        "vue" => FileIconGlyph::Vue,
        "svelte" => FileIconGlyph::Svelte,
        "astro" => FileIconGlyph::Astro,
        "html" | "htm" => FileIconGlyph::Html,
        "css" => FileIconGlyph::Css,
        "scss" | "sass" => FileIconGlyph::Sass,
        "less" => FileIconGlyph::Less,
        "json" | "jsonc" => FileIconGlyph::Json,
        "yaml" | "yml" => FileIconGlyph::Yaml,
        "toml" => FileIconGlyph::Toml,
        "md" | "mdx" | "markdown" => FileIconGlyph::Markdown,
        "svg" => FileIconGlyph::Svg,
        "py" | "pyi" | "pyw" => FileIconGlyph::Python,
        "java" | "jar" => FileIconGlyph::Java,
        "c" | "h" => FileIconGlyph::C,
        "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" => FileIconGlyph::Cpp,
        "cs" | "csx" => FileIconGlyph::CSharp,
        "go" => FileIconGlyph::Go,
        "php" => FileIconGlyph::Php,
        "rb" => FileIconGlyph::Ruby,
        "swift" => FileIconGlyph::Swift,
        "kt" | "kts" => FileIconGlyph::Kotlin,
        "lua" => FileIconGlyph::Lua,
        "r" | "rmd" => FileIconGlyph::R,
        "scala" | "sc" => FileIconGlyph::Scala,
        "sol" => FileIconGlyph::Solidity,
        "pl" | "pm" | "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd" => {
            FileIconGlyph::Terminal
        }
        "wasm" | "wat" => FileIconGlyph::Wasm,
        "tf" | "tfvars" | "hcl" => FileIconGlyph::Terraform,
        "graphql" | "gql" => FileIconGlyph::Graphql,
        "prisma" => FileIconGlyph::Prisma,
        "sql" => FileIconGlyph::Database,
        "sqlite" | "sqlite3" | "db" => FileIconGlyph::Sqlite,
        "pgsql" => FileIconGlyph::Postgresql,
        "mongo" => FileIconGlyph::Mongodb,
        "redis" => FileIconGlyph::Redis,
        "supabase" => FileIconGlyph::Supabase,
        "lock" | "pem" | "key" | "crt" | "cer" | "p12" => FileIconGlyph::Lock,
        "ini" | "cfg" | "conf" | "config" | "properties" => FileIconGlyph::Config,
        "txt" | "log" | "csv" | "tsv" | "pdf" | "rtf" => FileIconGlyph::Text,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "ico" | "bmp" | "tiff" => {
            FileIconGlyph::Image
        }
        "mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a" => FileIconGlyph::Audio,
        "mp4" | "webm" | "mov" | "avi" | "mkv" | "m4v" => FileIconGlyph::Video,
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "tgz" => FileIconGlyph::Archive,
        "xml" | "xsl" | "xslt" | "proto" => FileIconGlyph::Code,
        _ => return None,
    };
    Some(glyph)
}

#[cfg(test)]
mod tests {
    use super::{file_icon_glyph, FileIconGlyph};

    #[test]
    fn branded_configs_win_over_generic_data_extensions() {
        assert_eq!(file_icon_glyph("vite.config.ts"), FileIconGlyph::Vite);
        assert_eq!(file_icon_glyph("biome.json"), FileIconGlyph::Biome);
        assert_eq!(file_icon_glyph("eslint.config.mjs"), FileIconGlyph::Eslint);
        assert_eq!(
            file_icon_glyph(".github/workflows/ci.yml"),
            FileIconGlyph::GithubActions
        );
    }

    #[test]
    fn package_managers_and_language_extensions_are_distinct() {
        assert_eq!(file_icon_glyph("Cargo.toml"), FileIconGlyph::Rust);
        assert_eq!(file_icon_glyph("pnpm-lock.yaml"), FileIconGlyph::Pnpm);
        assert_eq!(file_icon_glyph("src/app.tsx"), FileIconGlyph::React);
        assert_eq!(file_icon_glyph("settings.json"), FileIconGlyph::Json);
    }
}
