#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/install-ownership.sh
source "$SCRIPT_DIR/install-ownership.sh"
INSTALL_DIR="${ALAN_CLI_INSTALL_DIR:-${HOME:?}/.local/bin}"
acquire_install_lock
manifest="$INSTALL_DIR/.alan-cli-manifest"
if [[ ! -e "$manifest" && ! -L "$manifest" ]]; then
    printf 'No owned Alan installation in %s; nothing removed.\n' "$INSTALL_DIR"
    exit 0
fi
digest="$(manifest_digest "$manifest" alan)"
preflight_owned_path "$INSTALL_DIR/alan" "$digest"
rm -f "$INSTALL_DIR/alan"
rm -f "$manifest"
printf 'Removed owned Alan executable from %s; stores were left intact.\n' "$INSTALL_DIR"
