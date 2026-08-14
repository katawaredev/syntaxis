# Project task runner for Dioxus + Rust.
#
# Common usage:
#   just
#   just install
#   just serve
#   just serve desktop
#   just check
#   just ci
#   just update
#   just lighthouse
#   just docker-dev
#   just docker-prod
#   just dx doctor
#
# Override defaults:
#   just serve web 127.0.0.1 3000
#   just build web release
#   just test "my_test_name"

set shell := ["bash", "-euo", "pipefail", "-c"]
set dotenv-load

# -----------------------------------------------------------------------------
# Configuration
# -----------------------------------------------------------------------------

default_platform := env("DX_PLATFORM", "web")
default_host := env("DX_HOST", "127.0.0.1")
default_port := env("DX_PORT", "8080")
docker_repository := env("SYNTAXIS_DOCKER_REPOSITORY", "ghcr.io/katawaredev/syntaxis")

# -----------------------------------------------------------------------------
# Help
# -----------------------------------------------------------------------------

[default]
help:
    @just --list --unsorted

# -----------------------------------------------------------------------------
# Installation
# -----------------------------------------------------------------------------

# Install the Rust components and target required for Dioxus web development.
install-rust:
    rustup show

# Install cargo-binstall when it is not already available.
install-binstall:
    #!/usr/bin/env bash
    set -euo pipefail

    if command -v cargo-binstall >/dev/null 2>&1; then
        echo "cargo-binstall is already installed"
    else
        cargo install cargo-binstall
    fi

# Install the Dioxus CLI.
install-dx: install-binstall
    cargo binstall --no-confirm --locked dioxus-cli

# Install optional high-value Rust development tools.
install-goodies: install-binstall
    cargo binstall --no-confirm --locked \
        cargo-nextest \
        cargo-machete \
        cargo-deny \
        cargo-edit \
        cargo-expand

# Install lefthook (available as a binary, not via cargo).
install-lefthook:
    #!/usr/bin/env bash
    set -euo pipefail

    repo="evilmartians/lefthook"
    url=$(curl -sL "https://api.github.com/repos/$repo/releases/latest" \
        | grep -o '"browser_download_url": "[^"]*Linux_x86_64"'
        | head -1
        | cut -d'"' -f4)
    curl -sL "$url" -o /usr/local/bin/lefthook
    chmod +x /usr/local/bin/lefthook
    lefthook install

# Install all project development tooling.
install: install-rust install-dx install-goodies install-lefthook install-lighthouse
    @echo
    @echo "Development tooling installed."
    @echo "Run 'just doctor' to verify the environment."

# Update the Cargo-installed development tools.
update-tools: install-binstall
    cargo binstall --no-confirm --locked --force \
        dioxus-cli \
        cargo-nextest \
        cargo-machete \
        cargo-deny \
        cargo-edit \
        cargo-expand
    just install-lefthook

# Install the pinned Lighthouse CI dependency.
install-lighthouse:
    bun install

# Install JavaScript dependencies used by generated browser/Rust assets.
install-js:
    bun scripts/ensure-js-deps.mjs

# Build the terminal bundle when its sources or pinned packages changed.
build-terminal: install-js
    bun run build:terminal

# Build the CodeMirror editor bundle when its sources or pinned packages changed.
build-editor: install-js
    bun run build:editor

# Generate Pi settings metadata from the pinned coding-agent package.
build-pi-settings: install-js
    bun run generate:pi-settings

# Build all npm-backed application assets. Each generator has its own cache key.
build-assets: build-editor build-terminal build-pi-settings

# -----------------------------------------------------------------------------
# Environment inspection
# -----------------------------------------------------------------------------

# Verify the Rust and Dioxus development environment.
doctor:
    @echo "== Rust toolchain =="
    rustup show active-toolchain
    rustc --version
    cargo --version
    @echo
    @echo "== Installed compilation targets =="
    rustup target list --installed
    @echo
    @echo "== Dioxus =="
    dx --version
    dx doctor

# Print versions of all managed tools.
versions:
    #!/usr/bin/env bash
    set -euo pipefail

    commands=(
        rustup
        rustc
        cargo
        dx
        cargo-binstall
        cargo-nextest
        cargo-machete
        cargo-deny
        cargo-upgrade
        cargo-expand
    )

    for command in "${commands[@]}"; do
        if command -v "$command" >/dev/null 2>&1; then
            printf '%-20s ' "$command"
            "$command" --version 2>/dev/null | head -n 1 || echo "installed"
        else
            printf '%-20s %s\n' "$command" "not installed"
        fi
    done

# Interactively update mise tools plus Bun and Cargo dependencies.
# Mise tool pins are bumped to the selected latest versions in mise.toml.
# Use `just update latest` to allow new major dependency versions and rewrite manifests.
update mode="compatible":
    #!/usr/bin/env bash
    set -euo pipefail

    mode="{{ mode }}"
    if [[ "$mode" != "compatible" && "$mode" != "latest" ]]; then
        echo "Expected 'compatible' or 'latest', got: $mode" >&2
        exit 2
    fi

    if [[ "$mode" == "latest" ]] && ! cargo upgrade --version >/dev/null 2>&1; then
        echo "Latest Cargo upgrades require cargo-edit." >&2
        echo "Install it with: just install-goodies" >&2
        exit 1
    fi

    echo "== mise tools =="
    mise upgrade --local --bump --interactive

    echo
    echo "== Bun dependencies =="
    if [[ "$mode" == "latest" ]]; then
        bun update --interactive --latest
    else
        bun update --interactive
    fi

    echo
    if [[ "$mode" == "latest" ]]; then
        echo "== Cargo dependencies (latest versions) =="
        cargo upgrade --dry-run --incompatible allow --pinned allow
        echo
        read -r -p "Rewrite Cargo.toml files and update Cargo.lock? [y/N] " answer
        if [[ "$answer" =~ ^[Yy]$ ]]; then
            cargo upgrade --incompatible allow --pinned allow
        fi
    else
        echo "== Cargo dependencies (within Cargo.toml constraints) =="
        cargo update --dry-run
        echo
        read -r -p "Apply these Cargo.lock updates? [y/N] " answer
        if [[ "$answer" =~ ^[Yy]$ ]]; then
            cargo update
        fi
    fi

# -----------------------------------------------------------------------------
# Dioxus development
# -----------------------------------------------------------------------------

# Generate an Argon2id PHC hash for SYNTAXIS_PASSWORD_HASH.
auth-password:
    cargo run --quiet --no-default-features --features server -- hash-password

# Start the development server.
serve platform=default_platform host=default_host port=default_port: build-assets
    #!/usr/bin/env bash
    set -euo pipefail
    host="{{ host }}"
    if [[ "$host" == "127.0.0.1" || "$host" == "localhost" || "$host" == "::1" ]]; then
        export SYNTAXIS_AUTH_DISABLED=true
    fi
    dx serve \
        --platform "{{ platform }}" \
        --addr "$host" \
        --port "{{ port }}" \
        --force-sequential true

# Start the web development server.
web host=default_host port=default_port: build-assets
    #!/usr/bin/env bash
    set -euo pipefail
    host="{{ host }}"
    if [[ "$host" == "127.0.0.1" || "$host" == "localhost" || "$host" == "::1" ]]; then
        export SYNTAXIS_AUTH_DISABLED=true
    fi
    dx serve \
        --platform web \
        --addr "$host" \
        --port "{{ port }}" \
        --force-sequential true

# Start the desktop development server.
desktop: build-assets
    dx serve --platform desktop

# Start the mobile development server.
mobile: build-assets
    dx serve --platform mobile

serve-local port=default_port: build-assets
    #!/usr/bin/env bash
    set -euo pipefail

    if command -v ufw >/dev/null 2>&1; then
        cleanup() {
            sudo ufw delete allow "{{ port }}/tcp" >/dev/null 2>&1 || true
        }
        trap cleanup EXIT

        sudo ufw allow "{{ port }}/tcp"
    fi

    dx serve \
        --platform web \
        --addr 0.0.0.0 \
        --port "{{ port }}" \
        --force-sequential true

# Build the project.
#
# Examples:
#   just build
#   just build desktop
# just build web release
build platform=default_platform profile="debug": build-assets
    #!/usr/bin/env bash
    set -euo pipefail

    args=(build --platform "{{ platform }}")

    if [[ "{{ profile }}" == "release" ]]; then
        args+=(--release)
    elif [[ "{{ profile }}" != "debug" ]]; then
        echo "Unknown profile: {{ profile }}" >&2
        echo "Expected 'debug' or 'release'." >&2
        exit 2
    fi

    dx "${args[@]}"

# Build an optimized release.
release platform=default_platform: build-assets
    dx build --platform "{{ platform }}" --release

# Build the production web app and run repeatable local Lighthouse audits.
lighthouse:
    #!/usr/bin/env bash
    set -euo pipefail

    if [[ ! -x node_modules/.bin/lhci ]]; then
        bun install
    fi

    bun run lighthouse

# Open the most recent locally collected Lighthouse report.
lighthouse-open:
    bun run lighthouse:open

# -----------------------------------------------------------------------------
# Docker
# -----------------------------------------------------------------------------

# Print the root package version used for Docker image tags.
[private]
docker-version:
    @cargo metadata --no-deps --format-version 1 \
        | bun -e 'const metadata = JSON.parse(await Bun.stdin.text()); const manifest = `${metadata.workspace_root}/Cargo.toml`; console.log(metadata.packages.find((pkg) => pkg.manifest_path === manifest).version);'

# Build a Docker target and tag production with the Cargo package version.
docker-build target="production":
    #!/usr/bin/env bash
    set -euo pipefail

    target="{{ target }}"
    if [[ "$target" != "development" && "$target" != "production" ]]; then
        echo "Expected 'development' or 'production', got: $target" >&2
        exit 2
    fi

    docker="${DOCKER:-docker}"
    platform="${PLATFORM:-linux/amd64}"
    version="$(just docker-version)"
    tags=(-t "syntaxis:${target}")

    if [[ "$target" == "production" ]]; then
        tags+=(
            -t "{{ docker_repository }}:${version}"
            -t "{{ docker_repository }}:latest"
        )
    fi

    "$docker" build \
        --platform "$platform" \
        --target "$target" \
        --file Dockerfile \
        "${tags[@]}" \
        .

# Build and start the hot-reloading development container.
docker-dev port=default_port:
    #!/usr/bin/env bash
    set -euo pipefail

    docker="${DOCKER:-docker}"
    export SYNTAXIS_DEV_PORT="{{ port }}"

    if "$docker" compose version >/dev/null 2>&1; then
        compose=("$docker" compose)
    elif command -v docker-compose >/dev/null 2>&1; then
        compose=(docker-compose)
    else
        echo "Docker Compose is required." >&2
        exit 1
    fi

    mkdir -p data/dev-home
    "${compose[@]}" up --detach --build
    echo "Development server: http://localhost:{{ port }}"
    echo "Follow logs with: ${compose[*]} logs --follow syntaxis"

# Stop and remove the development Compose containers.
docker-dev-stop:
    #!/usr/bin/env bash
    set -euo pipefail

    docker="${DOCKER:-docker}"
    if "$docker" compose version >/dev/null 2>&1; then
        "$docker" compose down
    elif command -v docker-compose >/dev/null 2>&1; then
        docker-compose down
    else
        echo "Docker Compose is required." >&2
        exit 1
    fi

# Build and run the production image locally.
docker-prod port=default_port:
    #!/usr/bin/env bash
    set -euo pipefail

    docker="${DOCKER:-docker}"
    container="syntaxis-prod-test"
    version="$(just docker-version)"
    image="{{ docker_repository }}:${version}"
    host_home="${HOST_HOME:-${HOME}}"
    host_projects="${HOST_PROJECTS:-${HOME}/Projects}"
    data_dir="${DATA:-./data}/prod-home"

    if [[ -z "${SYNTAXIS_PASSWORD_HASH:-}" ]]; then
        echo "SYNTAXIS_PASSWORD_HASH is required; run 'just auth-password' first." >&2
        exit 1
    fi

    if "$docker" container inspect "$container" >/dev/null 2>&1; then
        echo "Container '$container' already exists." >&2
        echo "Stop it with: just docker-prod-stop" >&2
        exit 1
    fi

    just docker-build production
    mkdir -p "$data_dir"

    "$docker" run --detach --rm \
        --name "$container" \
        --user "$(id -u):$(id -g)" \
        --publish "127.0.0.1:{{ port }}:8080" \
        --env HOME=/home/dev \
        --env SHELL=/bin/bash \
        --env SYNTAXIS_PROJECTS_ROOT=/Projects \
        --env SYNTAXIS_PASSWORD_HASH \
        --env SYNTAXIS_API_TOKEN="${SYNTAXIS_API_TOKEN:-}" \
        --env SYNTAXIS_PREVIEW_ORIGIN="${SYNTAXIS_PREVIEW_ORIGIN:-}" \
        --env VERCEL_OIDC_TOKEN="${VERCEL_OIDC_TOKEN:-}" \
        --volume "$data_dir:/home/dev" \
        --volume "$host_home/.ssh:/home/dev/.ssh:ro" \
        --volume "$host_home/.gnupg:/home/dev/.gnupg" \
        --volume "$host_projects:/Projects" \
        "$image"

    echo "Production server: http://localhost:{{ port }}"
    echo "Follow logs with: $docker logs --follow $container"

# Stop the local production test container.
docker-prod-stop:
    #!/usr/bin/env bash
    set -euo pipefail

    docker="${DOCKER:-docker}"
    container="syntaxis-prod-test"

    if "$docker" container inspect "$container" >/dev/null 2>&1; then
        "$docker" stop --time 10 "$container"
    else
        echo "Container '$container' is not running."
    fi

# Run Dioxus checks for one explicit build platform.
dx-check platform=default_platform: build-assets
    #!/usr/bin/env bash
    set -euo pipefail

    case "{{ platform }}" in
        web | server | desktop)
            dx check "--{{ platform }}"
            ;;
        *)
            echo "Dioxus check supports web, server, or desktop; got '{{ platform }}'." >&2
            exit 2
            ;;
    esac

# Format Rust, RSX, JavaScript, CSS, and JSON source.
format:
    cargo fmt --all
    dx fmt
    bun run format:web

# Check formatting without modifying files.
format-check:
    cargo fmt --all -- --check
    dx fmt --check
    bun run format:web:check

# Preview and remove ignored build artifacts while preserving local configuration.
clean:
    #!/usr/bin/env bash
    set -euo pipefail

    exclusions=(
        -e ".env"
        -e ".env.*"
        -e ".envrc"
        -e ".direnv/"
        -e "*.local"
        -e "*.local.*"
    )

    echo "The following ignored files and directories will be removed:"
    git clean -ndX "${exclusions[@]}"
    echo

    read -r -p "Continue? [y/N] " answer
    if [[ "$answer" =~ ^[Yy]$ ]]; then
        git clean -fdX "${exclusions[@]}"
    fi

# Escape hatch for arbitrary Dioxus CLI commands.
#
# Examples:
#   just dx doctor
#   just dx config
# just dx build --platform web --release
dx *args:
    dx {{ args }}

# -----------------------------------------------------------------------------
# Rust quality
# -----------------------------------------------------------------------------

# Run the project's strict Clippy configuration with warnings treated as errors.
clippy platform=default_platform: build-assets
    cargo clippy \
        --workspace \
        --all-targets \
        --no-default-features \
        --features "{{ platform }}" \
        -- -D warnings

# Lint authored JavaScript with Oxlint.
lint-web:
    bun run lint:web

# Run all language-specific lint gates.
lint platform=default_platform: (clippy platform) lint-web

# Run tests using cargo-nextest. The filter remains the first argument for convenience.
test filter="" platform=default_platform: build-assets
    #!/usr/bin/env bash
    set -euo pipefail

    if ! cargo nextest --version >/dev/null 2>&1; then
        echo "cargo-nextest is not installed." >&2
        echo "Run: just install-goodies" >&2
        exit 1
    fi

    args=(
        nextest run
        --workspace
        --no-tests pass
        --no-default-features
        --features "{{ platform }}"
    )
    if [[ -n "{{ filter }}" ]]; then
        args+=("{{ filter }}")
    fi

    cargo "${args[@]}"

# Run standard Cargo tests, including doctests, for one build platform.
test-cargo platform=default_platform: build-assets
    cargo test \
        --workspace \
        --no-default-features \
        --features "{{ platform }}"

# Run doctests, which cargo-nextest does not replace.
test-doc platform=default_platform: build-assets
    #!/usr/bin/env bash
    set -euo pipefail

    if cargo metadata --no-deps --format-version 1 2>/dev/null | grep -q '"lib"'; then
        cargo test \
            --doc \
            --workspace \
            --no-default-features \
            --features "{{ platform }}"
    else
        echo "No library targets found; skipping doctests."
    fi

# Check dependency advisories, bans, and sources. License checks remain disabled
# until the project's policy and dependency exceptions are finalized.
deny:
    #!/usr/bin/env bash
    set -euo pipefail

    if [[ ! -f deny.toml ]]; then
        echo "deny.toml does not exist." >&2
        echo "Initialize it with: just deny-init" >&2
        exit 1
    fi

    cargo deny check advisories bans sources

# Initialize cargo-deny configuration.
deny-init:
    #!/usr/bin/env bash
    set -euo pipefail

    if [[ -e deny.toml ]]; then
        echo "deny.toml already exists; refusing to overwrite it." >&2
        exit 1
    fi

    cargo deny init

# Detect unused Cargo dependencies.
machete:
    cargo machete

# Show the project's dependency tree.
tree *args:
    cargo tree {{ args }}

# Expand Rust macros using the nightly toolchain.
expand *args:
    #!/usr/bin/env bash
    set -euo pipefail

    if ! rustup toolchain list | grep -q '^nightly'; then
        echo "The nightly toolchain is required by this recipe." >&2
        echo "Install it with: rustup toolchain install nightly" >&2
        exit 1
    fi

    cargo +nightly expand {{ args }}

# -----------------------------------------------------------------------------
# Composite workflows
# -----------------------------------------------------------------------------

# Fast, non-mutating local validation for one platform.
check platform=default_platform: format-check (dx-check platform) (lint platform) (test "" platform)

# Full validation suitable for a manually requested CI run.
ci platform=default_platform: format-check (dx-check platform) (lint platform) (test "" platform) (test-doc platform) deny machete
    @echo
    @echo "All quality gates passed."

# Lazy-developer workflow: apply safe fixes, then validate one platform and its doctests.
qa platform=default_platform: build-assets (fix platform) (test-doc platform)
    @echo
    @echo "All code quality gates passed."

# Keep commits responsive: generated assets, formatting, and JavaScript linting only.
# CI owns Rust compilation, Clippy, and tests.
pre-commit: build-assets
    cargo fmt --all -- --check
    dx fmt --check
    bun run format:web:check
    bun run lint:web

# Apply formatting and safe lint fixes, then perform the fast validation workflow.
fix platform=default_platform: build-assets
    cargo fmt --all
    dx fmt
    bun run format:web
    bun run lint:web:fix
    cargo clippy \
        --workspace \
        --all-targets \
        --no-default-features \
        --features "{{ platform }}" \
        --fix --allow-dirty --allow-staged
    just check "{{ platform }}"
