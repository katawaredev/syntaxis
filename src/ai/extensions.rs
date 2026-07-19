use dioxus::prelude::*;
use syntaxis_ui::prelude::{
    Button, ButtonKind, DangerNote, DialogActions, DialogForm, Modal, Tone,
};

use super::{
    api::{self, PiPackageAction, PiPackageSummary},
    management::ManagementSidebarButton,
};

#[component]
pub(super) fn ExtensionsPanel(
    workspace_id: String,
    mut revision: Signal<u64>,
    mut toast: Signal<Option<(String, Tone)>>,
    sidebar_open: bool,
    on_toggle_sidebar: EventHandler<()>,
    on_open_sidebar: EventHandler<()>,
) -> Element {
    let mut query = use_signal(String::new);
    let mut package_type = use_signal(|| "all".to_owned());
    let mut installation = use_signal(|| "all".to_owned());
    let mut sort = use_signal(|| "downloads".to_owned());
    let mut offset = use_signal(|| 0_usize);
    let mut loaded = use_signal(Vec::<PiPackageSummary>::new);
    let mut catalog_total = use_signal(|| 0_usize);
    let mut next_offset = use_signal(|| 0_usize);
    let mut has_more = use_signal(|| true);
    let pending = use_signal(|| None::<String>);
    let mut confirm = use_signal(|| None::<(PiPackageSummary, PiPackageAction)>);
    let packages_workspace_id = workspace_id.clone();
    let packages = use_resource(move || {
        let workspace_id = packages_workspace_id.clone();
        let query = query();
        let offset = offset();
        let _ = revision();
        async move {
            if !query.is_empty() {
                dioxus_sdk_time::sleep(std::time::Duration::from_millis(250)).await;
            }
            let result = api::pi_packages(workspace_id, query.clone(), offset).await;
            (query, result)
        }
    });
    let result = packages();
    let loading = result.is_none();
    let load_error = result
        .as_ref()
        .and_then(|(_, result)| result.as_ref().err())
        .map(ToString::to_string);
    let empty_error = load_error
        .clone()
        .unwrap_or_else(|| "Could not load packages".to_owned());
    use_effect(move || {
        let Some((resource_query, Ok(search))) = packages() else {
            return;
        };
        if resource_query != query() {
            return;
        }
        let mut merged = if search.start_offset == 0 {
            search.packages.clone()
        } else {
            let mut merged = loaded();
            for package in &search.packages {
                if let Some(existing) = merged.iter_mut().find(|item| item.name == package.name) {
                    *existing = package.clone();
                } else {
                    merged.push(package.clone());
                }
            }
            merged
        };
        merged.sort_by(|left, right| left.name.cmp(&right.name));
        if loaded() != merged {
            loaded.set(merged);
        }
        if catalog_total() != search.catalog_total {
            catalog_total.set(search.catalog_total);
        }
        if next_offset() != search.next_offset {
            next_offset.set(search.next_offset);
        }
        if has_more() != search.has_more {
            has_more.set(search.has_more);
        }
    });
    let visible = filtered_packages(&loaded(), &package_type(), &installation(), &sort());
    let confirm_package = confirm();
    rsx! {
        section { class: "flex h-full min-h-0 flex-col bg-card",
            header { class: "flex min-h-12 items-center gap-3 border-b border-border bg-background px-4",
                ManagementSidebarButton {
                    sidebar_open,
                    on_toggle_sidebar,
                    on_open_sidebar,
                }
                div { class: "min-w-0 flex-1",
                    strong { class: "block text-xs", "Extensions" }
                    small { class: "text-[9px] text-muted-foreground", "Pi packages published to npm" }
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto p-4",
                div { class: "grid grid-cols-[minmax(14rem,1fr)_minmax(9rem,0.35fr)_minmax(9rem,0.35fr)_minmax(10rem,0.4fr)_auto] gap-2 max-xl:grid-cols-2 max-sm:grid-cols-1",
                    input {
                        class: "h-9 min-w-0 rounded-lg border border-input bg-background px-3 text-xs outline-none focus:border-primary",
                        value: query(),
                        placeholder: "Filter packages…",
                        aria_label: "Filter Pi packages",
                        oninput: move |event| {
                            query.set(event.value());
                            reset_results(&mut offset, &mut loaded, &mut has_more);
                        },
                    }
                    select {
                        class: "h-9 rounded-lg border border-input bg-background px-2.5 text-xs",
                        value: package_type(),
                        aria_label: "Package type",
                        onchange: move |event| package_type.set(event.value()),
                        option { value: "all", "All types" }
                        option { value: "extension", "Extensions" }
                        option { value: "skill", "Skills" }
                        option { value: "prompt", "Prompts" }
                        option { value: "theme", "Themes" }
                        option { value: "package", "Packages" }
                    }
                    select {
                        class: "h-9 rounded-lg border border-input bg-background px-2.5 text-xs",
                        value: installation(),
                        aria_label: "Installation status",
                        onchange: move |event| installation.set(event.value()),
                        option { value: "all", "All packages" }
                        option { value: "installed", "Installed" }
                        option { value: "not-installed", "Not installed" }
                    }
                    select {
                        class: "h-9 rounded-lg border border-input bg-background px-2.5 text-xs",
                        value: sort(),
                        aria_label: "Package sorting",
                        onchange: move |event| sort.set(event.value()),
                        option { value: "downloads", "Most downloads" }
                        option { value: "recent", "Recently published" }
                        option { value: "az", "A–Z" }
                    }
                    Button {
                        label: "Reset",
                        kind: ButtonKind::Ghost,
                        onclick: move |_| {
                            query.set(String::new());
                            package_type.set("all".into());
                            installation.set("all".into());
                            sort.set("downloads".into());
                            reset_results(&mut offset, &mut loaded, &mut has_more);
                        },
                    }
                }
                p { class: "py-3 text-[9px] text-muted-foreground",
                    "Showing {visible.len()} of {loaded().len()} loaded packages · {catalog_total()} npm matches"
                }
                if loaded().is_empty() && result.is_none() {
                    p { class: "p-6 text-center text-xs text-muted-foreground", "Loading packages…" }
                } else if loaded().is_empty() && load_error.is_some() {
                    p { class: "p-6 text-center text-xs text-destructive", "{empty_error}" }
                } else {
                    if let Some(error) = load_error.clone() {
                        p { class: "mb-3 rounded-lg bg-destructive/10 p-3 text-xs text-destructive",
                            "{error}"
                        }
                    }
                    if visible.is_empty() {
                        p { class: "rounded-xl border border-border p-8 text-center text-xs text-muted-foreground",
                            "No loaded packages match these filters. Load more or change the filters."
                        }
                    } else {
                        div { class: "grid grid-cols-2 gap-3 max-lg:grid-cols-1",
                            for package in visible {
                                PackageCard {
                                    key: "{package.name}",
                                    package: package.clone(),
                                    pending: pending().as_deref() == Some(package.name.as_str()),
                                    on_manage: move |action| confirm.set(Some((package.clone(), action))),
                                }
                            }
                        }
                    }
                    if has_more() {
                        div { class: "mx-auto mt-4 grid max-w-48",
                            Button {
                                label: if loading { "Loading…" } else { "Load more" },
                                kind: ButtonKind::Ghost,
                                disabled: loading,
                                onclick: move |_| offset.set(next_offset()),
                            }
                        }
                    } else {
                        p { class: "py-4 text-center text-[9px] text-muted-foreground",
                            "End of catalog results"
                        }
                    }
                }
            }
        }
        if let Some((package, action)) = confirm_package {
            PackageConfirmation {
                workspace_id: workspace_id.clone(),
                package,
                action,
                pending,
                confirm,
                revision,
                toast,
                offset,
                loaded,
                has_more,
            }
        }
    }
}

#[component]
fn PackageConfirmation(
    workspace_id: String,
    package: PiPackageSummary,
    action: PiPackageAction,
    mut pending: Signal<Option<String>>,
    mut confirm: Signal<Option<(PiPackageSummary, PiPackageAction)>>,
    mut revision: Signal<u64>,
    mut toast: Signal<Option<(String, Tone)>>,
    mut offset: Signal<usize>,
    mut loaded: Signal<Vec<PiPackageSummary>>,
    mut has_more: Signal<bool>,
) -> Element {
    let uninstalling = action == PiPackageAction::Uninstall;
    let action_label = if uninstalling { "Uninstall" } else { "Install" };
    let package_name = package.name.clone();
    rsx! {
        Modal {
            title: "{action_label} {package_name}?",
            description: if uninstalling { "This removes the user-scoped package from Pi." } else { "Pi packages can execute arbitrary code with the server user's full permissions." },
            on_close: move |()| confirm.set(None),
            DialogForm {
                if !uninstalling {
                    DangerNote { message: "Review the package source before installing. Syntaxis does not sandbox Pi packages." }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        onclick: move |_| confirm.set(None),
                    }
                    Button {
                        label: action_label,
                        kind: if uninstalling { ButtonKind::Danger } else { ButtonKind::Primary },
                        onclick: move |_| {
                            confirm.set(None);
                            pending.set(Some(package_name.clone()));
                            let workspace_id = workspace_id.clone();
                            let name = package_name.clone();
                            spawn(async move {
                                match api::manage_pi_package(workspace_id, name, action).await {
                                    Ok(result) => toast.set(Some((result.message, Tone::Success))),
                                    Err(error) => toast.set(Some((error.to_string(), Tone::Destructive))),
                                }
                                pending.set(None);
                                revision.with_mut(|revision| *revision += 1);
                                reset_results(&mut offset, &mut loaded, &mut has_more);
                            });
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn PackageCard(
    package: PiPackageSummary,
    pending: bool,
    on_manage: EventHandler<PiPackageAction>,
) -> Element {
    let user_installed = package.installed_scopes.iter().any(|scope| scope == "user");
    let project_only = !user_installed
        && package
            .installed_scopes
            .iter()
            .any(|scope| scope == "project");
    let action = if user_installed {
        PiPackageAction::Uninstall
    } else {
        PiPackageAction::Install
    };
    let npm_url = format!("https://www.npmjs.com/package/{}", package.name);
    rsx! {
        article { class: "flex min-h-48 flex-col rounded-xl border border-border bg-background p-4",
            div { class: "flex items-start gap-3",
                div { class: "min-w-0 flex-1",
                    a {
                        href: npm_url,
                        target: "_blank",
                        rel: "noopener noreferrer",
                        class: "break-all text-sm font-semibold text-foreground hover:text-primary hover:underline",
                        "{package.name}"
                    }
                    p { class: "mt-2 line-clamp-3 text-[11px] leading-relaxed text-muted-foreground",
                        "{package.description}"
                    }
                }
                if !package.installed_scopes.is_empty() {
                    span { class: "shrink-0 rounded-md bg-success/12 px-2 py-1 text-[8px] font-medium text-success",
                        "Installed"
                    }
                }
            }
            div { class: "mt-3 flex flex-wrap items-center gap-x-3 gap-y-1 text-[9px] text-muted-foreground",
                span { "{package.publisher}" }
                span { "{format_downloads(package.monthly_downloads)}/mo" }
                span { "{format_published_at(&package.published_at)}" }
            }
            div { class: "mt-3 flex flex-wrap gap-1.5",
                if package.kinds.is_empty() {
                    PackageKind { label: "package" }
                } else {
                    for kind in package.kinds.clone() {
                        PackageKind { label: kind }
                    }
                }
            }
            div { class: "mt-auto flex items-end justify-between gap-3 pt-4",
                small { class: "truncate text-[9px] text-muted-foreground", "v{package.version}" }
                Button {
                    label: if pending { "Working…" } else if project_only { "Project-installed" } else if user_installed { "Uninstall" } else { "Install" },
                    kind: if user_installed { ButtonKind::Danger } else { ButtonKind::Primary },
                    disabled: pending || project_only,
                    onclick: move |_| on_manage.call(action),
                }
            }
        }
    }
}

#[component]
fn PackageKind(label: String) -> Element {
    rsx! {
        span { class: "rounded-md border border-border bg-secondary px-2 py-1 text-[8px] text-muted-foreground uppercase",
            "{label}"
        }
    }
}

fn filtered_packages(
    packages: &[PiPackageSummary],
    package_type: &str,
    installation: &str,
    sort: &str,
) -> Vec<PiPackageSummary> {
    let mut packages = packages
        .iter()
        .filter(|package| match package_type {
            "all" => true,
            "package" => package.kinds.is_empty(),
            kind => package.kinds.iter().any(|candidate| candidate == kind),
        })
        .filter(|package| {
            let installed = !package.installed_scopes.is_empty();
            match installation {
                "installed" => installed,
                "not-installed" => !installed,
                _ => true,
            }
        })
        .cloned()
        .collect::<Vec<_>>();
    match sort {
        "recent" => packages.sort_by(|left, right| {
            right
                .published_at
                .cmp(&left.published_at)
                .then_with(|| left.name.cmp(&right.name))
        }),
        "az" => packages.sort_by(|left, right| left.name.cmp(&right.name)),
        _ => packages.sort_by(|left, right| {
            right
                .monthly_downloads
                .cmp(&left.monthly_downloads)
                .then_with(|| left.name.cmp(&right.name))
        }),
    }
    packages
}

fn reset_results(
    offset: &mut Signal<usize>,
    loaded: &mut Signal<Vec<PiPackageSummary>>,
    has_more: &mut Signal<bool>,
) {
    offset.set(0);
    loaded.set(Vec::new());
    has_more.set(true);
}

fn format_downloads(downloads: u64) -> String {
    match downloads {
        1_000_000.. => format_compact_number(downloads, 1_000_000, "M"),
        1_000.. => format_compact_number(downloads, 1_000, "K"),
        _ => downloads.to_string(),
    }
}

fn format_compact_number(value: u64, divisor: u64, suffix: &str) -> String {
    let whole = value / divisor;
    let decimal = value % divisor / (divisor / 10);
    format!("{whole}.{decimal}{suffix}")
}

fn format_published_at(published_at: &str) -> String {
    published_at
        .split('T')
        .next()
        .unwrap_or(published_at)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(name: &str, downloads: u64, kinds: &[&str], installed: bool) -> PiPackageSummary {
        PiPackageSummary {
            name: name.into(),
            version: "1.0.0".into(),
            description: String::new(),
            publisher: "publisher".into(),
            published_at: format!("2026-01-0{}T00:00:00Z", downloads.min(9)),
            monthly_downloads: downloads,
            kinds: kinds.iter().map(|kind| (*kind).to_owned()).collect(),
            installed_scopes: installed.then(|| "user".into()).into_iter().collect(),
        }
    }

    #[test]
    fn catalog_filters_types_and_installation_state() {
        let packages = [
            package("extension", 1, &["extension"], true),
            package("skill", 2, &["skill"], false),
        ];

        let visible = filtered_packages(&packages, "extension", "installed", "downloads");

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].name, "extension");
    }

    #[test]
    fn catalog_sorting_uses_the_selected_order() {
        let packages = [
            package("alpha", 1, &[], false),
            package("beta", 9, &[], false),
        ];

        assert_eq!(
            filtered_packages(&packages, "all", "all", "downloads")[0].name,
            "beta"
        );
        assert_eq!(
            filtered_packages(&packages, "all", "all", "az")[0].name,
            "alpha"
        );
    }
}
