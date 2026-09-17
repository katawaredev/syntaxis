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
rustup show active-toolchain
rustup target add wasm32-unknown-unknown

dx_version=0.7.10
if ! command -v dx >/dev/null 2>&1 || ! dx --version | grep -Eq '^(dx|dioxus|dioxus-cli) 0\.7\.10([[:space:]]|$)'; then
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
      dx_target=x86_64-unknown-linux-gnu
      dx_sha256=4363e4ed2a3f1eb7f4d38d2d59aed59ce43271c44c16b425e92c89a64761fbe7
      ;;
    Linux-aarch64)
      dx_target=aarch64-unknown-linux-gnu
      dx_sha256=8f1a17d3218700ffbe15e6540d936a178b2556fc801121a31082e3ba4ab9ef55
      ;;
    *)
      echo "Install Dioxus CLI $dx_version before running this script on this platform." >&2
      exit 1
      ;;
  esac
  curl --fail --silent --show-error --location \
    "https://github.com/DioxusLabs/dioxus/releases/download/v$dx_version/dx-$dx_target.tar.gz" \
    --output "$tools_dir/dx.tar.gz"
  printf '%s  %s\n' "$dx_sha256" "$tools_dir/dx.tar.gz" | sha256sum --check
  tar -xzf "$tools_dir/dx.tar.gz" -C "$tools_dir/bin"
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
