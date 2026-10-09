#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# shellcheck source=scripts/cargo-cli-output.sh
source "$SCRIPT_DIR/cargo-cli-output.sh"

# shellcheck source=scripts/install-channel.sh
source "$SCRIPT_DIR/install-channel.sh"
alan_install_channel_load "${ALAN_INSTALL_CHANNEL:-stable}"

TARGET_DIR="$(alan_cli_target_dir "$PROJECT_ROOT")"
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    alan_cli_build_lease "$PROJECT_ROOT" "$TARGET_DIR" checkout "$SCRIPT_DIR/install-cli.sh"
fi
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
        CLI_SOURCE="$(alan_build_cli "$PROJECT_ROOT" "$TARGET_DIR" --release)"
    else
        CLI_SOURCE="$(alan_build_cli "$PROJECT_ROOT" "$TARGET_DIR")"
    fi
else
    CLI_SOURCE="${ALAN_CLI_SOURCE:-}"
    [[ -n "$CLI_SOURCE" ]] || fail "ALAN_SKIP_BUILD requires the explicit ALAN_CLI_SOURCE from the verified build"
fi
[[ -x "$CLI_SOURCE" ]] || fail "missing standalone CLI: $CLI_SOURCE"

sha256() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        sha256sum "$1" | awk '{print $1}'
    fi
}

manifest="$INSTALL_DIR/.alan-cli-manifest-$ALAN_CHANNEL_ID"
if [[ -e "$manifest" || -L "$manifest" ]]; then
    [[ -f "$manifest" && ! -L "$manifest" ]] || fail "refusing to replace non-regular manifest at $manifest"
fi

manifest_digest() {
    local wanted="$1"
    local name digest extra found=""

    [[ -f "$manifest" ]] || { printf ''; return 0; }
    while IFS='|' read -r name digest extra || [[ -n "${name:-}${digest:-}${extra:-}" ]]; do
        [[ -z "${name:-}${digest:-}${extra:-}" ]] && continue
        [[ "$name" == "$ALAN_CLI_NAME" || "$name" == "$ALAN_OS_HOST_NAME" ]] \
            || fail "invalid ownership manifest entry in $manifest"
        [[ "${digest:-}" =~ ^[[:xdigit:]]{64}$ && -z "${extra:-}" ]] \
            || fail "invalid ownership manifest entry in $manifest"
        if [[ "$name" == "$wanted" ]]; then
            [[ -z "$found" ]] || fail "duplicate ownership record for $name in $manifest"
            found="$digest"
        fi
    done <"$manifest"
    printf '%s' "$found"
}

preflight_owned_path() {
    local path="$1"
    local expected="$2"
    local actual

    [[ -e "$path" || -L "$path" ]] || return 0
    [[ -f "$path" && ! -L "$path" ]] || fail "refusing to replace or retire non-regular file at $path"
    [[ -n "$expected" ]] || fail "refusing to replace or retire unowned file at $path"
    actual="$(sha256 "$path")"
    [[ "$actual" == "$expected" ]] || fail "refusing to replace or retire modified owned file at $path"
}

CLI_PATH="$INSTALL_DIR/$ALAN_CLI_NAME"
HOST_PATH="$INSTALL_DIR/$ALAN_OS_HOST_NAME"
CLI_OLD_DIGEST="$(manifest_digest "$ALAN_CLI_NAME")"
HOST_OLD_DIGEST="$(manifest_digest "$ALAN_OS_HOST_NAME")"
preflight_owned_path "$CLI_PATH" "$CLI_OLD_DIGEST"
preflight_owned_path "$HOST_PATH" "$HOST_OLD_DIGEST"

mkdir -p "$INSTALL_DIR"
stage="$(mktemp -d "$INSTALL_DIR/.alan-cli-install.XXXXXX")"
keep_stage=0
cleanup() {
    if [[ "$keep_stage" != 1 ]]; then
        rm -rf "$stage"
    fi
}
trap cleanup EXIT

install -m 0755 "$CLI_SOURCE" "$stage/new-cli"
new_digest="$(sha256 "$stage/new-cli")"
printf '%s|%s\n' "$ALAN_CLI_NAME" "$new_digest" >"$stage/new-manifest"

cli_existed=0
if [[ -e "$CLI_PATH" ]]; then
    cp -p "$CLI_PATH" "$stage/old-cli"
    cli_existed=1
fi

rollback() {
    local rollback_failed=0

    if [[ ! -e "$stage/new-cli" ]]; then
        if [[ "$cli_existed" == 1 ]]; then
            if ! mv "$stage/old-cli" "$CLI_PATH"; then
                rollback_failed=1
            fi
        elif [[ -e "$CLI_PATH" ]]; then
            if [[ "$(sha256 "$CLI_PATH")" == "$new_digest" ]]; then
                rm -f "$CLI_PATH" || rollback_failed=1
            else
                rollback_failed=1
            fi
        fi
    fi

    if [[ -e "$stage/retired-host" ]]; then
        if [[ -e "$HOST_PATH" || -L "$HOST_PATH" ]]; then
            rollback_failed=1
        elif ! mv "$stage/retired-host" "$HOST_PATH"; then
            rollback_failed=1
        fi
    fi

    if [[ "$rollback_failed" == 1 ]]; then
        keep_stage=1
        printf 'error: upgrade rollback was incomplete; recovery files are in %s\n' "$stage" >&2
    else
        printf 'error: upgrade failed; previous executables were restored\n' >&2
    fi
}

handle_signal() {
    local exit_status="$1"
    if [[ -e "$stage/new-manifest" ]]; then
        rollback
    fi
    exit "$exit_status"
}
trap 'handle_signal 130' INT
trap 'handle_signal 129' HUP
trap 'handle_signal 143' TERM

if [[ -e "$HOST_PATH" ]]; then
    if ! mv "$HOST_PATH" "$stage/retired-host"; then
        rollback
        fail "unable to retire previous Host executable at $HOST_PATH"
    fi
fi
if ! mv "$stage/new-cli" "$CLI_PATH"; then
    rollback
    fail "unable to install CLI at $CLI_PATH"
fi
if ! mv "$stage/new-manifest" "$manifest"; then
    rollback
    fail "unable to update ownership manifest at $manifest"
fi

printf 'Installed standalone Alan %s CLI in %s.\n' \
    "$ALAN_CHANNEL_ID" "$INSTALL_DIR"
printf 'System Store: %s\nHost Store: %s\n' \
    "$ALAN_SYSTEM_STORE_DISPLAY" "$ALAN_HOST_STORE_DISPLAY"
