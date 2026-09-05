use std::{
    env, fs,
    path::{Path, PathBuf},
};

const CACHE_ROOTS: &[&str] = &[
    ".cache",
    ".npm/_cacache",
    ".npm/_logs",
    ".npm/_npx",
    ".bun/install/cache",
    ".cargo/registry/cache",
    ".cargo/registry/src",
    ".cargo/registry/index",
    ".cargo/git/checkouts",
    ".cargo/git/db",
    ".rustup/downloads",
    ".rustup/tmp",
    ".local/share/mise/downloads",
    ".local/share/pnpm/store",
    ".gradle/caches",
    ".gradle/daemon",
    ".gradle/native",
    ".gradle/notifications",
    ".gradle/wrapper/dists",
    ".nuget/packages",
    ".pnpm-store",
    ".yarn/cache",
    ".yarn/unplugged",
    "go/pkg/mod",
    "go/pkg/sumdb",
];

const TOOL_ROOTS: &[&str] = &[".bun", ".deno", ".rustup"];

pub(super) fn purge() -> Result<usize, String> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "The runtime home directory is unavailable.".to_owned())?;
    purge_under(&home, CACHE_ROOTS)
}

pub(super) fn purge_tools() -> Result<usize, String> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "The runtime home directory is unavailable.".to_owned())?;
    purge_under(&home, TOOL_ROOTS)
}

fn purge_under(home: &Path, roots: &[&str]) -> Result<usize, String> {
    let canonical_home = home
        .canonicalize()
        .map_err(|error| format!("Could not validate the runtime home directory: {error}"))?;
    if !canonical_home.is_dir() || canonical_home.parent().is_none() {
        return Err("The runtime home directory is not safe to clean.".into());
    }
    let mut removed = 0;
    for relative in roots {
        let path = home.join(relative);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!("Could not inspect {}: {error}", path.display()));
            }
        };
        if metadata.file_type().is_symlink() {
            fs::remove_file(&path)
                .map_err(|error| format!("Could not remove {}: {error}", path.display()))?;
        } else {
            let canonical = path
                .canonicalize()
                .map_err(|error| format!("Could not validate {}: {error}", path.display()))?;
            if canonical == canonical_home || !canonical.starts_with(&canonical_home) {
                return Err(format!(
                    "Refused to clean unsafe cache path {}",
                    path.display()
                ));
            }
            if metadata.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            }
            .map_err(|error| format!("Could not remove {}: {error}", path.display()))?;
        }
        removed += 1;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purge_removes_caches_without_touching_installed_tools() {
        let home = tempfile::tempdir().unwrap();
        let cache = home.path().join(".cache/uv/archive");
        let cargo = home.path().join(".cargo/registry/src/package");
        let pnpm = home.path().join(".local/share/pnpm/store/v3/files");
        let installed = home.path().join(".local/share/mise/installs/node/24");
        fs::create_dir_all(&cache).unwrap();
        fs::create_dir_all(&cargo).unwrap();
        fs::create_dir_all(&pnpm).unwrap();
        fs::create_dir_all(&installed).unwrap();
        fs::write(cache.join("wheel"), "cache").unwrap();
        fs::write(cargo.join("lib.rs"), "cache").unwrap();
        fs::write(installed.join("node"), "installed").unwrap();

        assert_eq!(purge_under(home.path(), CACHE_ROOTS).unwrap(), 3);
        assert!(!home.path().join(".cache").exists());
        assert!(!home.path().join(".cargo/registry/src").exists());
        assert!(!home.path().join(".local/share/pnpm/store").exists());
        assert!(installed.join("node").is_file());
    }

    #[test]
    fn purge_tools_removes_reinstallable_tools_without_touching_configuration() {
        let home = tempfile::tempdir().unwrap();
        let bun = home.path().join(".bun/install/global/node_modules/tool");
        let rustup = home.path().join(".rustup/toolchains/stable");
        let config = home.path().join(".config/mise/config.toml");
        fs::create_dir_all(&bun).unwrap();
        fs::create_dir_all(&rustup).unwrap();
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "[tools]").unwrap();

        assert_eq!(purge_under(home.path(), TOOL_ROOTS).unwrap(), 2);
        assert!(!home.path().join(".bun").exists());
        assert!(!home.path().join(".rustup").exists());
        assert!(config.is_file());
    }
}
