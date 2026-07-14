use dioxus::prelude::*;
use dioxus_icons::lucide::{
    ArrowDown, ArrowUp, Check, Command, Ellipsis, EllipsisVertical, FolderOpen, GitBranch,
    GitCommitHorizontal, ListChevronsDownUp, ListChevronsUpDown, Menu, PanelLeftOpen, Play, Plus,
    RefreshCw, Save, Search, X,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppIcon {
    Check,
    Close,
    Command,
    Commit,
    Collapse,
    Explorer,
    Fetch,
    Folder,
    GitBranch,
    Menu,
    More,
    MoreVertical,
    Play,
    Plus,
    Push,
    Refresh,
    Save,
    Search,
    Expand,
}

#[component]
pub fn Icon(icon: AppIcon, #[props(default = 16)] size: u32) -> Element {
    match icon {
        AppIcon::Check => rsx! {
            Check { size }
        },
        AppIcon::Close => rsx! {
            X { size }
        },
        AppIcon::Command => rsx! {
            Command { size }
        },
        AppIcon::Commit => rsx! {
            GitCommitHorizontal { size }
        },
        AppIcon::Collapse => rsx! {
            ListChevronsDownUp { size }
        },
        AppIcon::Explorer => rsx! {
            PanelLeftOpen { size }
        },
        AppIcon::Fetch => rsx! {
            ArrowDown { size }
        },
        AppIcon::Folder => rsx! {
            FolderOpen { size }
        },
        AppIcon::GitBranch => rsx! {
            GitBranch { size }
        },
        AppIcon::Menu => rsx! {
            Menu { size }
        },
        AppIcon::More => rsx! {
            Ellipsis { size }
        },
        AppIcon::MoreVertical => rsx! {
            EllipsisVertical { size }
        },
        AppIcon::Play => rsx! {
            Play { size }
        },
        AppIcon::Plus => rsx! {
            Plus { size }
        },
        AppIcon::Push => rsx! {
            ArrowUp { size }
        },
        AppIcon::Refresh => rsx! {
            RefreshCw { size }
        },
        AppIcon::Save => rsx! {
            Save { size }
        },
        AppIcon::Search => rsx! {
            Search { size }
        },
        AppIcon::Expand => rsx! {
            ListChevronsUpDown { size }
        },
    }
}
