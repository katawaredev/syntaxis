#!/data/data/com.termux/files/usr/bin/bash
# Run in Termux. Use --latest, --release=vX.Y.Z, or a local archive and checksum.
set -euo pipefail
umask 077

fail() { printf '%s\n' "$*" >&2; exit 1; }
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# Override only for isolated installer tests; normal installs always use Termux HOME.
termux_home="${SYNTAXIS_TERMUX_HOME:-$HOME}"
install_root="$termux_home/.local/share/syntaxis"
state_dir="$termux_home/.local/state/syntaxis"
start=true
app_launch=true
rollback=false
archive=""
release=""
for argument in "$@"; do
    case "$argument" in
        --no-start) start=false ;;
        --no-app-launch) app_launch=false ;;
        --rollback) rollback=true ;;
        --latest) [[ -z "$release" ]] || fail 'Choose one release option.'; release=latest ;;
        --release=*) [[ -z "$release" ]] || fail 'Choose one release option.'; release="${argument#*=}"; [[ "$release" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail 'Use a stable release tag such as --release=v0.13.0.' ;;
        --help) echo 'Usage: bash install.sh [bundle.tar.gz | --latest | --release=vX.Y.Z] [--no-start] [--no-app-launch] [--rollback]'; exit 0 ;;
        --*) fail "Unknown option: $argument" ;;
        *) [[ -z "$archive" ]] || fail 'Provide only one backend archive.'; archive="$argument" ;;
    esac
done
[[ -n "${PREFIX:-}" ]] && command -v pkg >/dev/null || fail 'Run this script inside Termux.'
case "$(dpkg --print-architecture)" in
    aarch64) abi=arm64-v8a ;;
    arm) abi=armeabi-v7a ;;
    *) fail 'Local mode supports ARM64 and ARMv7 Termux installations.' ;;
esac
# Resolve latest once so every asset comes from the same release. Download into
# private temporary storage; incomplete releases never replace a working install.
if [[ -n "$release" ]]; then
    [[ -z "$archive" && "$rollback" == false ]] || fail 'Release downloads cannot be combined with an archive or rollback.'
    command -v curl >/dev/null || fail 'Install curl in Termux first: pkg install curl'
    releases_url=https://github.com/katawaredev/syntaxis/releases
    curl_options=(--fail --silent --show-error --location --proto '=https' --proto-redir '=https' --connect-timeout 15 --max-time 300 --retry 2)
    if [[ "$release" == latest ]]; then
        resolved="$(curl "${curl_options[@]}" --head --output /dev/null --write-out '%{url_effective}' "$releases_url/latest")"
        [[ "$resolved" == "$releases_url/tag/"* ]] || fail 'Could not resolve the latest GitHub release.'
        release="${resolved#"$releases_url/tag/"}"
        [[ "$release" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail 'The latest release is not a stable version.'
    fi
    download_dir="$(mktemp -d)"
    trap 'rm -rf -- "$download_dir"' EXIT
    bundle_name="syntaxis-termux-$abi.tar.gz"
    echo "Downloading Syntaxis $release for $abi…"
    for asset in Android-SHA256SUMS install-syntaxis.sh "$bundle_name" "$bundle_name.sha256"; do
        curl "${curl_options[@]}" --output "$download_dir/$asset" "$releases_url/download/$release/$asset" || fail 'Release downloads are incomplete or unavailable. Wait for Publish Android to finish, then retry. The existing installation was not changed.'
    done
    expected="$(awk '$2 == "install-syntaxis.sh" {print $1}' "$download_dir/Android-SHA256SUMS")"
    actual="$(sha256sum "$download_dir/install-syntaxis.sh")"
    [[ "$expected" =~ ^[a-fA-F0-9]{64}$ && "${expected,,}" == "${actual%% *}" ]] || fail 'Installer checksum mismatch. The existing installation was not changed.'
    install_options=()
    [[ "$start" == true ]] || install_options+=(--no-start)
    [[ "$app_launch" == true ]] || install_options+=(--no-app-launch)
    # Run that release's installer, not an older saved copy's installation logic.
    bash "$download_dir/install-syntaxis.sh" "$download_dir/$bundle_name" "${install_options[@]}"
    exit
fi
if [[ "$rollback" == false ]]; then
    archive="${archive:-$script_dir/syntaxis-termux-$abi.tar.gz}"
    [[ -f "$archive" && -f "$archive.sha256" ]] || fail "Place $archive and its .sha256 checksum beside install.sh, or pass the archive path."
    read -r expected _ < "$archive.sha256"
    [[ "$expected" =~ ^[a-fA-F0-9]{64}$ ]] || fail 'Invalid checksum file.'
    actual="$(sha256sum -- "$archive")"
    actual="${actual%% *}"
    [[ "${expected,,}" == "$actual" ]] || fail 'Checksum mismatch. Download or copy the bundle again.'
fi

command -v flock >/dev/null || pkg install -y util-linux
mkdir -p "$install_root/releases" "$state_dir"
exec 8>"$state_dir/install.lock"
flock -n 8 || fail 'Another Syntaxis installation is running.'
exec 9>"$state_dir/server.lock"
flock -n 9 || fail 'Stop the running Syntaxis backend with Ctrl+C in its Termux session, then run this installer again.'
[[ ! -e "$install_root/current" || -L "$install_root/current" ]] || fail 'The current release path must be a symlink.'


# Install missing commands without upgrading the user's entire Termux environment.
packages=()
for entry in 'node:nodejs' 'npm:npm' 'git:git' 'bash:bash' 'flock:util-linux' 'tar:tar' 'rg:ripgrep' 'fd:fd'; do
    command -v "${entry%%:*}" >/dev/null || packages+=("${entry#*:}")
done
if ((${#packages[@]})); then pkg install -y "${packages[@]}"; fi
if ! node -e 'const [a,b]=process.versions.node.split(".").map(Number); process.exit(a>22 || (a===22 && b>=19) ? 0 : 1)'; then
    node_package=nodejs
    if dpkg-query -W -f='${Status}' nodejs-lts 2>/dev/null | grep -q 'install ok installed'; then node_package=nodejs-lts; fi
    pkg install -y "$node_package"
    node -e 'const [a,b]=process.versions.node.split(".").map(Number); process.exit(a>22 || (a===22 && b>=19) ? 0 : 1)' || fail 'Update Termux repositories: Pi needs Node 22.19 or newer.'
fi

stage=""
pi_stage=""
cleanup() {
    [[ -z "$stage" ]] || rm -rf -- "$stage"
    [[ -z "$pi_stage" ]] || rm -rf -- "$pi_stage"
    rm -f -- "$install_root/.current-$$" "$install_root/.previous-$$" "$install_root/.start-$$" "$install_root/.update-$$"
}
trap cleanup EXIT
previous="$(readlink "$install_root/current" || true)"
if [[ "$rollback" == true ]]; then
    release="$(readlink "$install_root/previous" || true)"
    [[ "$release" =~ ^releases/[a-f0-9]{16}$ && -f "$install_root/$release/start.sh" ]] || fail 'No previous managed release is available.'
else
    # Reject traversal, links, device nodes, and unexpected top-level files before extraction.
    while IFS= read -r entry; do
        entry="${entry#./}"
        [[ -z "$entry" ]] && continue
        [[ "$entry" != /* && "/$entry/" != *'/../'* && "$entry" != *'\'* ]] || fail 'Unsafe archive path.'
        case "$entry" in
            server|start.sh|pi-version.txt|revision.txt|abi.txt|public|public/|public/*) ;;
            *) fail "Unexpected archive entry: $entry" ;;
        esac
    done < <(tar -tzf "$archive")
    tar -tvzf "$archive" | awk 'substr($0,1,1)!="-" && substr($0,1,1)!="d" {exit 1}' || fail 'Archive contains links or special files.'
    stage="$(mktemp -d "$install_root/releases/.install-XXXXXX")"
    tar --no-same-owner --no-same-permissions -xzf "$archive" -C "$stage"
    for file in server start.sh pi-version.txt revision.txt abi.txt public/index.html; do
        [[ -f "$stage/$file" ]] || fail "Incomplete bundle: missing $file"
    done
    [[ "$(cat "$stage/abi.txt")" == "$abi" ]] || fail "This bundle does not match the device architecture ($abi)."
    pi_version="$(cat "$stage/pi-version.txt")"
    [[ "$pi_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?$ ]] || fail 'Invalid Pi version in bundle.'
    mkdir -p "$install_root/pi"
    pi_target="$install_root/pi/$pi_version"
    if [[ ! -x "$pi_target/node_modules/.bin/pi" ]]; then
        pi_stage="$(mktemp -d "$install_root/pi/.install-XXXXXX")"
        npm install --prefix "$pi_stage" --ignore-scripts --no-audit --no-fund "@earendil-works/pi-coding-agent@$pi_version"
        [[ -x "$pi_stage/node_modules/.bin/pi" ]] || fail 'Pi installation did not produce an executable.'
        [[ ! -e "$pi_target" ]] || fail "Incomplete Pi installation at $pi_target; move it aside before retrying."
        mv -- "$pi_stage" "$pi_target"
        pi_stage=""
    fi
    chmod 700 "$stage/server" "$stage/start.sh"
    release="releases/${actual:0:16}"
    if [[ ! -d "$install_root/$release" ]]; then mv -- "$stage" "$install_root/$release"; fi
fi

# Migrate the original flat installation without touching its data or global Pi.
if [[ -z "$previous" && -f "$install_root/server" && -f "$install_root/start.sh" && -d "$install_root/public" ]]; then
    legacy_hash="$(sha256sum "$install_root/server")"
    previous="releases/${legacy_hash:0:16}"
    if [[ ! -d "$install_root/$previous" ]]; then
        mkdir -p "$install_root/$previous"
        cp -a "$install_root/server" "$install_root/public" "$install_root/start.sh" "$install_root/$previous/"
        for file in pi-version.txt revision.txt; do
            [[ ! -f "$install_root/$file" ]] || cp "$install_root/$file" "$install_root/$previous/"
        done
    fi
fi
if [[ -n "$previous" && "$previous" != "$release" ]]; then
    ln -s "$previous" "$install_root/.previous-$$"
    mv -Tf "$install_root/.previous-$$" "$install_root/previous"
fi
ln -s "$release" "$install_root/.current-$$"
mv -Tf "$install_root/.current-$$" "$install_root/current"
cat > "$install_root/.start-$$" <<'LAUNCHER'
#!/data/data/com.termux/files/usr/bin/bash
set -euo pipefail
export SYNTAXIS_INSTALL_ROOT
SYNTAXIS_INSTALL_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
exec "$PREFIX/bin/bash" "$SYNTAXIS_INSTALL_ROOT/current/start.sh" "$@"
LAUNCHER
chmod 700 "$install_root/.start-$$"
mv -f "$install_root/.start-$$" "$install_root/start.sh"
if [[ "$app_launch" == true ]]; then
    mkdir -p "$termux_home/.termux"
    properties="$termux_home/.termux/termux.properties"
    touch "$properties"
    # Replace only this setting; leave other Termux preferences intact.
    sed -i '/^[[:space:]]*allow-external-apps[[:space:]]*=/d' "$properties"
    printf '\nallow-external-apps=true\n' >> "$properties"
    if command -v termux-reload-settings >/dev/null; then termux-reload-settings || true; fi
    echo 'App launching enabled. Grant Syntaxis the Run commands permission when Android asks.'
fi
# Save a stable updater so future updates need neither Downloads nor storage access.
cp -- "${BASH_SOURCE[0]}" "$install_root/.update-$$"
chmod 700 "$install_root/.update-$$"
mv -f "$install_root/.update-$$" "$install_root/update.sh"
cleanup
trap - EXIT
flock -u 9
exec 9>&-
flock -u 8
exec 8>&-
echo "Installed $abi backend. Projects, passwords, and AI sessions were preserved."
if [[ "$start" == true && -s "$termux_home/.config/syntaxis/android-token" ]]; then
    exec "$PREFIX/bin/bash" "$install_root/start.sh"
fi
echo 'Open Syntaxis and tap Start and pair to connect this installation.'
printf 'Start: bash %q\n' "$install_root/start.sh"
