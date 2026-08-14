use super::FileIconGlyph;

pub(super) fn file_icon_glyph(path: &str) -> FileIconGlyph {
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
    use super::{FileIconGlyph, file_icon_glyph};

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
