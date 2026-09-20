#!/usr/bin/env bash
# Collect already-built artifacts; intentionally does not build or sign them.
set -euo pipefail
repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
output="$repo/target/android-distribution"
apk="$repo/apps/Android/app/build/outputs/apk/debug/app-debug.apk"
[[ -f "$apk" ]] || { echo 'Build the debug APK first.' >&2; exit 1; }
for abi in arm64-v8a armeabi-v7a; do
    archive="syntaxis-termux-$abi.tar.gz"
    [[ -f "$repo/target/$archive.sha256" ]] || { echo "Build $abi first." >&2; exit 1; }
    (cd "$repo/target" && sha256sum -c "$archive.sha256")
done
mkdir -p "$output"
cp "$apk" "$output/syntaxis-debug.apk"
for abi in arm64-v8a armeabi-v7a; do
    cp "$repo/target/syntaxis-termux-$abi.tar.gz"{,.sha256} "$output/"
done
cp "$repo/apps/Android/termux/install.sh" "$output/install-syntaxis.sh"
cp "$repo/apps/Android/README.md" "$output/README.md"
(cd "$output" && sha256sum syntaxis-debug.apk install-syntaxis.sh > SHA256SUMS)
printf 'Distribution files: %s\n' "$output"
