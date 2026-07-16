use dioxus::prelude::*;
use syntaxis_ui::prelude::{Button, ButtonKind, DialogActions, Icon, Modal};
use syntaxis_workspace::WorkspaceRecord;

use crate::{
    terminal::ProjectInitializerTerminal,
    workspace::{api, home::HomeDialog},
};

const BOOTSTRAP_COMMAND: &str = r#"if ! command -v mise >/dev/null 2>&1; then
    echo 'mise is not installed in this runtime.' >&2
    exit 127
fi

has_config=false
for config in mise.toml .mise.toml mise.local.toml .mise.local.toml mise/config.toml .mise/config.toml .config/mise.toml .config/mise/config.toml .tool-versions; do
    if [[ -f "$config" ]]; then
        has_config=true
        break
    fi
done
if compgen -G '.config/mise/conf.d/*.toml' >/dev/null; then
    has_config=true
fi

if [[ "$has_config" == true ]]; then
    echo 'Found a mise-compatible project configuration.'
    mise trust --yes
    mise install --yes
else
    echo 'No mise configuration found; detecting the project toolchain.'
    tools=()

    if [[ -f Cargo.toml || -f rust-toolchain || -f rust-toolchain.toml ]]; then
        tools+=(rust@stable)
    fi
    if [[ -f deno.json || -f deno.jsonc || -f deno.lock ]]; then
        tools+=(deno@latest)
    elif [[ -f bun.lock || -f bun.lockb ]]; then
        tools+=(bun@latest)
    elif [[ -f package.json ]]; then
        tools+=(node@lts)
        if [[ -f pnpm-lock.yaml ]]; then
            tools+=(pnpm@latest)
        elif [[ -f yarn.lock ]]; then
            tools+=(yarn@latest)
        fi
    fi
    if [[ -f pyproject.toml || -f requirements.txt || -f setup.py || -f setup.cfg || -f Pipfile ]]; then
        tools+=(python@latest)
        if [[ -f uv.lock ]]; then
            tools+=(uv@latest)
        fi
    fi
    if [[ -f go.mod || -f go.work ]]; then
        tools+=(go@latest)
    fi
    if [[ -f global.json ]] || compgen -G '*.csproj' >/dev/null || compgen -G '*.sln' >/dev/null; then
        tools+=(dotnet@latest)
    fi
    if [[ -f pom.xml || -f build.gradle || -f build.gradle.kts || -f gradlew ]]; then
        tools+=(java@latest)
    fi
    if [[ -f Gemfile || -f .ruby-version ]]; then
        tools+=(ruby@latest)
    fi
    if [[ -f composer.json ]]; then
        tools+=(php@latest composer@latest)
    fi
    if compgen -G '*.tf' >/dev/null || [[ -f .terraform.lock.hcl ]]; then
        tools+=(terraform@latest)
    fi
    if [[ -f Justfile || -f justfile ]]; then
        tools+=(just@latest)
    fi

    if (( ${#tools[@]} == 0 )); then
        echo 'No supported toolchain markers were found.' >&2
        exit 2
    fi

    printf 'Detected tools:'
    printf ' %s' "${tools[@]}"
    printf '\n'
    if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        exclude_path=$(git rev-parse --git-path info/exclude)
        for ignored in mise.local.toml mise.local.lock; do
            if ! grep -qxF "$ignored" "$exclude_path"; then
                printf '%s\n' "$ignored" >> "$exclude_path"
            fi
        done
    fi
    echo 'Writing mise.local.toml so the inferred setup stays local to this checkout.'
    mise use --yes --env local "${tools[@]}"
fi

echo 'Project toolchain is ready.'"#;

#[component]
pub(super) fn BootstrapProjectDialog(
    index: usize,
    mut dialog: Signal<HomeDialog>,
    workspaces: Vec<WorkspaceRecord>,
    on_notice: EventHandler<String>,
    on_changed: EventHandler<()>,
) -> Element {
    let workspace = workspaces[index].clone();
    let mut started = use_signal(|| false);
    let mut result = use_signal(|| None::<bool>);

    rsx! {
        Modal {
            title: format!("Bootstrap {}", workspace.name),
            description: format!("Install the toolchain required by {} with mise.", workspace.root),
            content_class: if started() { "max-w-180" } else { "" },
            on_close: move |()| {
                if started() && result().is_none() {
                    on_notice.call("Project bootstrap continues in its terminal session".into());
                }
                dialog.set(HomeDialog::None);
            },
            if started() {
                div { class: "px-5 pt-3 pb-5",
                    div { class: "h-[min(34rem,calc(100svh-13rem))] min-h-72 overflow-hidden rounded-lg border border-border bg-background",
                        ProjectInitializerTerminal {
                            workspace_id: workspace.id.0.clone(),
                            workspace_slug: workspace.slug.clone(),
                            command: BOOTSTRAP_COMMAND.to_owned(),
                            label: "Bootstrap with mise".to_owned(),
                            on_finished: {
                                let workspace_id = workspace.id.0.clone();
                                let workspace_name = workspace.name.clone();
                                move |success| {
                                    result.set(Some(success));
                                    let workspace_id = workspace_id.clone();
                                    let workspace_name = workspace_name.clone();
                                    spawn(async move {
                                        let _ = api::refresh_workspace(workspace_id).await;
                                        on_changed.call(());
                                        if success {
                                            on_notice.call(format!("{workspace_name} is ready"));
                                        }
                                    });
                                }
                            },
                        }
                    }
                    p { class: "mt-2.5 flex items-center gap-2 text-xs text-muted-foreground",
                        span { class: if result() == Some(true) { "size-2 rounded-full bg-success" } else if result() == Some(false) { "size-2 rounded-full bg-destructive" } else { "size-2 animate-pulse rounded-full bg-primary" } }
                        if result() == Some(true) {
                            "Bootstrap finished successfully."
                        } else if result() == Some(false) {
                            "Bootstrap exited with an error. Review the terminal output above."
                        } else {
                            "You can leave this dialog while installation continues."
                        }
                    }
                    div { class: "mt-4 flex justify-end",
                        Button {
                            label: "Close",
                            kind: ButtonKind::Primary,
                            onclick: move |_| dialog.set(HomeDialog::None),
                        }
                    }
                }
            } else {
                div { class: "space-y-3 px-5 pt-3 pb-5",
                    div { class: "flex items-start gap-3 rounded-lg border border-border bg-muted/20 p-3",
                        span { class: "mt-0.5 text-primary",
                            Icon {
                                icon: syntaxis_ui::prelude::AppIcon::Terminal,
                                size: 18,
                            }
                        }
                        div {
                            strong { class: "block text-sm text-foreground", "What bootstrap does" }
                            p { class: "mt-1 text-xs leading-relaxed text-muted-foreground",
                                "If the repository has mise.toml or .tool-versions, Syntaxis trusts it and installs its declared tools. Otherwise it detects common project manifests and writes a checkout-local mise.local.toml."
                            }
                        }
                    }
                    p { class: "rounded-md border border-warning/35 bg-warning/10 px-2.5 py-2 text-xs leading-relaxed text-foreground",
                        "Only continue after reviewing the repository. A trusted mise configuration can run project-provided environment hooks."
                    }
                    DialogActions {
                        Button {
                            label: "Cancel",
                            kind: ButtonKind::Ghost,
                            onclick: move |_| dialog.set(HomeDialog::None),
                        }
                        Button {
                            label: "Trust and bootstrap",
                            kind: ButtonKind::Primary,
                            onclick: move |_| started.set(true),
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BOOTSTRAP_COMMAND;

    #[test]
    fn bootstrap_prefers_project_config_and_has_manifest_fallbacks() {
        assert!(BOOTSTRAP_COMMAND.contains("mise trust --yes"));
        assert!(BOOTSTRAP_COMMAND.contains("mise install --yes"));
        assert!(BOOTSTRAP_COMMAND.contains("Cargo.toml"));
        assert!(BOOTSTRAP_COMMAND.contains("package.json"));
        assert!(BOOTSTRAP_COMMAND.contains("pyproject.toml"));
        assert!(BOOTSTRAP_COMMAND.contains("mise use --yes --env local"));
    }
}
