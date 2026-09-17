#!/bin/sh
set -eu

repository_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$repository_root"

# Vercel does not provision the repository's Rust/Dioxus toolchain.
tools_dir="$repository_root/target/vercel-tools"
mkdir -p "$tools_dir/bin"
export PATH="$tools_dir/bin:${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"

if ! command -v rustup >/dev/null 2>&1; then
  curl --fail --silent --show-error --location https://sh.rustup.rs \
    --output "$tools_dir/rustup-init.sh"
  sh "$tools_dir/rustup-init.sh" -y --profile minimal --default-toolchain none --no-modify-path
fi
# rust-toolchain.toml selects the pinned version, components, and WASM target.
rustup install
rustup show active-toolchain
rustup target add wasm32-unknown-unknown

dx_version=0.7.10
if ! command -v dx >/dev/null 2>&1 || ! dx --version | grep -Eq '^(dx|dioxus|dioxus-cli) 0\.7\.10([[:space:]]|$)'; then
  # Release binaries can require a newer glibc than the Vercel build image.
  # Compile on the host so dx links against its available system libraries.
  # --force also replaces an incompatible binary left by an earlier build.
  cargo install dioxus-cli --version "=$dx_version" --locked --force \
    --root "$tools_dir"
fi
dx --version

if ! command -v bun >/dev/null 2>&1; then
  export BUN_INSTALL="$tools_dir/bun"
  curl --fail --silent --show-error --location https://bun.sh/install \
    --output "$tools_dir/install-bun.sh"
  bash "$tools_dir/install-bun.sh" bun-v1.3.14
  export PATH="$BUN_INSTALL/bin:$PATH"
fi

bun scripts/ensure-js-deps.mjs
bun run build:editor
bun run build:browser-archive
bun run build:browser-git
bun run build:browser-terminal
bun run build:browser-ai

dx build \
  --package syntaxis-browser \
  --platform web \
  --release \
  --locked \
  --debug-symbols false

browser_output="$repository_root/apps/browser/dist"
rm -rf "$browser_output"
mkdir -p "$browser_output"
cp -R "$repository_root/target/dx/syntaxis-browser/release/web/public/." "$browser_output/"
