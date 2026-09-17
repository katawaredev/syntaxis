#!/bin/sh
set -eu

repository_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$repository_root"

bun scripts/ensure-js-deps.mjs
bun run build:editor
bun run build:browser-archive
bun run build:browser-git
bun run build:browser-terminal

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
