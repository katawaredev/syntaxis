use syntaxis_workspace::WorkspaceTechnology;

/// An allowlisted language server that Syntaxis can launch through mise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LanguageServerDefinition {
    pub id: &'static str,
    pub label: &'static str,
    pub language_ids: &'static [&'static str],
    pub mise_tools: &'static [&'static str],
    pub executable: &'static str,
    pub arguments: &'static [&'static str],
    /// An installed project package that may provide the executable locally.
    pub project_local: Option<ProjectLocalLanguageServer>,
    /// Supplementary servers augment, rather than replace, a primary server.
    pub supplementary: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectLocalLanguageServer {
    pub package: &'static str,
    pub minimum_major: Option<u64>,
}

const fn project_package(package: &'static str) -> ProjectLocalLanguageServer {
    ProjectLocalLanguageServer {
        package,
        minimum_major: None,
    }
}

const LANGUAGE_SERVERS: &[LanguageServerDefinition] = &[
    LanguageServerDefinition {
        id: "rust-analyzer",
        label: "rust-analyzer",
        language_ids: &["rust"],
        mise_tools: &["rust-analyzer@latest"],
        executable: "rust-analyzer",
        arguments: &[],
        project_local: None,
        supplementary: false,
    },
    LanguageServerDefinition {
        id: "deno",
        label: "Deno",
        language_ids: &["javascript", "typescript", "tsx"],
        mise_tools: &["deno@latest"],
        executable: "deno",
        arguments: &["lsp"],
        project_local: None,
        supplementary: false,
    },
    LanguageServerDefinition {
        id: "typescript",
        label: "TypeScript",
        language_ids: &["javascript", "typescript", "tsx"],
        mise_tools: &["npm:typescript@latest"],
        executable: "tsc",
        arguments: &["--lsp", "--stdio"],
        project_local: Some(ProjectLocalLanguageServer {
            package: "typescript",
            minimum_major: Some(7),
        }),
        supplementary: false,
    },
    LanguageServerDefinition {
        id: "pyright",
        label: "Pyright",
        language_ids: &["python"],
        mise_tools: &["npm:pyright@latest"],
        executable: "pyright-langserver",
        arguments: &["--stdio"],
        project_local: Some(project_package("pyright")),
        supplementary: false,
    },
    LanguageServerDefinition {
        id: "gopls",
        label: "gopls",
        language_ids: &["go"],
        mise_tools: &["go@latest", "go:golang.org/x/tools/gopls@latest"],
        executable: "gopls",
        arguments: &[],
        project_local: None,
        supplementary: false,
    },
    LanguageServerDefinition {
        id: "vscode-html-language-server",
        label: "HTML Language Server",
        language_ids: &["html"],
        mise_tools: &["npm:vscode-langservers-extracted@latest"],
        executable: "vscode-html-language-server",
        arguments: &["--stdio"],
        project_local: Some(project_package("vscode-langservers-extracted")),
        supplementary: false,
    },
    LanguageServerDefinition {
        id: "vscode-css-language-server",
        label: "CSS Language Server",
        language_ids: &["css", "scss"],
        mise_tools: &["npm:vscode-langservers-extracted@latest"],
        executable: "vscode-css-language-server",
        arguments: &["--stdio"],
        project_local: Some(project_package("vscode-langservers-extracted")),
        supplementary: false,
    },
    LanguageServerDefinition {
        id: "vscode-json-language-server",
        label: "JSON Language Server",
        language_ids: &["json"],
        mise_tools: &["npm:vscode-langservers-extracted@latest"],
        executable: "vscode-json-language-server",
        arguments: &["--stdio"],
        project_local: Some(project_package("vscode-langservers-extracted")),
        supplementary: false,
    },
    LanguageServerDefinition {
        id: "yaml-language-server",
        label: "YAML Language Server",
        language_ids: &["yaml"],
        mise_tools: &["npm:yaml-language-server@latest"],
        executable: "yaml-language-server",
        arguments: &["--stdio"],
        project_local: Some(project_package("yaml-language-server")),
        supplementary: false,
    },
    LanguageServerDefinition {
        id: "taplo",
        label: "Taplo",
        language_ids: &["toml"],
        mise_tools: &["taplo@latest"],
        executable: "taplo",
        arguments: &["lsp", "stdio"],
        project_local: None,
        supplementary: false,
    },
    LanguageServerDefinition {
        id: "bash-language-server",
        label: "Bash Language Server",
        language_ids: &["bash"],
        mise_tools: &["npm:bash-language-server@latest"],
        executable: "bash-language-server",
        arguments: &["start"],
        project_local: Some(project_package("bash-language-server")),
        supplementary: false,
    },
    LanguageServerDefinition {
        id: "terraform-ls",
        label: "Terraform Language Server",
        language_ids: &["terraform"],
        mise_tools: &["terraform-ls@latest"],
        executable: "terraform-ls",
        arguments: &["serve"],
        project_local: None,
        supplementary: false,
    },
    LanguageServerDefinition {
        id: "intelephense",
        label: "Intelephense",
        language_ids: &["php"],
        mise_tools: &["npm:intelephense@latest"],
        executable: "intelephense",
        arguments: &["--stdio"],
        project_local: Some(project_package("intelephense")),
        supplementary: false,
    },
    LanguageServerDefinition {
        id: "solargraph",
        label: "Solargraph",
        language_ids: &["ruby"],
        mise_tools: &["ruby@latest", "gem:solargraph@latest"],
        executable: "solargraph",
        arguments: &["stdio"],
        project_local: None,
        supplementary: false,
    },
    LanguageServerDefinition {
        id: "vue",
        label: "Vue",
        language_ids: &["vue"],
        mise_tools: &["npm:@vue/language-server@latest"],
        executable: "vue-language-server",
        arguments: &["--stdio"],
        project_local: Some(project_package("@vue/language-server")),
        supplementary: false,
    },
    LanguageServerDefinition {
        id: "svelte",
        label: "Svelte",
        language_ids: &["svelte"],
        mise_tools: &["npm:svelte-language-server@latest"],
        executable: "svelteserver",
        arguments: &["--stdio"],
        project_local: Some(project_package("svelte-language-server")),
        supplementary: false,
    },
    LanguageServerDefinition {
        id: "astro",
        label: "Astro",
        language_ids: &["astro"],
        mise_tools: &["npm:@astrojs/language-server@latest"],
        executable: "astro-ls",
        arguments: &["--stdio"],
        project_local: Some(project_package("@astrojs/language-server")),
        supplementary: false,
    },
    LanguageServerDefinition {
        id: "tailwindcss",
        label: "Tailwind CSS",
        language_ids: &[
            "html",
            "css",
            "scss",
            "javascript",
            "typescript",
            "tsx",
            "vue",
            "svelte",
            "astro",
        ],
        mise_tools: &["npm:@tailwindcss/language-server@latest"],
        executable: "tailwindcss-language-server",
        arguments: &["--stdio"],
        project_local: Some(project_package("@tailwindcss/language-server")),
        supplementary: true,
    },
];

#[must_use]
pub fn language_server_by_id(id: &str) -> Option<&'static LanguageServerDefinition> {
    LANGUAGE_SERVERS.iter().find(|server| server.id == id)
}

#[must_use]
pub fn language_servers_for_language(
    language_id: &str,
    technologies: &[WorkspaceTechnology],
) -> Vec<&'static LanguageServerDefinition> {
    let mut servers = Vec::with_capacity(2);
    if matches!(language_id, "javascript" | "typescript" | "tsx")
        && technologies.contains(&WorkspaceTechnology::Deno)
    {
        if let Some(server) = language_server_by_id("deno") {
            servers.push(server);
        }
    } else if let Some(server) = LANGUAGE_SERVERS
        .iter()
        .filter(|server| server.id != "deno" && !server.supplementary)
        .find(|server| server.language_ids.contains(&language_id))
    {
        servers.push(server);
    }

    if technologies.contains(&WorkspaceTechnology::Tailwind) {
        servers.extend(
            LANGUAGE_SERVERS.iter().filter(|server| {
                server.supplementary && server.language_ids.contains(&language_id)
            }),
        );
    }
    servers
}

#[must_use]
pub fn profile_language_id(name: &str) -> Option<&'static str> {
    match name {
        "Rust" => Some("rust"),
        "JavaScript" => Some("javascript"),
        "TypeScript" => Some("typescript"),
        "Python" => Some("python"),
        "Go" => Some("go"),
        "HTML" => Some("html"),
        "CSS" => Some("css"),
        "SCSS" | "Sass" => Some("scss"),
        "JSON" => Some("json"),
        "YAML" => Some("yaml"),
        "TOML" => Some("toml"),
        "Shell" => Some("bash"),
        "HCL" | "Terraform" => Some("terraform"),
        "PHP" => Some("php"),
        "Ruby" => Some("ruby"),
        "Vue" => Some("vue"),
        "Svelte" => Some("svelte"),
        "Astro" => Some("astro"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deno_projects_prefer_the_builtin_language_server() {
        assert_eq!(
            language_servers_for_language("typescript", &[WorkspaceTechnology::Deno])
                .iter()
                .map(|server| server.id)
                .collect::<Vec<_>>(),
            vec!["deno"]
        );
        assert_eq!(
            language_servers_for_language("typescript", &[])
                .iter()
                .map(|server| server.id)
                .collect::<Vec<_>>(),
            vec!["typescript"]
        );
    }

    #[test]
    fn unsupported_languages_do_not_fall_back_to_an_arbitrary_server() {
        assert!(language_servers_for_language("plaintext", &[]).is_empty());
        assert!(language_server_by_id("client-supplied-command").is_none());
    }

    #[test]
    fn tailwind_augments_the_primary_language_server() {
        assert_eq!(
            language_servers_for_language("tsx", &[WorkspaceTechnology::Tailwind])
                .iter()
                .map(|server| server.id)
                .collect::<Vec<_>>(),
            vec!["typescript", "tailwindcss"]
        );
        assert_eq!(
            language_servers_for_language(
                "typescript",
                &[WorkspaceTechnology::Deno, WorkspaceTechnology::Tailwind],
            )
            .iter()
            .map(|server| server.id)
            .collect::<Vec<_>>(),
            vec!["deno", "tailwindcss"]
        );
    }

    #[test]
    fn web_framework_files_use_their_framework_server() {
        assert_eq!(
            language_servers_for_language("vue", &[])
                .iter()
                .map(|server| server.id)
                .collect::<Vec<_>>(),
            vec!["vue"]
        );
        assert_eq!(
            language_servers_for_language("astro", &[WorkspaceTechnology::Tailwind])
                .iter()
                .map(|server| server.id)
                .collect::<Vec<_>>(),
            vec!["astro", "tailwindcss"]
        );
    }

    #[test]
    fn node_servers_declare_their_project_local_packages() {
        let typescript = language_server_by_id("typescript").unwrap();
        assert_eq!(
            typescript.project_local,
            Some(ProjectLocalLanguageServer {
                package: "typescript",
                minimum_major: Some(7),
            })
        );
        assert_eq!(typescript.mise_tools, ["npm:typescript@latest"]);
        for server_id in ["vue", "svelte", "astro", "tailwindcss"] {
            assert!(
                language_server_by_id(server_id)
                    .unwrap()
                    .project_local
                    .is_some(),
                "{server_id} should support a project-local package"
            );
        }
        assert!(
            language_server_by_id("rust-analyzer")
                .unwrap()
                .project_local
                .is_none()
        );
    }
}
