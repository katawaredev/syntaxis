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
