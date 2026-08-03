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

exec "$@"
