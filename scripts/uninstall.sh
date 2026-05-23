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
LEGACY_APP_TARGET=""
if [[ -n "$ALAN_LEGACY_APP_BUNDLE_NAME" ]]; then
    LEGACY_APP_TARGET="$APP_INSTALL_DIR/$ALAN_LEGACY_APP_BUNDLE_NAME"
fi
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

    if [[ -n "$ALAN_LEGACY_APP_BUNDLE_NAME" ]]; then
        case "$target" in
            *"/$ALAN_LEGACY_APP_BUNDLE_NAME/Contents/Resources/bin/$tool")
                rm -f "$path"
                ;;
        esac
    fi
}

remove_alan_link "$ALAN_CLI_NAME"
remove_alan_link "$ALAN_TUI_NAME"
rm -rf "$APP_TARGET"
if [[ -n "$LEGACY_APP_TARGET" ]] && alan_is_distinct_existing_path "$LEGACY_APP_TARGET" "$APP_TARGET"; then
    rm -rf "$LEGACY_APP_TARGET"
fi

printf '%s app and PATH symlinks were removed when owned by this install.\n' "$ALAN_DISPLAY_NAME"
printf 'User data under %s was left intact.\n' "$ALAN_HOME_DISPLAY"
