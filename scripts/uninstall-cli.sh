#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/install-channel.sh"
alan_install_channel_load "${ALAN_INSTALL_CHANNEL:-stable}"

INSTALL_DIR="${ALAN_CLI_INSTALL_DIR:-${HOME:?}/.local/bin}"
MANIFEST="$INSTALL_DIR/.alan-cli-manifest-${ALAN_CHANNEL_ID}"

if [[ ! -f "$MANIFEST" ]]; then
    printf 'No owned standalone install manifest found in %s; nothing removed.\n' "$INSTALL_DIR"
    exit 0
fi

remove_owned() {
    local name="$1"
    local expected="$2"
    local path="$INSTALL_DIR/$name"

    [[ -e "$path" || -L "$path" ]] || return 0
    local actual
    if command -v shasum >/dev/null 2>&1; then
        actual="$(shasum -a 256 "$path" | awk '{print $1}')"
    else
        actual="$(sha256sum "$path" | awk '{print $1}')"
    fi
    if [[ "$actual" != "$expected" ]]; then
        printf 'Preserved modified standalone file: %s\n' "$path" >&2
        return 0
    fi
    rm -f "$path"
}

while IFS='|' read -r name expected; do
    [[ -n "$name" && -n "$expected" ]] && remove_owned "$name" "$expected"
done <"$MANIFEST"
rm -f "$MANIFEST"
printf 'Removed owned standalone %s CLI links from %s; stores were left intact.\n' \
    "$ALAN_CHANNEL_ID" "$INSTALL_DIR"
