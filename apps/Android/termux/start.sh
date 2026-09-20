#!/data/data/com.termux/files/usr/bin/bash
set -euo pipefail
umask 077

install_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
install_root="${SYNTAXIS_INSTALL_ROOT:-$install_dir}"
termux_home="${SYNTAXIS_TERMUX_HOME:-$HOME}"
config_dir="$termux_home/.config/syntaxis"
password_file="$config_dir/password.hash"

mkdir -p "$config_dir" "$termux_home/Projects" "$termux_home/.local/state/syntaxis"
# Share the lock with the installer, including initial password creation.
command -v flock >/dev/null || { echo "Install util-linux in Termux first." >&2; exit 1; }
exec 9>"$termux_home/.local/state/syntaxis/server.lock"
if ! flock -n 9; then
    echo 'The local backend is running or being updated. Return to Syntaxis and retry Local.'
    exit 0
fi

if [[ ! -x "$install_dir/server" || ! -f "$install_dir/public/index.html" ]]; then
    echo 'Install the matching Android backend bundle in ~/.local/share/syntaxis first.' >&2
    exit 1
fi
for tool in node git bash flock; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "Missing Termux tool: $tool. See apps/Android/README.md." >&2
        exit 1
    fi
done
pi_command=""
if [[ -f "$install_dir/pi-version.txt" ]]; then
    pi_version="$(cat "$install_dir/pi-version.txt")"
    [[ "$pi_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?$ ]] || { echo 'Invalid Pi version in backend bundle.' >&2; exit 1; }
    managed_pi="$install_root/pi/$pi_version/node_modules/.bin/pi"
    [[ ! -x "$managed_pi" ]] || pi_command="$managed_pi"
fi
# Older flat installations keep working with their existing global Pi.
if [[ -z "$pi_command" ]]; then pi_command="$(command -v pi || true)"; fi
[[ -n "$pi_command" ]] || { echo 'Pi is missing. Run the Termux installer again.' >&2; exit 1; }
node -e 'const [major, minor] = process.versions.node.split(".").map(Number); if (major < 22 || (major === 22 && minor < 19)) { console.error("Pi requires Node >=22.19.0"); process.exit(1); }'

if [[ ! -s "$password_file" ]]; then
    password_hash="$("$install_dir/server" random-password-hash)"
    printf '%s\n' "$password_hash" > "$password_file"
fi

token_file="$config_dir/android-token"
if [[ "${1:-}" == --app-token ]]; then
    [[ "$#" == 2 && "$2" =~ ^[a-f0-9]{64}$ ]] || { echo 'Invalid app pairing token.' >&2; exit 1; }
    printf '%s\n' "$2" > "$token_file"
elif [[ "$#" != 0 ]]; then
    echo 'Usage: start.sh [--app-token TOKEN]' >&2
    exit 1
fi
if [[ ! -s "$token_file" ]]; then
    node -e 'console.log(require("crypto").randomBytes(32).toString("hex"))' > "$token_file"
fi
export SYNTAXIS_API_TOKEN
SYNTAXIS_API_TOKEN="$(cat "$token_file")"
export SYNTAXIS_PASSWORD_HASH
SYNTAXIS_PASSWORD_HASH="$(cat "$password_file")"
export SYNTAXIS_INSECURE_COOKIE=true
export SYNTAXIS_PROJECTS_ROOT="$termux_home/Projects"
export SYNTAXIS_WORKSPACE_ROOTS="$termux_home/Projects"
export SYNTAXIS_DATA_DIR="$termux_home/.local/state/syntaxis"
export SYNTAXIS_PI_COMMAND
SYNTAXIS_PI_COMMAND="$pi_command"
export SHELL="$PREFIX/bin/bash"
export DIOXUS_PUBLIC_PATH="$install_dir/public"
export IP=127.0.0.1
export PORT=8787
unset SYNTAXIS_AUTH_DISABLED

cd "$termux_home/Projects"
echo 'Syntaxis Local: http://127.0.0.1:8787 (leave this Termux session running)'
exec "$install_dir/server"
