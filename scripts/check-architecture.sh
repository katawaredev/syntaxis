#!/usr/bin/env bash
set -euo pipefail

if ! command -v rg >/dev/null 2>&1; then
    printf 'Architecture check cannot run: ripgrep (rg) is required but was not found on PATH\n' >&2
    exit 1
fi

failures=0

reject_matches() {
    local description="$1"
    shift
    local output
    if output=$(rg -n "$@" 2>&1); then
        printf 'Architecture boundary violation: %s\n%s\n' "$description" "$output" >&2
        failures=$((failures + 1))
    else
        local status=$?
        if ((status != 1)); then
            printf 'Architecture check failed: %s\n%s\n' "$description" "$output" >&2
            failures=$((failures + 1))
        fi
    fi
}

module_sources=(
    crates/module-files/src
    crates/module-terminal/src
    crates/module-git/src
    crates/module-preview/src
    crates/module-ai/src
)

module_manifests=(
    crates/module-files/Cargo.toml
    crates/module-terminal/Cargo.toml
    crates/module-git/Cargo.toml
    crates/module-preview/Cargo.toml
    crates/module-ai/Cargo.toml
)

reject_matches \
    'shared modules may not import concrete runtimes, browser bindings, or host APIs' \
    '(syntaxis_runtime_[a-z_]+|web_sys|wasm_bindgen|std::fs|std::process|tokio::process|syntaxis_[a-z_]+_host|dioxus::fullstack|document::eval|globalThis|window\.|navigator\.|ServerFnError|#\[(get|post|put|delete)\()' \
    "${module_sources[@]}"

reject_matches \
    'shared module manifests may not depend on runtime, browser-binding, or host crates' \
    '(syntaxis-runtime-[a-z-]+|web-sys|wasm-bindgen|syntaxis-[a-z-]+-host)' \
    "${module_manifests[@]}"

reject_matches \
    'the shared application shell may not import concrete runtimes, browser bindings, host services, or server functions' \
    '(syntaxis[_-]runtime[_-][a-z_-]+|web_sys|web-sys|wasm_bindgen|wasm-bindgen|js_sys|syntaxis_[a-z_]+_host|syntaxis-[a-z-]+-host|dioxus::fullstack|ServerFnError|#\[(get|post|put|delete)\()' \
    crates/app-shell/src crates/app-shell/Cargo.toml

reject_matches \
    'composition packages may not own feature ports, server functions, or host-service selection' \
    '(syntaxis_module_|syntaxis-module-|syntaxis_[a-z_]+_host|syntaxis-[a-z-]+-host|ServerFnError|#\[(get|post|put|delete)\()' \
    apps/server/src apps/browser/src apps/server/Cargo.toml apps/browser/Cargo.toml

reject_matches \
    'composition build scripts may not stage assets owned by shared features or runtime adapters' \
    '(ai-chat\.js|"(ai|files|git|preview|terminal)/)' \
    apps/server/build.rs apps/browser/build.rs

legacy_feature_assets=$(rg --files assets 2>/dev/null \
    | rg '^assets/(ai($|[-/])|code-editor/|files/|git/|(guest|browser)-(archive|git|terminal)/|preview/|terminal/)' \
    || true)
if [[ -n "$legacy_feature_assets" ]]; then
    printf 'Architecture boundary violation: feature/runtime assets must live with their owning crate\n%s\n' \
        "$legacy_feature_assets" >&2
    failures=$((failures + 1))
fi

if ! rg -q 'syntaxis-runtime-remote' apps/server/Cargo.toml; then
    printf 'Architecture boundary violation: apps/server must compose syntaxis-runtime-remote\n' >&2
    failures=$((failures + 1))
fi

if ! rg -q 'syntaxis-runtime-browser' apps/browser/Cargo.toml; then
    printf 'Architecture boundary violation: apps/browser must compose syntaxis-runtime-browser\n' >&2
    failures=$((failures + 1))
fi

reject_matches \
    'modules and the shared shell may not branch on deployment identity' \
    '(AppKind|is_guest|is_main|is_browser|is_server|RuntimeKind::(Guest|Main|Browser|Server))' \
    "${module_sources[@]}" crates/app-shell/src

reject_matches \
    'runtime adapters contain infrastructure only, never feature RSX' \
    'rsx!' \
    crates/runtime-remote/src crates/runtime-browser/src

reject_matches \
    'the browser composition package may not define parallel feature components' \
    '#\[component\][[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?fn[[:space:]]+(Files|Terminal|Git|Preview|Ai|Browser)' \
    -U \
    apps/browser/src

for app in server browser; do
    app_rsx=$(rg -l 'rsx!' "apps/$app/src" --glob '*.rs' | sort || true)
    if [[ "$app_rsx" != "apps/$app/src/app.rs" ]]; then
        printf 'Architecture boundary violation: apps/%s may contain RSX only in its root composition/head file\n%s\n' \
            "$app" "$app_rsx" >&2
        failures=$((failures + 1))
    fi
done

route_declarations=$(rg -l 'derive\([^)]*Routable' apps crates --glob '*.rs' | sort || true)
if [[ "$route_declarations" != "crates/app-shell/src/route.rs" ]]; then
    printf 'Architecture boundary violation: the shared Route must be the only Routable enum\n%s\n' \
        "$route_declarations" >&2
    failures=$((failures + 1))
fi

if ((failures > 0)); then
    exit 1
fi

echo 'Architecture boundaries are intact.'
