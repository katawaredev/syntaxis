use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;
use syntaxis_workspace::{
    BrowseDirectory, BrowseRoot, ErrorCode, WorkspaceBrowser, WorkspaceError, WorkspaceResult,
};

use crate::{error::map_io_error, RegistrationPolicy};

#[derive(Clone, Debug)]
pub struct HostWorkspaceBrowser {
    policy: RegistrationPolicy,
}

impl HostWorkspaceBrowser {
    /// Creates a browser constrained by the supplied registration policy.
    ///
    /// # Errors
    ///
    /// Returns an error if an allowlisted root cannot be canonicalized.
    pub fn new(policy: RegistrationPolicy) -> WorkspaceResult<Self> {
        Ok(Self {
            policy: policy.canonicalize()?,
        })
    }

    fn require_permitted(&self, path: &Path) -> WorkspaceResult<std::path::PathBuf> {
        let canonical = path.canonicalize().map_err(map_io_error)?;
        if !canonical.is_dir() {
            return Err(WorkspaceError::invalid_path(
                "Only directories can be browsed.",
            ));
        }
        if self.policy.permits(&canonical) {
            Ok(canonical)
        } else {
            Err(WorkspaceError::new(
                ErrorCode::OutsideAllowedRoot,
                "That directory is outside the roots exposed by this runtime.",
            ))
        }
    }

    /// Resolves a client-visible browser path to a validated host directory.
    ///
    /// Allowlisted runtimes expose a virtual `/`; unrestricted runtimes continue to use native
    /// absolute paths.
    ///
    /// # Errors
    ///
    /// Returns an error when the path is malformed, unavailable, or outside an exposed root.
    pub fn resolve_path(&self, path: &str) -> WorkspaceResult<PathBuf> {
        match &self.policy {
            RegistrationPolicy::Unrestricted => self.require_permitted(Path::new(path)),
            RegistrationPolicy::Allowlisted { roots } if roots.len() == 1 => {
                let relative = virtual_relative_path(path)?;
                self.require_permitted(&roots[0].join(relative))
            }
            RegistrationPolicy::Allowlisted { roots } => {
                let relative = virtual_relative_path(path)?;
                let mut components = relative.components();
                let Some(Component::Normal(mount)) = components.next() else {
                    return Err(WorkspaceError::invalid_path("Choose a workspace root."));
                };
                let mount = mount.to_string_lossy();
                let Some((_, root)) = roots
                    .iter()
                    .enumerate()
                    .find(|(index, root)| mount_name(roots, *index, root) == mount)
                else {
                    return Err(outside_root_error());
                };
                let remainder = components.collect::<PathBuf>();
                self.require_permitted(&root.join(remainder))
            }
        }
    }

    /// Creates a new directory at a client-visible browser path.
    ///
    /// Missing parent directories are created, but every existing ancestor is resolved before any
    /// write so a symlink cannot redirect creation outside the runtime's registration policy.
    ///
    /// # Errors
    ///
    /// Returns an error when the path is malformed, already exists, or falls outside an exposed
    /// workspace root.
    pub fn create_directory(&self, path: &str) -> WorkspaceResult<PathBuf> {
        let candidate = self.new_directory_candidate(path)?;
        if candidate.exists() {
            return Err(WorkspaceError::new(
                ErrorCode::AlreadyExists,
                "A file or directory already exists at that project path.",
            ));
        }

        let mut existing = candidate.as_path();
        while !existing.exists() {
            existing = existing.parent().ok_or_else(outside_root_error)?;
        }
        let canonical_parent = existing.canonicalize().map_err(map_io_error)?;
        if !canonical_parent.is_dir() || !self.policy.permits(&canonical_parent) {
            return Err(outside_root_error());
        }
        let missing = candidate
            .strip_prefix(existing)
            .map_err(|_| outside_root_error())?;
        let destination = canonical_parent.join(missing);
        std::fs::create_dir_all(&destination).map_err(map_io_error)?;
        let canonical = destination.canonicalize().map_err(map_io_error)?;
        if self.policy.permits(&canonical) {
            Ok(canonical)
        } else {
            let _ = std::fs::remove_dir_all(&canonical);
            Err(outside_root_error())
        }
    }

    fn new_directory_candidate(&self, path: &str) -> WorkspaceResult<PathBuf> {
        validate_project_text(path)?;
        match &self.policy {
            RegistrationPolicy::Unrestricted => {
                let candidate = Path::new(path);
                if !candidate.is_absolute() {
                    return Err(WorkspaceError::invalid_path(
                        "Choose an absolute project path.",
                    ));
                }
                validate_new_project_path(candidate)?;
                Ok(candidate.to_owned())
            }
            RegistrationPolicy::Allowlisted { roots } if roots.len() == 1 => {
                let relative = virtual_relative_path(path)?;
                require_project_relative_path(&relative)?;
                Ok(roots[0].join(relative))
            }
            RegistrationPolicy::Allowlisted { roots } => {
                let relative = virtual_relative_path(path)?;
                let mut components = relative.components();
                let Some(Component::Normal(mount)) = components.next() else {
                    return Err(WorkspaceError::invalid_path("Choose a workspace root."));
                };
                let mount = mount.to_string_lossy();
                let Some((_, root)) = roots
                    .iter()
                    .enumerate()
                    .find(|(index, root)| mount_name(roots, *index, root) == mount)
                else {
                    return Err(outside_root_error());
                };
                let remainder = components.collect::<PathBuf>();
                require_project_relative_path(&remainder)?;
                Ok(root.join(remainder))
            }
        }
    }

    /// Converts a host directory into its client-visible browser path.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory is outside the configured roots.
    pub fn virtual_path(&self, absolute_path: &Path) -> WorkspaceResult<String> {
        match &self.policy {
            RegistrationPolicy::Unrestricted => Ok(absolute_path.to_string_lossy().into_owned()),
            RegistrationPolicy::Allowlisted { roots } if roots.len() == 1 => absolute_path
                .strip_prefix(&roots[0])
                .map(virtual_path_from_relative)
                .map_err(|_| outside_root_error()),
            RegistrationPolicy::Allowlisted { roots } => {
                let Some((index, root)) = roots
                    .iter()
                    .enumerate()
                    .filter(|(_, root)| absolute_path.starts_with(root))
                    .max_by_key(|(_, root)| root.components().count())
                else {
                    return Err(outside_root_error());
                };
                let relative = absolute_path
                    .strip_prefix(root)
                    .map_err(|_| outside_root_error())?;
                let mount = mount_name(roots, index, root);
                if relative.as_os_str().is_empty() {
                    Ok(format!("/{mount}"))
                } else {
                    Ok(format!("/{mount}/{}", relative.to_string_lossy()))
                }
            }
        }
    }

    fn root_choices(&self) -> Vec<BrowseRoot> {
        match &self.policy {
            RegistrationPolicy::Unrestricted => Vec::new(),
            RegistrationPolicy::Allowlisted { roots } if roots.len() == 1 => {
                vec![BrowseRoot {
                    name: root_name(&roots[0]),
                    path: "/".into(),
                }]
            }
            RegistrationPolicy::Allowlisted { roots } => roots
                .iter()
                .enumerate()
                .map(|(index, root)| BrowseRoot {
                    name: root_name(root),
                    path: format!("/{}", mount_name(roots, index, root)),
                })
                .collect(),
        }
    }
}

fn validate_project_text(path: &str) -> WorkspaceResult<()> {
    let relative = path.strip_prefix('/').unwrap_or(path);
    if relative.is_empty()
        || path.starts_with("//")
        || path.contains('\\')
        || relative.split('/').any(|component| {
            component.is_empty()
                || matches!(component, "." | "..")
                || component.len() > 255
                || component.chars().any(char::is_control)
        })
    {
        return Err(WorkspaceError::invalid_path(
            "Use a project name or subpath without empty, dot, or parent folders.",
        ));
    }
    Ok(())
}

fn require_project_relative_path(path: &Path) -> WorkspaceResult<()> {
    if path.as_os_str().is_empty() {
        Err(WorkspaceError::invalid_path("Enter a project name."))
    } else {
        validate_new_project_path(path)
    }
}

fn validate_new_project_path(path: &Path) -> WorkspaceResult<()> {
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::Prefix(_) | Component::CurDir
        )
    }) {
        return Err(outside_root_error());
    }
    if path.components().any(|component| {
        matches!(component, Component::Normal(value) if value.is_empty() || value.to_string_lossy().chars().any(char::is_control))
    }) {
        return Err(WorkspaceError::invalid_path(
            "Project folders cannot be empty or contain control characters.",
        ));
    }
    Ok(())
}

#[async_trait(?Send)]
impl WorkspaceBrowser for HostWorkspaceBrowser {
    async fn roots(&self) -> WorkspaceResult<Vec<BrowseRoot>> {
        Ok(self.root_choices())
    }

    async fn directories(&self, path: &str) -> WorkspaceResult<Vec<BrowseDirectory>> {
        if path == "/"
            && matches!(
                &self.policy,
                RegistrationPolicy::Allowlisted { roots } if roots.len() > 1
            )
        {
            return Ok(self
                .root_choices()
                .into_iter()
                .map(|root| BrowseDirectory {
                    name: root.name,
                    path: root.path,
                })
                .collect());
        }
        let directory = self.resolve_path(path)?;
        let mut directories = std::fs::read_dir(directory)
            .map_err(map_io_error)?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                let file_type = entry.file_type().ok()?;
                if !file_type.is_dir() || file_type.is_symlink() {
                    return None;
                }
                Some(BrowseDirectory {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path: self.virtual_path(&path).ok()?,
                })
            })
            .collect::<Vec<_>>();
        directories.sort_by_key(|entry| entry.name.to_lowercase());
        Ok(directories)
    }
}

fn virtual_relative_path(path: &str) -> WorkspaceResult<PathBuf> {
    if path.starts_with("//") {
        return Err(WorkspaceError::invalid_path(
            "Use no more than one leading slash in a workspace path.",
        ));
    }
    let path = Path::new(path);
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(component) => relative.push(component),
            Component::ParentDir | Component::Prefix(_) => return Err(outside_root_error()),
        }
    }
    Ok(relative)
}

fn virtual_path_from_relative(relative: &Path) -> String {
    if relative.as_os_str().is_empty() {
        "/".into()
    } else {
        format!("/{}", relative.to_string_lossy())
    }
}

fn root_name(root: &Path) -> String {
    root.file_name()
        .unwrap_or(root.as_os_str())
        .to_string_lossy()
        .into_owned()
}

fn mount_name(roots: &[PathBuf], index: usize, root: &Path) -> String {
    let name = root_name(root);
    let duplicate = roots
        .iter()
        .filter(|candidate| root_name(candidate) == name)
        .count()
        > 1;
    if duplicate {
        format!("{name}-{}", index + 1)
    } else {
        name
    }
}

fn outside_root_error() -> WorkspaceError {
    WorkspaceError::new(
        ErrorCode::OutsideAllowedRoot,
        "That directory is outside the roots exposed by this runtime.",
    )
}

#[cfg(test)]
mod tests {
    use futures_lite::future::block_on;
    use syntaxis_workspace::{ErrorCode, WorkspaceBrowser};
    use tempfile::tempdir;

    use crate::{HostWorkspaceBrowser, RegistrationPolicy};

    #[test]
    fn browser_cannot_leave_allowlisted_roots() {
        let allowed = tempdir().unwrap();
        let browser = HostWorkspaceBrowser::new(RegistrationPolicy::Allowlisted {
            roots: vec![allowed.path().to_owned()],
        })
        .unwrap();
        let error = block_on(browser.directories("/../outside")).unwrap_err();
        assert_eq!(error.code, ErrorCode::OutsideAllowedRoot);
    }

    #[test]
    fn allowlisted_browser_exposes_only_virtual_paths() {
        let allowed = tempdir().unwrap();
        std::fs::create_dir(allowed.path().join("syntaxis")).unwrap();
        let browser = HostWorkspaceBrowser::new(RegistrationPolicy::Allowlisted {
            roots: vec![allowed.path().to_owned()],
        })
        .unwrap();

        let roots = block_on(browser.roots()).unwrap();
        let directories = block_on(browser.directories("/")).unwrap();

        assert_eq!(roots[0].path, "/");
        assert_eq!(directories[0].path, "/syntaxis");
        assert_eq!(
            browser.resolve_path("/syntaxis").unwrap(),
            allowed.path().join("syntaxis").canonicalize().unwrap()
        );
        assert_eq!(
            browser.resolve_path("syntaxis").unwrap(),
            allowed.path().join("syntaxis").canonicalize().unwrap()
        );
        assert_eq!(
            browser.resolve_path("//syntaxis").unwrap_err().code,
            ErrorCode::InvalidPath
        );
    }

    #[test]
    fn creates_nested_projects_without_leaving_the_exposed_root() {
        let allowed = tempdir().unwrap();
        let browser = HostWorkspaceBrowser::new(RegistrationPolicy::Allowlisted {
            roots: vec![allowed.path().to_owned()],
        })
        .unwrap();

        let created = browser.create_directory("/testing/MyAwesomeIdea").unwrap();
        assert_eq!(created, allowed.path().join("testing/MyAwesomeIdea"));
        assert!(created.is_dir());
        assert_eq!(
            browser
                .create_directory("/testing/MyAwesomeIdea")
                .unwrap_err()
                .code,
            ErrorCode::AlreadyExists
        );
        assert_eq!(
            browser.create_directory("/../outside").unwrap_err().code,
            ErrorCode::InvalidPath
        );
    }
}
