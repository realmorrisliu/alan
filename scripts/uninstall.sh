#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/install-channel.sh
source "$SCRIPT_DIR/install-channel.sh"
# shellcheck source=scripts/app-bundle-paths.sh
source "$SCRIPT_DIR/app-bundle-paths.sh"

alan_install_channel_load "${ALAN_INSTALL_CHANNEL:-stable}"

APP_INSTALL_DIR="${ALAN_APP_INSTALL_DIR:-$HOME/Applications}"
APP_TARGET="$APP_INSTALL_DIR/$ALAN_APP_BUNDLE_NAME"
CLI_INSTALL_DIR="${ALAN_CLI_INSTALL_DIR:-/usr/local/bin}"

remove_alan_link() {
    local tool="$1"
    local path="$CLI_INSTALL_DIR/$tool"
    local target

    if [[ ! -L "$path" ]]; then
        return
    fi

    target="$(readlink "$path")"
    case "$target" in
        *"/$ALAN_APP_BUNDLE_NAME/Contents/Resources/bin/$tool")
            rm -f "$path"
            ;;
    esac

}

remove_alan_link "$ALAN_CLI_NAME"
rm -rf "$APP_TARGET"

printf '%s app and PATH symlinks were removed when owned by this install.\n' "$ALAN_DISPLAY_NAME"
printf 'Alan OS data under %s was left intact.\n' "$ALAN_SYSTEM_STORE_DISPLAY"
printf 'Host-owned data under %s was left intact.\n' "$ALAN_HOST_STORE_DISPLAY"
