#!/usr/bin/env bash
# Run on a Linux development machine; never compile the workspace on the tablet.
set -euo pipefail
repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$repo"

abi="${1:-arm64-v8a}"
case "$abi" in
    arm64-v8a) target=aarch64-linux-android; compiler=aarch64-linux-android26 ;;
    armeabi-v7a) target=armv7-linux-androideabi; compiler=armv7a-linux-androideabi26 ;;
    *) echo 'Usage: build-backend.sh [arm64-v8a|armeabi-v7a]' >&2; exit 2 ;;
esac
ndk="${ANDROID_NDK_HOME:?Set ANDROID_NDK_HOME to an installed Android NDK (r28 or newer).}"
toolchain="$ndk/toolchains/llvm/prebuilt/linux-x86_64/bin"
[[ -x "$toolchain/$compiler-clang" ]] || { echo "Missing NDK compiler: $toolchain/$compiler-clang" >&2; exit 1; }
for tool in rustup cargo dx bun node rg; do
    command -v "$tool" >/dev/null || { echo "Missing development tool: $tool" >&2; exit 1; }
done
if ! rustup target list --installed | rg -qx "$target"; then
    echo "Install the target first: rustup target add $target" >&2
    exit 1
fi
target_key="${target//-/_}"
export "CARGO_TARGET_${target_key^^}_LINKER=$toolchain/$compiler-clang"
export "CC_${target_key}=$toolchain/$compiler-clang"
export "AR_${target_key}=$toolchain/llvm-ar"

bun install --frozen-lockfile
bun run build:editor
bun run build:terminal
bun run generate:pi-settings
# One fullstack build keeps asset hashes and client/server endpoints together.
bundle="$repo/target/dx/syntaxis-server/release/web"
# Dioxus otherwise reprocesses stale compressed assets from earlier builds.
rm -rf -- "$bundle/public"
dx build --package syntaxis-server --platform web --release --locked \
    --debug-symbols false @server --platform server --target "$target"

[[ -f "$bundle/server" && -f "$bundle/public/index.html" ]] || {
    echo "Dioxus did not produce the expected fullstack bundle in $bundle" >&2; exit 1;
}
# Reject an accidental host binary before producing a tablet archive.
machine="$("$toolchain/llvm-readelf" -h "$bundle/server")"
case "$abi:$machine" in
    arm64-v8a:*AArch64*|armeabi-v7a:*ARM*) ;;
    *) echo 'The server binary does not match the requested Android ABI.' >&2; exit 1 ;;
esac
# Capture the full output: rg -q can close the pipe early and make readelf
# fail with SIGPIPE under pipefail, falsely rejecting a valid Android binary.
program_headers="$("$toolchain/llvm-readelf" -l "$bundle/server")"
if [[ "$program_headers" != *'/system/bin/linker'* ]]; then
    echo 'The server is not linked for the Android runtime.' >&2
    exit 1
fi
stage="$(mktemp -d)"
trap 'rm -rf -- "$stage"' EXIT
cp "$bundle/server" "$stage/server"
cp -R "$bundle/public" "$stage/public"
cp apps/Android/termux/start.sh "$stage/start.sh"
chmod 700 "$stage/server" "$stage/start.sh"
git describe --always --dirty > "$stage/revision.txt"
node -p 'require("./package.json").devDependencies["@earendil-works/pi-coding-agent"]' > "$stage/pi-version.txt"
printf '%s\n' "$abi" > "$stage/abi.txt"
archive="$repo/target/syntaxis-termux-$abi.tar.gz"
tar -czf "$archive" -C "$stage" .
(cd "$repo/target" && sha256sum "$(basename "$archive")" > "$(basename "$archive").sha256")
cp apps/Android/termux/install.sh "$repo/target/install-syntaxis.sh"
echo "Backend bundle: $archive"
