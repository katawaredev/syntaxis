use crate::{AiQuery, Route};
use dioxus::prelude::*;
use syntaxis_module_files::{FilesController, FilesQuery, FilesUiState};
use syntaxis_module_terminal::TerminalQuery;
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, DialogActions, DialogForm, Icon, Modal, ProjectIcon,
    RuntimeStatusPopover, SkipLink, StatusBadge, Tone, WorkspaceHeader, WorkspaceModuleNav,
};
use syntaxis_workspace::{WorkspaceRecord, WorkspaceSection};

#[component]
pub fn WorkspaceShellFrame(
    workspace: WorkspaceRecord,
    active: WorkspaceSection,
    section_title: String,
    runtime_label: String,
    runtime_message: String,
    #[props(default = Tone::Neutral)] runtime_tone: Tone,
    header_actions: Element,
    children: Element,
) -> Element {
    let slug = workspace.slug.clone();
    let page_title = format!("{} · {section_title}", workspace.name);
    let files_ui = use_context::<FilesUiState>();
    let files_controller = use_context::<FilesController>();
    let mut pending_navigation = use_signal(|| None::<Route>);
    let dirty = files_ui.has_dirty();
    let navigator = use_navigator();
    rsx! {
        document::Title { "{page_title}" }
        main { class: "app-viewport flex w-full flex-col overflow-hidden",
            SkipLink { target_id: "workspace-main-content" }
            WorkspaceHeader {
                Link {
                    class: "inline-flex size-8.5 items-center justify-center rounded-lg text-muted-foreground outline-none hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
                    to: Route::Home {},
                    onclick: move |event: MouseEvent| {
                        if dirty {
                            event.prevent_default();
                            pending_navigation.set(Some(Route::Home {}));
                        }
                    },
                    title: "Back to projects",
                    "aria-label": "Back to projects",
                    "←"
                }
                ProjectIcon { name: workspace.name.clone(), icon: workspace.icon.clone(), compact: true }
                div { class: "flex min-w-0 items-center gap-2",
                    strong { class: "truncate text-[13px]", "{workspace.name}" }
                    StatusBadge { label: runtime_label, tone: runtime_tone }
                }
                div { class: "ml-auto flex items-center gap-1 pr-2 text-[11px] text-muted-foreground",
                    RuntimeStatusPopover { message: runtime_message, tone: runtime_tone }
                    {header_actions}
                }
            }
            div { id: "workspace-main-content", tabindex: "-1", class: "min-h-0 flex-1 overflow-hidden", {children} }
            WorkspaceModuleNav {
                ShellNavItem { label: "Files", icon: AppIcon::Folder, active: active == WorkspaceSection::Files, to: Route::Files { slug: slug.clone(), query: FilesQuery::default() }, dirty, pending_navigation }
                ShellNavItem { label: "Terminal", icon: AppIcon::Terminal, active: active == WorkspaceSection::Terminal, to: Route::Terminal { slug: slug.clone(), query: TerminalQuery::default() }, dirty, pending_navigation }
                ShellNavItem { label: "Git", icon: AppIcon::GitBranch, active: active == WorkspaceSection::Git, to: Route::Git { slug: slug.clone() }, dirty, pending_navigation }
                ShellNavItem { label: "Preview", icon: AppIcon::Eye, active: active == WorkspaceSection::Preview, to: Route::Preview { slug: slug.clone() }, dirty, pending_navigation }
                ShellNavItem { label: "AI", icon: AppIcon::Bot, active: active == WorkspaceSection::Ai, to: Route::Ai { slug: slug.clone(), query: AiQuery::default() }, dirty, pending_navigation }
            }
        }
        if let Some(target) = pending_navigation() {
            Modal {
                title: "Leave with unsaved changes?",
                description: "One or more editor documents have unsaved changes.",
                on_close: move |()| pending_navigation.set(None),
                DialogForm {
                    DialogActions {
                        Button { label: "Stay", kind: ButtonKind::Ghost, onclick: move |_| pending_navigation.set(None) }
                        Button { label: "Discard and leave", kind: ButtonKind::Danger, onclick: move |_| {
                            files_ui.reset();
                            files_controller.reset();
                            pending_navigation.set(None);
                            navigator.push(target.clone());
                        } }
                    }
                }
            }
        }
    }
}

#[component]
fn ShellNavItem(
    label: String,
    icon: AppIcon,
    active: bool,
    to: Route,
    dirty: bool,
    mut pending_navigation: Signal<Option<Route>>,
) -> Element {
    let guarded_target = to.clone();
    rsx! {
        Link {
            class: if active { "flex w-26 flex-col items-center justify-center gap-1 border-t-2 border-transparent bg-transparent px-2.5 pt-2 pb-1.5 text-foreground max-md:w-1/5 max-md:pb-2" } else { "flex w-26 flex-col items-center justify-center gap-1 border-t-2 border-transparent bg-transparent px-2.5 pt-2 pb-1.5 text-muted-foreground hover:bg-accent/50 hover:text-foreground max-md:w-1/5 max-md:pb-2" },
            "aria-current": if active { "page" },
            to,
            onclick: move |event: MouseEvent| {
                if dirty && !active {
                    event.prevent_default();
                    pending_navigation.set(Some(guarded_target.clone()));
                }
            },
            span { class: "h-5 text-base leading-5", Icon { icon, size: 18 } }
            small { class: "text-[10px]", "{label}" }
        }
    }
}
