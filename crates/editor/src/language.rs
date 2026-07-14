pub fn language_slug_for_path(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    if matches!(name, "dockerfile" | "containerfile") || lower.ends_with(".dockerfile") {
        return "dockerfile";
    }
    if is_shell_name(name) {
        return "bash";
    }
    match extension(name) {
        "rs" => "rust",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" | "sass" => "scss",
        "json" | "jsonc" => "json",
        "md" | "markdown" => "markdown",
        "xml" | "svg" => "xml",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "py" | "pyw" => "python",
        "php" => "php",
        "rb" => "ruby",
        "go" => "go",
        "sql" => "sql",
        "diff" | "patch" => "diff",
        "ps1" | "psd1" | "psm1" => "powershell",
        "lua" => "lua",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" => "cpp",
        "cs" => "c-sharp",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "dart" => "dart",
        "ini" | "properties" => "ini",
        _ if name == "nginx.conf" => "nginx",
        _ => "rust",
    }
}

pub fn language_label_for_path(path: &str) -> &'static str {
    match language_slug_for_path(path) {
        "bash" => "Shell",
        "c-sharp" => "C#",
        "cpp" => "C++",
        "dockerfile" => "Dockerfile",
        "javascript" => "JavaScript",
        "json" => "JSON",
        "markdown" => "Markdown",
        "powershell" => "PowerShell",
        "python" => "Python",
        "rust" => "Rust",
        "sql" => "SQL",
        "toml" => "TOML",
        "tsx" => "TSX",
        "typescript" => "TypeScript",
        "xml" => "XML",
        "yaml" => "YAML",
        other => other,
    }
}

fn extension(name: &str) -> &str {
    name.rsplit_once('.').map_or("", |(_, extension)| extension)
}

fn is_shell_name(name: &str) -> bool {
    matches!(
        name,
        ".bash_aliases"
            | ".bash_login"
            | ".bash_logout"
            | ".bash_profile"
            | ".bashrc"
            | ".profile"
            | ".zprofile"
            | ".zshenv"
            | ".zshrc"
            | "pre-commit"
            | "pre-push"
            | "post-checkout"
            | "commit-msg"
    ) || matches!(extension(name), "sh" | "bash" | "zsh" | "ksh")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_special_names_and_broad_language_extensions() {
        assert_eq!(language_slug_for_path("Dockerfile"), "dockerfile");
        assert_eq!(language_slug_for_path(".git/hooks/pre-commit"), "bash");
        assert_eq!(language_slug_for_path("src/view.tsx"), "tsx");
        assert_eq!(language_slug_for_path("config/app.yaml"), "yaml");
    }
}
