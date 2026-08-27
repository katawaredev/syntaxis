#!/usr/bin/env bash
set -euo pipefail

export HOME="${HOME:-/home/dev}"
export NPM_CONFIG_PREFIX="${HOME}/.local"
export PATH="${HOME}/.local/share/mise/shims:${HOME}/.local/bin:${PATH}"

# Do not let crashing workspace tools write process-memory dumps into projects.
ulimit -S -c 0

if [ ! -x "${HOME}/.local/bin/pi" ]; then
	mkdir -p "${HOME}/.local"
	cp -R /opt/syntaxis-pi/. "${HOME}/.local/"
fi

if [ -x "${HOME}/.local/bin/pi" ] && ! "${HOME}/.local/bin/pi" --version >/dev/null 2>&1; then
	echo "The persisted Pi installation is unusable; restoring the image version." >&2
	cp -R /opt/syntaxis-pi/. "${HOME}/.local/"
fi

if [ ! -x "${HOME}/.local/bin/pi" ] || ! "${HOME}/.local/bin/pi" --version >/dev/null 2>&1; then
	echo "Pi is unavailable at ${HOME}/.local/bin/pi; rebuild the Syntaxis image to restore it." >&2
	exit 1
fi

exec "$@"
