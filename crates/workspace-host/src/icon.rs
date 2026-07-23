use std::{fs, path::Path};

use base64::{engine::general_purpose::STANDARD, Engine};
use syntaxis_workspace::{WorkspaceIcon, WorkspaceIconSymbol};

const IMAGE_CANDIDATES: &[&str] = &[
    "favicon.svg",
    "favicon.ico",
    "favicon.png",
    "public/favicon.svg",
    "public/favicon.ico",
    "public/favicon.png",
    "app/favicon.ico",
    "app/favicon.png",
    "app/icon.svg",
    "app/icon.png",
    "src/favicon.svg",
    "src/favicon.ico",
    "assets/icon.svg",
    "assets/icon.png",
    "assets/logo.svg",
    "assets/logo.png",
    ".idea/icon.svg",
];

pub fn detect_workspace_icon(root: &Path) -> WorkspaceIcon {
    for relative_path in IMAGE_CANDIDATES {
        if let Some(data_url) = read_icon_data_url(&root.join(relative_path)) {
            return WorkspaceIcon::Image {
                relative_path: (*relative_path).to_owned(),
                data_url: Some(data_url),
            };
        }
    }
    WorkspaceIcon::Symbol {
        name: detect_symbol(root),
    }
}

fn detect_symbol(root: &Path) -> WorkspaceIconSymbol {
    let package = read_small_text(&root.join("package.json")).unwrap_or_default();
    let dependency = |name: &str| package.contains(&format!("\"{name}\""));

    if root.join("next.config.js").exists()
        || root.join("next.config.mjs").exists()
        || dependency("next")
    {
        WorkspaceIconSymbol::Nextjs
    } else if root.join(".storybook/main.ts").exists() || dependency("@storybook/react") {
        WorkspaceIconSymbol::Storybook
    } else if root.join("vercel.json").exists() {
        WorkspaceIconSymbol::Vercel
    } else if root.join("vite.config.ts").exists()
        || root.join("vite.config.js").exists()
        || dependency("vite")
    {
        WorkspaceIconSymbol::Vite
    } else if root.join("tsconfig.json").exists()
        || root.join("tsconfig.base.json").exists()
        || dependency("typescript")
    {
        WorkspaceIconSymbol::Typescript
    } else if dependency("react") || dependency("react-dom") {
        WorkspaceIconSymbol::React
    } else if dependency("vue") {
        WorkspaceIconSymbol::Vue
    } else if dependency("svelte") {
        WorkspaceIconSymbol::Svelte
    } else if root.join("package.json").exists()
        || root.join("pnpm-lock.yaml").exists()
        || root.join("package-lock.json").exists()
    {
        WorkspaceIconSymbol::Node
    } else if root.join("Cargo.toml").exists() {
        WorkspaceIconSymbol::Rust
    } else if root.join("pyproject.toml").exists() || root.join("requirements.txt").exists() {
        WorkspaceIconSymbol::Python
    } else if root.join("go.mod").exists() {
        WorkspaceIconSymbol::Go
    } else if root.join("Dockerfile").exists() || root.join("docker-compose.yml").exists() {
        WorkspaceIconSymbol::Docker
    } else if root.join(".git").exists() {
        WorkspaceIconSymbol::Git
    } else {
        WorkspaceIconSymbol::Folder
    }
}

fn read_icon_data_url(path: &Path) -> Option<String> {
    let metadata = path.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > 128 * 1024 {
        return None;
    }
    let mime = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        _ => return None,
    };
    Some(format!(
        "data:{mime};base64,{}",
        STANDARD.encode(fs::read(path).ok()?)
    ))
}

fn read_small_text(path: &Path) -> Option<String> {
    let metadata = path.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > 32 * 1024 {
        return None;
    }
    fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use super::detect_workspace_icon;
    use std::fs;
    use syntaxis_workspace::{WorkspaceIcon, WorkspaceIconSymbol};
    use tempfile::tempdir;

    #[test]
    fn detects_rust_projects_without_an_image() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(
            detect_workspace_icon(root.path()),
            WorkspaceIcon::Symbol {
                name: WorkspaceIconSymbol::Rust
            }
        );
    }

    #[test]
    fn image_candidates_take_priority() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("favicon.svg"), "<svg/>").unwrap();
        let icon = detect_workspace_icon(root.path());
        assert!(matches!(icon, WorkspaceIcon::Image { .. }));
        if let WorkspaceIcon::Image {
            relative_path,
            data_url,
        } = icon
        {
            assert_eq!(relative_path, "favicon.svg");
            assert!(data_url.unwrap().starts_with("data:image/svg+xml;base64,"));
        }
    }
}
