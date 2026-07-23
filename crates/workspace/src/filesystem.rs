use std::{convert::TryFrom, path::Component};

use serde::{Deserialize, Serialize};

use crate::{WorkspaceError, WorkspaceResult};

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RelativePath(String);

impl RelativePath {
    pub fn root() -> Self {
        Self(String::new())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }
}

impl TryFrom<&str> for RelativePath {
    type Error = WorkspaceError;

    fn try_from(value: &str) -> WorkspaceResult<Self> {
        if value.is_empty() || value == "." {
            return Ok(Self::root());
        }

        let path = std::path::Path::new(value);
        if path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(WorkspaceError::invalid_path(
                "Workspace paths must be relative and cannot contain '..'.",
            ));
        }

        let normalized = path
            .components()
            .filter_map(|component| match component {
                Component::Normal(part) => Some(part.to_string_lossy()),
                Component::Prefix(_)
                | Component::RootDir
                | Component::CurDir
                | Component::ParentDir => None,
            })
            .collect::<Vec<_>>()
            .join("/");

        Ok(Self(normalized))
    }
}

impl TryFrom<String> for RelativePath {
    type Error = WorkspaceError;

    fn try_from(value: String) -> WorkspaceResult<Self> {
        Self::try_from(value.as_str())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileVersion {
    pub length: u64,
    pub modified_unix_nanos: u128,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileEntry {
    pub path: RelativePath,
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
    pub version: Option<FileVersion>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TextFile {
    pub content: String,
    pub version: FileVersion,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BinaryFile {
    pub content: Vec<u8>,
    pub version: FileVersion,
}

#[cfg(test)]
mod tests {
    use super::RelativePath;

    #[test]
    fn relative_paths_reject_escape_attempts() {
        RelativePath::try_from("../secret").expect_err("parent traversal must be rejected");
        RelativePath::try_from("folder/../../secret")
            .expect_err("nested parent traversal must be rejected");
        RelativePath::try_from("/etc/passwd").expect_err("absolute paths must be rejected");
    }

    #[test]
    fn relative_paths_are_normalized() {
        let path = RelativePath::try_from("./src/./main.rs")
            .expect("valid relative path should normalize");
        assert_eq!(path.as_str(), "src/main.rs");
    }
}
