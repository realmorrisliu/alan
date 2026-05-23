#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export ALAN_INSTALL_CHANNEL="dev"

exec "$SCRIPT_DIR/uninstall.sh" "$@"
