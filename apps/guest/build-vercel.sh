#!/bin/sh
set -eu

repository_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$repository_root"

bun scripts/ensure-js-deps.mjs
bun run build:guest-archive
bun run build:guest-terminal

dx build \
  --package syntaxis-guest \
  --platform web \
  --release \
  --locked \
  --debug-symbols false

guest_output="$repository_root/apps/guest/dist"
rm -rf "$guest_output"
mkdir -p "$guest_output"
cp -R "$repository_root/target/dx/syntaxis-guest/release/web/public/." "$guest_output/"
