#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceState {
    Available,
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MockWorkspace {
    pub slug: &'static str,
    pub name: &'static str,
    pub path: &'static str,
    pub icon: &'static str,
    pub recent: &'static str,
    pub state: WorkspaceState,
}

pub const WORKSPACES: [MockWorkspace; 4] = [
    MockWorkspace {
        slug: "syntaxis",
        name: "Syntaxis",
        path: "/home/alex/projects/syntaxis",
        icon: "S",
        recent: "Just now",
        state: WorkspaceState::Available,
    },
    MockWorkspace {
        slug: "atlas-api",
        name: "Atlas API",
        path: "/home/alex/projects/atlas-api",
        icon: "A",
        recent: "2 hours ago",
        state: WorkspaceState::Available,
    },
    MockWorkspace {
        slug: "field-notes",
        name: "Field Notes",
        path: "/home/alex/projects/field-notes",
        icon: "F",
        recent: "Yesterday",
        state: WorkspaceState::Available,
    },
    MockWorkspace {
        slug: "old-dashboard",
        name: "Old Dashboard",
        path: "/mnt/archive/old-dashboard",
        icon: "O",
        recent: "3 weeks ago",
        state: WorkspaceState::Missing,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Modified,
    Added,
    Deleted,
    Untracked,
    Conflicted,
}

impl ChangeKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Modified => "M",
            Self::Added => "A",
            Self::Deleted => "D",
            Self::Untracked => "U",
            Self::Conflicted => "!",
        }
    }

    pub const fn class(self) -> &'static str {
        match self {
            Self::Modified => "change-modified",
            Self::Added => "change-added",
            Self::Deleted | Self::Conflicted => "change-deleted",
            Self::Untracked => "change-untracked",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MockChange {
    pub path: &'static str,
    pub kind: ChangeKind,
}

pub const CONFLICTS: [MockChange; 1] = [MockChange {
    path: "src/workspace.rs",
    kind: ChangeKind::Conflicted,
}];
pub const STAGED: [MockChange; 2] = [
    MockChange {
        path: "src/app.rs",
        kind: ChangeKind::Modified,
    },
    MockChange {
        path: "assets/app.css",
        kind: ChangeKind::Added,
    },
];
pub const CHANGES: [MockChange; 4] = [
    MockChange {
        path: "src/files.rs",
        kind: ChangeKind::Modified,
    },
    MockChange {
        path: "README.md",
        kind: ChangeKind::Modified,
    },
    MockChange {
        path: "src/legacy.rs",
        kind: ChangeKind::Deleted,
    },
    MockChange {
        path: "notes/todo.md",
        kind: ChangeKind::Untracked,
    },
];
