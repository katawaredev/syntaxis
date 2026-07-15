use dioxus::prelude::*;
use dioxus_icons::lucide::{
    ArrowDown, ArrowUp, CaseSensitive, Check, ChevronLeft, ChevronRight, Code, Command, Copy,
    Ellipsis, EllipsisVertical, Eye, FileDiff, FileInput, FileMinus, FilePlus, FolderOpen,
    FolderPlus, GitBranch, GitCommitHorizontal, Hash, ListChevronsDownUp, ListChevronsUpDown,
    ListOrdered, Menu, PanelLeftOpen, Play, Plus, RefreshCw, Regex, Repeat1, Replace, ReplaceAll,
    RotateCcw, Save, Search, Send, Sparkles, Square, SquarePen, SquareTerminal, Trash2, Type,
    WholeWord, X,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppIcon {
    Check,
    Close,
    Code,
    Command,
    Copy,
    Delete,
    Terminal,
    Commit,
    Collapse,
    Explorer,
    Eye,
    Fetch,
    FileDiff,
    FileMinus,
    FileMove,
    FilePlus,
    Folder,
    FolderPlus,
    GitBranch,
    GoToLine,
    LineNumbers,
    MatchCase,
    MatchWholeWord,
    Menu,
    More,
    MoreVertical,
    Play,
    Previous,
    Plus,
    Push,
    Refresh,
    Regex,
    ReplaceAll,
    ReplaceNext,
    Revert,
    Save,
    Search,
    Send,
    Sparkles,
    Stop,
    NewChat,
    Next,
    ToggleReplace,
    WordWrap,
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
        AppIcon::Code => rsx! {
            Code { size }
        },
        AppIcon::Command => rsx! {
            Command { size }
        },
        AppIcon::Copy => rsx! {
            Copy { size }
        },
        AppIcon::Delete => rsx! {
            Trash2 { size }
        },
        AppIcon::Terminal => rsx! {
            SquareTerminal { size }
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
        AppIcon::Eye => rsx! {
            Eye { size }
        },
        AppIcon::Fetch => rsx! {
            ArrowDown { size }
        },
        AppIcon::FileDiff => rsx! {
            FileDiff { size }
        },
        AppIcon::FileMinus => rsx! {
            FileMinus { size }
        },
        AppIcon::FileMove => rsx! {
            FileInput { size }
        },
        AppIcon::FilePlus => rsx! {
            FilePlus { size }
        },
        AppIcon::Folder => rsx! {
            FolderOpen { size }
        },
        AppIcon::FolderPlus => rsx! {
            FolderPlus { size }
        },
        AppIcon::GitBranch => rsx! {
            GitBranch { size }
        },
        AppIcon::GoToLine => rsx! {
            ListOrdered { size }
        },
        AppIcon::LineNumbers => rsx! {
            Hash { size }
        },
        AppIcon::MatchCase => rsx! {
            CaseSensitive { size }
        },
        AppIcon::MatchWholeWord => rsx! {
            WholeWord { size }
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
        AppIcon::Previous => rsx! {
            ChevronLeft { size }
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
        AppIcon::Regex => rsx! {
            Regex { size }
        },
        AppIcon::ReplaceAll => rsx! {
            ReplaceAll { size }
        },
        AppIcon::ReplaceNext => rsx! {
            Repeat1 { size }
        },
        AppIcon::Revert => rsx! {
            RotateCcw { size }
        },
        AppIcon::Save => rsx! {
            Save { size }
        },
        AppIcon::Search => rsx! {
            Search { size }
        },
        AppIcon::Send => rsx! {
            Send { size }
        },
        AppIcon::Sparkles => rsx! {
            Sparkles { size }
        },
        AppIcon::Stop => rsx! {
            Square { size }
        },
        AppIcon::NewChat => rsx! {
            SquarePen { size }
        },
        AppIcon::Next => rsx! {
            ChevronRight { size }
        },
        AppIcon::ToggleReplace => rsx! {
            Replace { size }
        },
        AppIcon::WordWrap => rsx! {
            Type { size }
        },
        AppIcon::Expand => rsx! {
            ListChevronsUpDown { size }
        },
    }
}
