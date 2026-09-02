use dioxus::prelude::*;

/// Canonical top bar for an open workspace.
#[component]
pub fn WorkspaceHeader(children: Element) -> Element {
    rsx! {
        header { class: "flex h-[calc(2.875rem+env(safe-area-inset-top))] min-h-[calc(2.875rem+env(safe-area-inset-top))] items-center gap-2 border-b border-border bg-background px-[max(0.625rem,env(safe-area-inset-left))] pt-[env(safe-area-inset-top)] max-md:h-[calc(3rem+env(safe-area-inset-top))] max-md:min-h-[calc(3rem+env(safe-area-inset-top))]",
            {children}
        }
    }
}

/// Canonical bottom navigation frame for workspace modules.
#[component]
pub fn WorkspaceModuleNav(children: Element) -> Element {
    rsx! {
        nav {
            class: "flex h-[calc(3.625rem+env(safe-area-inset-bottom))] min-h-[calc(3.625rem+env(safe-area-inset-bottom))] items-stretch justify-center border-t border-border bg-background pb-[env(safe-area-inset-bottom)] max-md:h-[calc(3.875rem+env(safe-area-inset-bottom))] max-md:min-h-[calc(3.875rem+env(safe-area-inset-bottom))]",
            aria_label: "Workspace modules",
            {children}
        }
    }
}
