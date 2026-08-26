//! Application actions backed by Lucide icons.

use dioxus::prelude::*;
use dioxus_icons::lucide::{
    ArrowDown, ArrowUp, Bell, Blocks, Bot, BrainCog, BrushCleaning, CaseSensitive,
    ChartNoAxesColumn, Check, ChevronDown, ChevronLeft, ChevronRight, Code, Command, Copy, CopyX,
    CornerDownRight, Ellipsis, EllipsisVertical, ExternalLink, Eye, FileDiff, FileInput, FileMinus,
    FilePlus, FolderGit2, FolderOpen, FolderPlus, GitBranch, GitCommitHorizontal, GitFork, Hash,
    Info, ListChevronsDownUp, ListChevronsUpDown, ListOrdered, LogOut, Menu, Mic, PanelLeftOpen,
    Paperclip, Play, Plus, RefreshCw, Regex, Repeat1, Replace, ReplaceAll, RotateCcw, Save,
    ScanSearch, Search, Send, Settings, Share2, ShieldAlert, Sparkles, Square, SquarePen,
    SquareTerminal, Star, TextCursorInput, TextWrap, Trash2, Volume2, WandSparkles, WholeWord, X,
};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppIcon {
    Bell,
    BrainCog,
    Check,
    ChevronDown,
    Close,
    Code,
    Completion,
    Command,
    Copy,
    CloseOthers,
    Attachment,
    Microphone,
    Usage,
    Cleanup,
    Delete,
    Terminal,
    Commit,
    Collapse,
    Explorer,
    ExternalLink,
    Eye,
    Fetch,
    Favourite,
    FavouriteFilled,
    FileDiff,
    FileMinus,
    FileMove,
    FilePlus,
    Folder,
    FolderGit2,
    FolderPlus,
    Info,
    GitBranch,
    Worktree,
    GoToLine,
    GoToDefinition,
    FindReferences,
    FormatDocument,
    LanguageServices,
    LineNumbers,
    Logout,
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
    Settings,
    Send,
    Share,
    ShieldAlert,
    Sparkles,
    Bot,
    Stop,
    NewChat,
    Next,
    ToggleReplace,
    Volume2,
    Undo,
    Redo,
    SelectAll,
    WordWrap,
    Expand,
}
#[component]
pub fn Icon(icon: AppIcon, #[props(default = 16)] size: u32) -> Element {
    match icon {
        AppIcon::Bell => {
            rsx! {
                Bell { size }
            }
        }
        AppIcon::BrainCog => {
            rsx! {
                BrainCog { size }
            }
        }
        AppIcon::Check => {
            rsx! {
                Check { size }
            }
        }
        AppIcon::ChevronDown => {
            rsx! {
                ChevronDown { size }
            }
        }
        AppIcon::Close => {
            rsx! {
                X { size }
            }
        }
        AppIcon::Code => {
            rsx! {
                Code { size }
            }
        }
        AppIcon::Completion => {
            rsx! {
                TextCursorInput { size }
            }
        }
        AppIcon::Command => {
            rsx! {
                Command { size }
            }
        }
        AppIcon::Copy => {
            rsx! {
                Copy { size }
            }
        }
        AppIcon::CloseOthers => {
            rsx! {
                CopyX { size }
            }
        }
        AppIcon::Attachment => {
            rsx! {
                Paperclip { size }
            }
        }
        AppIcon::Microphone => {
            rsx! {
                Mic { size }
            }
        }
        AppIcon::Usage => {
            rsx! {
                ChartNoAxesColumn { size }
            }
        }
        AppIcon::Cleanup => {
            rsx! {
                BrushCleaning { size }
            }
        }
        AppIcon::Delete => {
            rsx! {
                Trash2 { size }
            }
        }
        AppIcon::Terminal => {
            rsx! {
                SquareTerminal { size }
            }
        }
        AppIcon::Commit => {
            rsx! {
                GitCommitHorizontal { size }
            }
        }
        AppIcon::Collapse => {
            rsx! {
                ListChevronsDownUp { size }
            }
        }
        AppIcon::Explorer => {
            rsx! {
                PanelLeftOpen { size }
            }
        }
        AppIcon::ExternalLink => {
            rsx! {
                ExternalLink { size }
            }
        }
        AppIcon::Eye => {
            rsx! {
                Eye { size }
            }
        }
        AppIcon::Fetch => {
            rsx! {
                ArrowDown { size }
            }
        }
        AppIcon::Favourite => {
            rsx! {
                Star { size }
            }
        }
        AppIcon::FavouriteFilled => {
            rsx! {
                Star { size, fill: "currentColor" }
            }
        }
        AppIcon::FileDiff => {
            rsx! {
                FileDiff { size }
            }
        }
        AppIcon::FileMinus => {
            rsx! {
                FileMinus { size }
            }
        }
        AppIcon::FileMove => {
            rsx! {
                FileInput { size }
            }
        }
        AppIcon::FilePlus => {
            rsx! {
                FilePlus { size }
            }
        }
        AppIcon::Folder => {
            rsx! {
                FolderOpen { size }
            }
        }
        AppIcon::FolderGit2 => {
            rsx! {
                FolderGit2 { size }
            }
        }
        AppIcon::FolderPlus => {
            rsx! {
                FolderPlus { size }
            }
        }
        AppIcon::Info => {
            rsx! {
                Info { size }
            }
        }
        AppIcon::GitBranch => {
            rsx! {
                GitBranch { size }
            }
        }
        AppIcon::Worktree => {
            rsx! {
                GitFork { size }
            }
        }
        AppIcon::GoToLine => {
            rsx! {
                ListOrdered { size }
            }
        }
        AppIcon::GoToDefinition => {
            rsx! {
                CornerDownRight { size }
            }
        }
        AppIcon::FindReferences => {
            rsx! {
                ScanSearch { size }
            }
        }
        AppIcon::FormatDocument => {
            rsx! {
                WandSparkles { size }
            }
        }
        AppIcon::LanguageServices => {
            rsx! {
                Blocks { size }
            }
        }
        AppIcon::LineNumbers => {
            rsx! {
                Hash { size }
            }
        }
        AppIcon::Logout => {
            rsx! {
                LogOut { size }
            }
        }
        AppIcon::MatchCase => {
            rsx! {
                CaseSensitive { size }
            }
        }
        AppIcon::MatchWholeWord => {
            rsx! {
                WholeWord { size }
            }
        }
        AppIcon::Menu => {
            rsx! {
                Menu { size }
            }
        }
        AppIcon::More => {
            rsx! {
                Ellipsis { size }
            }
        }
        AppIcon::MoreVertical => {
            rsx! {
                EllipsisVertical { size }
            }
        }
        AppIcon::Play => {
            rsx! {
                Play { size }
            }
        }
        AppIcon::Previous => {
            rsx! {
                ChevronLeft { size }
            }
        }
        AppIcon::Plus => {
            rsx! {
                Plus { size }
            }
        }
        AppIcon::Push => {
            rsx! {
                ArrowUp { size }
            }
        }
        AppIcon::Refresh => {
            rsx! {
                RefreshCw { size }
            }
        }
        AppIcon::Regex => {
            rsx! {
                Regex { size }
            }
        }
        AppIcon::ReplaceAll => {
            rsx! {
                ReplaceAll { size }
            }
        }
        AppIcon::ReplaceNext => {
            rsx! {
                Repeat1 { size }
            }
        }
        AppIcon::Revert => {
            rsx! {
                RotateCcw { size }
            }
        }
        AppIcon::Save => {
            rsx! {
                Save { size }
            }
        }
        AppIcon::Search => {
            rsx! {
                Search { size }
            }
        }
        AppIcon::Settings => {
            rsx! {
                Settings { size }
            }
        }
        AppIcon::Send => {
            rsx! {
                Send { size }
            }
        }
        AppIcon::Share => {
            rsx! {
                Share2 { size }
            }
        }
        AppIcon::ShieldAlert => {
            rsx! {
                ShieldAlert { size }
            }
        }
        AppIcon::Sparkles => {
            rsx! {
                Sparkles { size }
            }
        }
        AppIcon::Bot => {
            rsx! {
                Bot { size }
            }
        }
        AppIcon::Stop => {
            rsx! {
                Square { size }
            }
        }
        AppIcon::NewChat => {
            rsx! {
                SquarePen { size }
            }
        }
        AppIcon::Next => {
            rsx! {
                ChevronRight { size }
            }
        }
        AppIcon::ToggleReplace => {
            rsx! {
                Replace { size }
            }
        }
        AppIcon::Volume2 => {
            rsx! {
                Volume2 { size }
            }
        }
        AppIcon::Undo => {
            rsx! {
                RotateCcw { size }
            }
        }
        AppIcon::Redo => {
            rsx! {
                RefreshCw { size }
            }
        }
        AppIcon::SelectAll => {
            rsx! {
                TextCursorInput { size }
            }
        }
        AppIcon::WordWrap => {
            rsx! {
                TextWrap { size }
            }
        }
        AppIcon::Expand => {
            rsx! {
                ListChevronsUpDown { size }
            }
        }
    }
}
