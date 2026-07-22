use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct WorkspaceId(pub String);

impl WorkspaceId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceAvailability {
    Available,
    Missing,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceIconSymbol {
    Docker,
    Folder,
    Git,
    Go,
    Javascript,
    Nextjs,
    Node,
    Python,
    React,
    Rust,
    Storybook,
    Svelte,
    Typescript,
    Vercel,
    Vite,
    Vue,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceCleanupEntry {
    pub path: String,
    pub directory: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceIcon {
    Image {
        relative_path: String,
        data_url: Option<String>,
    },
    Symbol {
        name: WorkspaceIconSymbol,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceTechnology {
    Angular,
    Astro,
    Biome,
    Bun,
    Composer,
    Deno,
    Django,
    Dioxus,
    Docker,
    Eslint,
    Fastapi,
    Firebase,
    GithubActions,
    Graphql,
    Jest,
    Just,
    Mongodb,
    Nextjs,
    Nginx,
    Nodejs,
    Npm,
    Playwright,
    Pnpm,
    Postgresql,
    Prettier,
    Prisma,
    React,
    Redis,
    Rollup,
    Sqlite,
    Storybook,
    Supabase,
    Svelte,
    Tailwind,
    Terraform,
    Vercel,
    Vite,
    Vitest,
    Vue,
    Webpack,
    Yarn,
}

impl WorkspaceTechnology {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Angular => "Angular",
            Self::Astro => "Astro",
            Self::Biome => "Biome",
            Self::Bun => "Bun",
            Self::Composer => "Composer",
            Self::Deno => "Deno",
            Self::Django => "Django",
            Self::Dioxus => "Dioxus",
            Self::Docker => "Docker",
            Self::Eslint => "ESLint",
            Self::Fastapi => "FastAPI",
            Self::Firebase => "Firebase",
            Self::GithubActions => "GitHub Actions",
            Self::Graphql => "GraphQL",
            Self::Jest => "Jest",
            Self::Just => "Just",
            Self::Mongodb => "MongoDB",
            Self::Nextjs => "Next.js",
            Self::Nginx => "NGINX",
            Self::Nodejs => "Node.js",
            Self::Npm => "npm",
            Self::Playwright => "Playwright",
            Self::Pnpm => "pnpm",
            Self::Postgresql => "PostgreSQL",
            Self::Prettier => "Prettier",
            Self::Prisma => "Prisma",
            Self::React => "React",
            Self::Redis => "Redis",
            Self::Rollup => "Rollup",
            Self::Sqlite => "SQLite",
            Self::Storybook => "Storybook",
            Self::Supabase => "Supabase",
            Self::Svelte => "Svelte",
            Self::Tailwind => "Tailwind CSS",
            Self::Terraform => "Terraform",
            Self::Vercel => "Vercel",
            Self::Vite => "Vite",
            Self::Vitest => "Vitest",
            Self::Vue => "Vue",
            Self::Webpack => "Webpack",
            Self::Yarn => "Yarn",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceLanguage {
    pub name: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceProfile {
    pub technologies: Vec<WorkspaceTechnology>,
    pub languages: Vec<WorkspaceLanguage>,
}

impl WorkspaceProfile {
    pub fn total_language_bytes(&self) -> u64 {
        self.languages.iter().map(|language| language.bytes).sum()
    }
}

impl Default for WorkspaceIcon {
    fn default() -> Self {
        Self::Symbol {
            name: WorkspaceIconSymbol::Folder,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceRecord {
    pub id: WorkspaceId,
    pub slug: String,
    pub name: String,
    /// Canonical absolute path as understood by the runtime.
    pub root: String,
    pub icon: WorkspaceIcon,
    #[serde(default)]
    pub profile: WorkspaceProfile,
    pub registered_at_unix_ms: i64,
    pub last_opened_unix_ms: i64,
    pub availability: WorkspaceAvailability,
}
