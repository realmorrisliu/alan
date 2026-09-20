#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

source "$SCRIPT_DIR/install-channel.sh"
alan_install_channel_load "${ALAN_INSTALL_CHANNEL:-stable}"

TARGET_DIR="${ALAN_STANDALONE_TARGET_DIR:-$PROJECT_ROOT/target/standalone-release}"
INSTALL_DIR="${ALAN_CLI_INSTALL_DIR:-${HOME:?}/.local/bin}"
PROFILE="${ALAN_BUILD_PROFILE:-release}"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

if [[ "$PROFILE" != release && "$PROFILE" != debug ]]; then
    fail "ALAN_BUILD_PROFILE must be release or debug"
fi

export CARGO_TARGET_DIR="$TARGET_DIR"
if [[ "${ALAN_SKIP_BUILD:-0}" != 1 ]]; then
    if [[ "$PROFILE" == release ]]; then
        cargo build --locked --release -p alan --bin alan
        cargo build --locked --release -p alan-os-host \
            --bin alan-os-host --bin alan-os-host-dev
    else
        cargo build --locked -p alan --bin alan
        cargo build --locked -p alan-os-host \
            --bin alan-os-host --bin alan-os-host-dev
    fi
fi

BIN_DIR="$TARGET_DIR/$PROFILE"
[[ -x "$BIN_DIR/alan" ]] || fail "missing standalone CLI: $BIN_DIR/alan"
[[ -x "$BIN_DIR/alan-os-host" ]] || fail "missing standalone Host: $BIN_DIR/alan-os-host"
[[ -x "$BIN_DIR/alan-os-host-dev" ]] || fail "missing development Host: $BIN_DIR/alan-os-host-dev"

mkdir -p "$INSTALL_DIR"

same_file() {
    local source="$1"
    local target="$2"
    [[ -f "$target" ]] && cmp -s "$source" "$target"
}

sha256() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        sha256sum "$1" | awk '{print $1}'
    fi
}

install_owned() {
    local source="$1"
    local target="$2"

    if [[ -e "$target" || -L "$target" ]]; then
        if [[ -L "$target" ]]; then
            local resolved
            resolved="$(readlink "$target")"
            if [[ "$resolved" != "$source" && "$resolved" != "$(basename "$source")" ]]; then
                fail "refusing to overwrite non-owned file at $target"
            fi
            rm -f "$target"
        elif same_file "$source" "$target"; then
            return
        else
            fail "refusing to overwrite non-owned file at $target"
        fi
    fi

    install -m 0755 "$source" "$target"
}

if alan_install_channel_is_dev; then
    install_owned "$BIN_DIR/alan" "$INSTALL_DIR/alan-dev"
    install_owned "$BIN_DIR/alan-os-host-dev" "$INSTALL_DIR/alan-os-host-dev"
else
    install_owned "$BIN_DIR/alan" "$INSTALL_DIR/alan"
    install_owned "$BIN_DIR/alan-os-host" "$INSTALL_DIR/alan-os-host"
fi

manifest="$INSTALL_DIR/.alan-cli-manifest-$ALAN_CHANNEL_ID"
if [[ -e "$manifest" && ! -f "$manifest" ]]; then
    fail "refusing to overwrite non-owned file at $manifest"
fi
if alan_install_channel_is_dev; then
    printf 'alan-dev|%s\nalan-os-host-dev|%s\n' \
        "$(sha256 "$INSTALL_DIR/alan-dev")" \
        "$(sha256 "$INSTALL_DIR/alan-os-host-dev")" >"$manifest"
else
    printf 'alan|%s\nalan-os-host|%s\n' \
        "$(sha256 "$INSTALL_DIR/alan")" \
        "$(sha256 "$INSTALL_DIR/alan-os-host")" >"$manifest"
fi

printf 'Installed standalone Alan %s CLI/Host binaries in %s.\n' \
    "$ALAN_CHANNEL_ID" "$INSTALL_DIR"
printf 'System Store: %s\nHost Store: %s\n' \
    "$ALAN_SYSTEM_STORE_DISPLAY" "$ALAN_HOST_STORE_DISPLAY"
