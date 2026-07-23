#!/usr/bin/env bash
set -euo pipefail

export HOME="${HOME:-/home/dev}"
export NPM_CONFIG_PREFIX="${HOME}/.local"
export PATH="${HOME}/.local/bin:${PATH}"

if [ ! -x "${HOME}/.local/bin/pi" ]; then
    mkdir -p "${HOME}/.local"
    cp -R /opt/syntaxis-pi/. "${HOME}/.local/"
fi

exec "$@"
