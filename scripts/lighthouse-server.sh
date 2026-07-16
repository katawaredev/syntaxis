#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
server="$root/target/dx/syntaxis/release/web/server"
host="${LHCI_HOST:-127.0.0.1}"
port="${LHCI_PORT:-4173}"
log="$root/.lighthouseci/server.log"

if [[ ! -x "$server" ]]; then
    echo "Production server not found: $server" >&2
    echo "Run the Lighthouse build before starting the audit server." >&2
    exit 1
fi

mkdir -p "$root/.lighthouseci"
IP="$host" PORT="$port" "$server" >"$log" 2>&1 &
server_pid=$!

cleanup() {
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

for _ in {1..300}; do
    if ! kill -0 "$server_pid" 2>/dev/null; then
        echo "Production server exited before becoming ready:" >&2
        sed -n '1,160p' "$log" >&2
        wait "$server_pid"
    fi

    if curl --fail --silent --show-error "http://$host:$port/" >/dev/null 2>&1; then
        echo "Lighthouse server ready at http://$host:$port/"
        wait "$server_pid"
        exit $?
    fi

    sleep 0.1
done

echo "Timed out waiting for the production server at http://$host:$port/." >&2
sed -n '1,160p' "$log" >&2
exit 1
