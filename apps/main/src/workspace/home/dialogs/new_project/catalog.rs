use syntaxis_ui::prelude::ProjectTemplateIcon;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum ProjectTemplate {
    #[default]
    Empty,
    Rust,
    Python,
    Go,
    Deno,
    Bun,
    Nodejs,
    DotnetConsole,
    Dioxus,
    Blazor,
    Vite,
    VitePlus,
    Cloudflare,
    Shadcn,
    React,
    Vue,
    SvelteKit,
    SolidStart,
    Nextjs,
    Astro,
    Nuxt,
    TanstackStart,
    ReactRouter,
    Hono,
    Fresh,
    AspNetApi,
    Aspire,
    Django,
    Expo,
    Tauri,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TemplateCategory {
    Basics,
    Web,
    Backend,
    Native,
}

impl TemplateCategory {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Basics => "Basics",
            Self::Web => "Web",
            Self::Backend => "Backend",
            Self::Native => "Native",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct TemplateDefinition {
    pub(super) template: ProjectTemplate,
    pub(super) label: &'static str,
    pub(super) description: &'static str,
    pub(super) icon: ProjectTemplateIcon,
    pub(super) category: TemplateCategory,
    pub(super) command: Option<&'static str>,
}

pub(super) const CATEGORIES: [TemplateCategory; 4] = [
    TemplateCategory::Basics,
    TemplateCategory::Web,
    TemplateCategory::Backend,
    TemplateCategory::Native,
];

pub(super) const TEMPLATES: [TemplateDefinition; 30] = [
    TemplateDefinition {
        template: ProjectTemplate::Empty,
        label: "Empty",
        description: "Just a folder",
        icon: ProjectTemplateIcon::Empty,
        category: TemplateCategory::Basics,
        command: None,
    },
    TemplateDefinition {
        template: ProjectTemplate::Rust,
        label: "Rust",
        description: "Cargo binary",
        icon: ProjectTemplateIcon::Rust,
        category: TemplateCategory::Basics,
        command: Some("mise x rust@stable -- cargo init . && mise use -y rust@stable"),
    },
    TemplateDefinition {
        template: ProjectTemplate::Python,
        label: "Python",
        description: "uv package",
        icon: ProjectTemplateIcon::Python,
        category: TemplateCategory::Basics,
        command: Some(
            "mise x python@latest uv@latest -- uv init . && mise use -y python@latest uv@latest",
        ),
    },
    TemplateDefinition {
        template: ProjectTemplate::Go,
        label: "Go",
        description: "Go module",
        icon: ProjectTemplateIcon::Go,
        category: TemplateCategory::Basics,
        command: Some(
            "mise x go@latest -- sh -lc 'go mod init \"$(basename \"$PWD\")\"' && mise use -y go@latest",
        ),
    },
    TemplateDefinition {
        template: ProjectTemplate::Deno,
        label: "Deno",
        description: "Deno starter",
        icon: ProjectTemplateIcon::Deno,
        category: TemplateCategory::Basics,
        command: Some("mise x deno@latest -- deno init . && mise use -y deno@latest"),
    },
    TemplateDefinition {
        template: ProjectTemplate::Bun,
        label: "Bun",
        description: "Interactive bun init",
        icon: ProjectTemplateIcon::Bun,
        category: TemplateCategory::Basics,
        command: Some("mise x bun@latest -- bun init && mise use -y bun@latest"),
    },
    TemplateDefinition {
        template: ProjectTemplate::Nodejs,
        label: "Node.js",
        description: "Interactive npm init",
        icon: ProjectTemplateIcon::Nodejs,
        category: TemplateCategory::Basics,
        command: Some("mise x node@lts -- npm init && mise use -y node@lts"),
    },
    TemplateDefinition {
        template: ProjectTemplate::DotnetConsole,
        label: ".NET Console",
        description: "C# console app",
        icon: ProjectTemplateIcon::Dotnet,
        category: TemplateCategory::Basics,
        command: Some(
            "mise x dotnet@latest -- dotnet new console --output . && mise use -y dotnet@latest",
        ),
    },
    TemplateDefinition {
        template: ProjectTemplate::Dioxus,
        label: "Dioxus",
        description: "Interactive 0.7 app",
        icon: ProjectTemplateIcon::Dioxus,
        category: TemplateCategory::Web,
        command: Some(
            "mise x rust@stable cargo:dioxus-cli@0.7.1 -- dx new . --vcs none && mise use -y rust@stable cargo:dioxus-cli@0.7.1",
        ),
    },
    TemplateDefinition {
        template: ProjectTemplate::Blazor,
        label: "Blazor",
        description: "Blazor Web App",
        icon: ProjectTemplateIcon::Dotnet,
        category: TemplateCategory::Web,
        command: Some(
            "mise x dotnet@latest -- dotnet new blazor --output . && mise use -y dotnet@latest",
        ),
    },
    TemplateDefinition {
        template: ProjectTemplate::Vite,
        label: "Vite",
        description: "Interactive framework picker",
        icon: ProjectTemplateIcon::Vite,
        category: TemplateCategory::Web,
        command: Some("mise x node@lts -- npx --yes create-vite@latest . && mise use -y node@lts"),
    },
    TemplateDefinition {
        template: ProjectTemplate::VitePlus,
        label: "Vite+",
        description: "Unified toolchain picker",
        icon: ProjectTemplateIcon::VitePlus,
        category: TemplateCategory::Web,
        command: Some(
            "if ! command -v vp >/dev/null 2>&1; then curl -fsSL https://vite.plus | bash; fi; export PATH=\"${VP_HOME:-$HOME/.vite-plus}/bin:$PATH\"; vp create --directory .",
        ),
    },
    TemplateDefinition {
        template: ProjectTemplate::Cloudflare,
        label: "Cloudflare",
        description: "Interactive Workers app",
        icon: ProjectTemplateIcon::Cloudflare,
        category: TemplateCategory::Web,
        command: Some(
            "mise x node@lts -- npm create cloudflare@latest -- . && mise use -y node@lts",
        ),
    },
    TemplateDefinition {
        template: ProjectTemplate::Shadcn,
        label: "shadcn/ui",
        description: "Interactive UI starter",
        icon: ProjectTemplateIcon::Shadcn,
        category: TemplateCategory::Web,
        command: Some(
            "mise x node@lts -- sh -lc 'project_name=$(basename \"$PWD\"); npx --yes shadcn@latest init --name \"$project_name\" && cp -a -- \"$project_name\"/. . && rm -rf -- \"$project_name\"' && mise use -y node@lts",
        ),
    },
    TemplateDefinition {
        template: ProjectTemplate::React,
        label: "React",
        description: "Vite + TypeScript",
        icon: ProjectTemplateIcon::React,
        category: TemplateCategory::Web,
        command: Some(
            "mise x node@lts -- sh -lc 'npx --yes create-vite@latest . --template react-ts && npm install' && mise use -y node@lts",
        ),
    },
    TemplateDefinition {
        template: ProjectTemplate::Vue,
        label: "Vue",
        description: "Interactive create-vue",
        icon: ProjectTemplateIcon::Vue,
        category: TemplateCategory::Web,
        command: Some("mise x node@lts -- npx --yes create-vue@latest . && mise use -y node@lts"),
    },
    TemplateDefinition {
        template: ProjectTemplate::SvelteKit,
        label: "SvelteKit",
        description: "Interactive sv create",
        icon: ProjectTemplateIcon::Svelte,
        category: TemplateCategory::Web,
        command: Some("mise x node@lts -- npx --yes sv@latest create . && mise use -y node@lts"),
    },
    TemplateDefinition {
        template: ProjectTemplate::SolidStart,
        label: "SolidStart",
        description: "Interactive Solid app",
        icon: ProjectTemplateIcon::Solid,
        category: TemplateCategory::Web,
        command: Some("mise x node@lts -- npx --yes create-solid@latest . && mise use -y node@lts"),
    },
    TemplateDefinition {
        template: ProjectTemplate::Nextjs,
        label: "Next.js",
        description: "Interactive create-next-app",
        icon: ProjectTemplateIcon::Nextjs,
        category: TemplateCategory::Web,
        command: Some(
            "mise x node@lts -- npx --yes create-next-app@latest . && mise use -y node@lts",
        ),
    },
    TemplateDefinition {
        template: ProjectTemplate::Astro,
        label: "Astro",
        description: "Interactive create-astro",
        icon: ProjectTemplateIcon::Astro,
        category: TemplateCategory::Web,
        command: Some("mise x node@lts -- npx --yes create-astro@latest . && mise use -y node@lts"),
    },
    TemplateDefinition {
        template: ProjectTemplate::Nuxt,
        label: "Nuxt",
        description: "Interactive create-nuxt",
        icon: ProjectTemplateIcon::Nuxt,
        category: TemplateCategory::Web,
        command: Some("mise x node@lts -- npx --yes create-nuxt@latest . && mise use -y node@lts"),
    },
    TemplateDefinition {
        template: ProjectTemplate::TanstackStart,
        label: "TanStack Start",
        description: "Interactive add-on builder",
        icon: ProjectTemplateIcon::Tanstack,
        category: TemplateCategory::Web,
        command: Some(
            "mise x node@lts -- npx --yes @tanstack/cli@latest create . && mise use -y node@lts",
        ),
    },
    TemplateDefinition {
        template: ProjectTemplate::ReactRouter,
        label: "React Router",
        description: "Framework mode starter",
        icon: ProjectTemplateIcon::ReactRouter,
        category: TemplateCategory::Web,
        command: Some(
            "mise x node@lts -- npx --yes create-react-router@latest . && mise use -y node@lts",
        ),
    },
    TemplateDefinition {
        template: ProjectTemplate::Hono,
        label: "Hono",
        description: "Interactive runtime picker",
        icon: ProjectTemplateIcon::Hono,
        category: TemplateCategory::Web,
        command: Some("mise x node@lts -- npx --yes create-hono@latest . && mise use -y node@lts"),
    },
    TemplateDefinition {
        template: ProjectTemplate::Fresh,
        label: "Fresh",
        description: "Interactive Deno app",
        icon: ProjectTemplateIcon::Fresh,
        category: TemplateCategory::Web,
        command: Some(
            "mise x deno@latest -- deno run -Ar jsr:@fresh/init . && mise use -y deno@latest",
        ),
    },
    TemplateDefinition {
        template: ProjectTemplate::AspNetApi,
        label: "ASP.NET Core API",
        description: "Minimal Web API",
        icon: ProjectTemplateIcon::Dotnet,
        category: TemplateCategory::Backend,
        command: Some(
            "mise x dotnet@latest -- dotnet new webapi --output . && mise use -y dotnet@latest",
        ),
    },
    TemplateDefinition {
        template: ProjectTemplate::Aspire,
        label: ".NET Aspire",
        description: "Distributed app stack",
        icon: ProjectTemplateIcon::Dotnet,
        category: TemplateCategory::Backend,
        command: Some(
            "mise x dotnet@latest aspire@latest -- sh -lc 'aspire new aspire-starter --name \"$(basename \"$PWD\")\" --output .' && mise use -y dotnet@latest aspire@latest",
        ),
    },
    TemplateDefinition {
        template: ProjectTemplate::Django,
        label: "Django",
        description: "uv + Django project",
        icon: ProjectTemplateIcon::Django,
        category: TemplateCategory::Backend,
        command: Some(
            "mise x python@latest uv@latest -- sh -lc 'uv init --bare . && uv add django && uv run django-admin startproject config .' && mise use -y python@latest uv@latest",
        ),
    },
    TemplateDefinition {
        template: ProjectTemplate::Expo,
        label: "React Native",
        description: "Interactive Expo app",
        icon: ProjectTemplateIcon::Expo,
        category: TemplateCategory::Native,
        command: Some(
            "mise x node@lts -- npx --yes create-expo-app@latest . && mise use -y node@lts",
        ),
    },
    TemplateDefinition {
        template: ProjectTemplate::Tauri,
        label: "Tauri",
        description: "Interactive desktop app",
        icon: ProjectTemplateIcon::Tauri,
        category: TemplateCategory::Native,
        command: Some(
            "mise x node@lts rust@stable -- npx --yes create-tauri-app@latest . && mise use -y node@lts rust@stable",
        ),
    },
];
