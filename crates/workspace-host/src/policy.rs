use std::path::{Path, PathBuf};

use syntaxis_workspace::{ErrorCode, WorkspaceError, WorkspaceResult};

#[derive(Clone, Debug)]
pub enum RegistrationPolicy {
    Unrestricted,
    Allowlisted { roots: Vec<PathBuf> },
}

impl RegistrationPolicy {
    pub(crate) fn canonicalize(self) -> WorkspaceResult<Self> {
        match self {
            Self::Unrestricted => Ok(Self::Unrestricted),
            Self::Allowlisted { roots } => {
                let roots = roots
                    .into_iter()
                    .map(|root| {
                        root.canonicalize().map_err(|_| {
                            WorkspaceError::new(
                                ErrorCode::InvalidPath,
                                "A configured workspace root is unavailable.",
                            )
                        })
                    })
                    .collect::<WorkspaceResult<Vec<_>>>()?;
                Ok(Self::Allowlisted { roots })
            }
        }
    }

    pub(crate) fn permits(&self, candidate: &Path) -> bool {
        match self {
            Self::Unrestricted => true,
            Self::Allowlisted { roots } => roots.iter().any(|root| candidate.starts_with(root)),
        }
    }

    pub(crate) fn permits_registered_root(&self, candidate: &Path) -> bool {
        match self {
            Self::Unrestricted => true,
            Self::Allowlisted { .. } => candidate.canonicalize().map_or_else(
                |_| self.permits(candidate),
                |canonical| canonical == candidate && self.permits(&canonical),
            ),
        }
    }

    pub(crate) fn roots(&self) -> &[PathBuf] {
        match self {
            Self::Unrestricted => &[],
            Self::Allowlisted { roots } => roots,
        }
    }
}
