#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export ALAN_INSTALL_CHANNEL="dev"
export ALAN_NOTARIZE="0"
export ALAN_CREATE_RELEASE_ARCHIVE="0"

exec "$SCRIPT_DIR/install.sh" "$@"
