use dioxus::prelude::*;
use dioxus_icons::lucide::{
    ArrowDown, ArrowUp, Check, Command, Ellipsis, FolderOpen, GitBranch, Menu, PanelLeftOpen, Play,
    Plus, RefreshCw, Save, Search, X,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppIcon {
    Check,
    Close,
    Command,
    Explorer,
    Fetch,
    Folder,
    GitBranch,
    Menu,
    More,
    Play,
    Plus,
    Push,
    Refresh,
    Save,
    Search,
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
    }
}
