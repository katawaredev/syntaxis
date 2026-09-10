use syntaxis_ui::prelude::ProjectTemplateIcon;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ProjectTemplate {
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
pub(crate) enum TemplateCategory {
    Basics,
    Web,
    Backend,
    Native,
}

impl TemplateCategory {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Basics => "Basics",
            Self::Web => "Web",
            Self::Backend => "Backend",
            Self::Native => "Native",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct TemplateDefinition {
    pub(crate) template: ProjectTemplate,
    pub(crate) label: &'static str,
    pub(crate) description: &'static str,
    pub(crate) icon: ProjectTemplateIcon,
    pub(crate) category: TemplateCategory,
    pub(crate) command: Option<&'static str>,
}

pub(crate) const CATEGORIES: [TemplateCategory; 4] = [
    TemplateCategory::Basics,
    TemplateCategory::Web,
    TemplateCategory::Backend,
    TemplateCategory::Native,
];

pub(crate) const TEMPLATES: [TemplateDefinition; 30] = [
    template(
        ProjectTemplate::Empty,
        "Empty",
        "Just a folder",
        ProjectTemplateIcon::Empty,
        TemplateCategory::Basics,
        None,
    ),
    template(
        ProjectTemplate::Rust,
        "Rust",
        "Cargo binary",
        ProjectTemplateIcon::Rust,
        TemplateCategory::Basics,
        Some("mise x rust@stable -- cargo init . && mise use -y rust@stable"),
    ),
    template(
        ProjectTemplate::Python,
        "Python",
        "uv package",
        ProjectTemplateIcon::Python,
        TemplateCategory::Basics,
        Some("mise x python@latest uv@latest -- uv init . && mise use -y python@latest uv@latest"),
    ),
    template(
        ProjectTemplate::Go,
        "Go",
        "Go module",
        ProjectTemplateIcon::Go,
        TemplateCategory::Basics,
        Some(
            "mise x go@latest -- sh -lc 'go mod init \"$(basename \"$PWD\")\"' && mise use -y go@latest",
        ),
    ),
    template(
        ProjectTemplate::Deno,
        "Deno",
        "Deno starter",
        ProjectTemplateIcon::Deno,
        TemplateCategory::Basics,
        Some("mise x deno@latest -- deno init . && mise use -y deno@latest"),
    ),
    template(
        ProjectTemplate::Bun,
        "Bun",
        "Interactive bun init",
        ProjectTemplateIcon::Bun,
        TemplateCategory::Basics,
        Some("mise x bun@latest -- bun init && mise use -y bun@latest"),
    ),
    template(
        ProjectTemplate::Nodejs,
        "Node.js",
        "Interactive npm init",
        ProjectTemplateIcon::Nodejs,
        TemplateCategory::Basics,
        Some("mise x node@lts -- npm init && mise use -y node@lts"),
    ),
    template(
        ProjectTemplate::DotnetConsole,
        ".NET Console",
        "C# console app",
        ProjectTemplateIcon::Dotnet,
        TemplateCategory::Basics,
        Some("mise x dotnet@latest -- dotnet new console --output . && mise use -y dotnet@latest"),
    ),
    template(
        ProjectTemplate::Dioxus,
        "Dioxus",
        "Interactive 0.7 app",
        ProjectTemplateIcon::Dioxus,
        TemplateCategory::Web,
        Some(
            "mise x rust@stable cargo:dioxus-cli@0.7.10 -- dx new . --vcs none && mise use -y rust@stable cargo:dioxus-cli@0.7.10",
        ),
    ),
    template(
        ProjectTemplate::Blazor,
        "Blazor",
        "Blazor Web App",
        ProjectTemplateIcon::Dotnet,
        TemplateCategory::Web,
        Some("mise x dotnet@latest -- dotnet new blazor --output . && mise use -y dotnet@latest"),
    ),
    template(
        ProjectTemplate::Vite,
        "Vite",
        "Interactive framework picker",
        ProjectTemplateIcon::Vite,
        TemplateCategory::Web,
        Some("mise x node@lts -- npx --yes create-vite@latest . && mise use -y node@lts"),
    ),
    template(
        ProjectTemplate::VitePlus,
        "Vite+",
        "Unified toolchain picker",
        ProjectTemplateIcon::VitePlus,
        TemplateCategory::Web,
        Some(
            "if ! command -v vp >/dev/null 2>&1; then curl -fsSL https://vite.plus | bash; fi; export PATH=\"${VP_HOME:-$HOME/.vite-plus}/bin:$PATH\"; vp create --directory .",
        ),
    ),
    template(
        ProjectTemplate::Cloudflare,
        "Cloudflare",
        "Interactive Workers app",
        ProjectTemplateIcon::Cloudflare,
        TemplateCategory::Web,
        Some("mise x node@lts -- npm create cloudflare@latest -- . && mise use -y node@lts"),
    ),
    template(
        ProjectTemplate::Shadcn,
        "shadcn/ui",
        "Interactive UI starter",
        ProjectTemplateIcon::Shadcn,
        TemplateCategory::Web,
        Some(
            "mise x node@lts -- sh -lc 'project_name=$(basename \"$PWD\"); npx --yes shadcn@latest init --name \"$project_name\" && cp -a -- \"$project_name\"/. . && rm -rf -- \"$project_name\"' && mise use -y node@lts",
        ),
    ),
    template(
        ProjectTemplate::React,
        "React",
        "Vite + TypeScript",
        ProjectTemplateIcon::React,
        TemplateCategory::Web,
        Some(
            "mise x node@lts -- sh -lc 'npx --yes create-vite@latest . --template react-ts && npm install' && mise use -y node@lts",
        ),
    ),
    template(
        ProjectTemplate::Vue,
        "Vue",
        "Interactive create-vue",
        ProjectTemplateIcon::Vue,
        TemplateCategory::Web,
        Some("mise x node@lts -- npx --yes create-vue@latest . && mise use -y node@lts"),
    ),
    template(
        ProjectTemplate::SvelteKit,
        "SvelteKit",
        "Interactive sv create",
        ProjectTemplateIcon::Svelte,
        TemplateCategory::Web,
        Some("mise x node@lts -- npx --yes sv@latest create . && mise use -y node@lts"),
    ),
    template(
        ProjectTemplate::SolidStart,
        "SolidStart",
        "Interactive Solid app",
        ProjectTemplateIcon::Solid,
        TemplateCategory::Web,
        Some("mise x node@lts -- npx --yes create-solid@latest . && mise use -y node@lts"),
    ),
    template(
        ProjectTemplate::Nextjs,
        "Next.js",
        "Interactive create-next-app",
        ProjectTemplateIcon::Nextjs,
        TemplateCategory::Web,
        Some("mise x node@lts -- npx --yes create-next-app@latest . && mise use -y node@lts"),
    ),
    template(
        ProjectTemplate::Astro,
        "Astro",
        "Interactive create-astro",
        ProjectTemplateIcon::Astro,
        TemplateCategory::Web,
        Some("mise x node@lts -- npx --yes create-astro@latest . && mise use -y node@lts"),
    ),
    template(
        ProjectTemplate::Nuxt,
        "Nuxt",
        "Interactive create-nuxt",
        ProjectTemplateIcon::Nuxt,
        TemplateCategory::Web,
        Some("mise x node@lts -- npx --yes create-nuxt@latest . && mise use -y node@lts"),
    ),
    template(
        ProjectTemplate::TanstackStart,
        "TanStack Start",
        "Interactive add-on builder",
        ProjectTemplateIcon::Tanstack,
        TemplateCategory::Web,
        Some("mise x node@lts -- npx --yes @tanstack/cli@latest create . && mise use -y node@lts"),
    ),
    template(
        ProjectTemplate::ReactRouter,
        "React Router",
        "Framework mode starter",
        ProjectTemplateIcon::ReactRouter,
        TemplateCategory::Web,
        Some("mise x node@lts -- npx --yes create-react-router@latest . && mise use -y node@lts"),
    ),
    template(
        ProjectTemplate::Hono,
        "Hono",
        "Interactive runtime picker",
        ProjectTemplateIcon::Hono,
        TemplateCategory::Web,
        Some("mise x node@lts -- npx --yes create-hono@latest . && mise use -y node@lts"),
    ),
    template(
        ProjectTemplate::Fresh,
        "Fresh",
        "Interactive Deno app",
        ProjectTemplateIcon::Fresh,
        TemplateCategory::Web,
        Some("mise x deno@latest -- deno run -Ar jsr:@fresh/init . && mise use -y deno@latest"),
    ),
    template(
        ProjectTemplate::AspNetApi,
        "ASP.NET Core API",
        "Minimal Web API",
        ProjectTemplateIcon::Dotnet,
        TemplateCategory::Backend,
        Some("mise x dotnet@latest -- dotnet new webapi --output . && mise use -y dotnet@latest"),
    ),
    template(
        ProjectTemplate::Aspire,
        ".NET Aspire",
        "Distributed app stack",
        ProjectTemplateIcon::Dotnet,
        TemplateCategory::Backend,
        Some(
            "mise x dotnet@latest aspire@latest -- sh -lc 'aspire new aspire-starter --name \"$(basename \"$PWD\")\" --output .' && mise use -y dotnet@latest aspire@latest",
        ),
    ),
    template(
        ProjectTemplate::Django,
        "Django",
        "uv + Django project",
        ProjectTemplateIcon::Django,
        TemplateCategory::Backend,
        Some(
            "mise x python@latest uv@latest -- sh -lc 'uv init --bare . && uv add django && uv run django-admin startproject config .' && mise use -y python@latest uv@latest",
        ),
    ),
    template(
        ProjectTemplate::Expo,
        "React Native",
        "Interactive Expo app",
        ProjectTemplateIcon::Expo,
        TemplateCategory::Native,
        Some("mise x node@lts -- npx --yes create-expo-app@latest . && mise use -y node@lts"),
    ),
    template(
        ProjectTemplate::Tauri,
        "Tauri",
        "Interactive desktop app",
        ProjectTemplateIcon::Tauri,
        TemplateCategory::Native,
        Some(
            "mise x node@lts rust@stable -- npx --yes create-tauri-app@latest . && mise use -y node@lts rust@stable",
        ),
    ),
];

const fn template(
    template: ProjectTemplate,
    label: &'static str,
    description: &'static str,
    icon: ProjectTemplateIcon,
    category: TemplateCategory,
    command: Option<&'static str>,
) -> TemplateDefinition {
    TemplateDefinition {
        template,
        label,
        description,
        icon,
        category,
        command,
    }
}

pub(crate) fn definition(template: ProjectTemplate) -> TemplateDefinition {
    TEMPLATES
        .into_iter()
        .find(|definition| definition.template == template)
        .unwrap_or(TEMPLATES[0])
}

pub(crate) fn matches(template: &TemplateDefinition, query: &str) -> bool {
    let query = query.trim().to_ascii_lowercase();
    query.is_empty()
        || template.label.to_ascii_lowercase().contains(&query)
        || template.description.to_ascii_lowercase().contains(&query)
        || template
            .category
            .label()
            .to_ascii_lowercase()
            .contains(&query)
}

pub(crate) fn category_has_matches(category: TemplateCategory, query: &str) -> bool {
    TEMPLATES
        .iter()
        .any(|template| template.category == category && matches(template, query))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{ProjectTemplate, TEMPLATES, definition};

    #[test]
    fn catalog_is_unique_and_retains_interactive_starters() {
        assert_eq!(TEMPLATES.len(), 30);
        assert_eq!(
            TEMPLATES
                .iter()
                .map(|template| template.label)
                .collect::<HashSet<_>>()
                .len(),
            TEMPLATES.len(),
        );
        assert!(
            definition(ProjectTemplate::Dioxus)
                .command
                .unwrap()
                .contains("0.7.10")
        );
        assert!(
            definition(ProjectTemplate::VitePlus)
                .command
                .unwrap()
                .contains("vp create")
        );
    }
}
