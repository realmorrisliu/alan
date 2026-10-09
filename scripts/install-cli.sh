#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# shellcheck source=scripts/cargo-cli-output.sh
source "$SCRIPT_DIR/cargo-cli-output.sh"

# shellcheck source=scripts/install-ownership.sh
source "$SCRIPT_DIR/install-ownership.sh"

TARGET_DIR="$(alan_cli_target_dir "$PROJECT_ROOT")"
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    alan_cli_build_lease "$PROJECT_ROOT" "$TARGET_DIR" checkout "$SCRIPT_DIR/install-cli.sh"
fi
INSTALL_DIR="${ALAN_CLI_INSTALL_DIR:-${HOME:?}/.local/bin}"
PROFILE="${ALAN_BUILD_PROFILE:-release}"

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

acquire_install_lock
manifest="$INSTALL_DIR/.alan-cli-manifest"
names=(alan alan-os-host alan-dev alan-os-host-dev)
digests=()
for name in "${names[@]}"; do
    expected=""
    for receipt in "$manifest" "$manifest-stable" "$manifest-dev"; do
        digest="$(manifest_digest "$receipt" "$name")"
        if [[ -n "$digest" ]]; then
            [[ -z "$expected" ]] || fail "conflicting ownership receipts for $INSTALL_DIR/$name"
            expected="$digest"
        fi
    done
    preflight_owned_path "$INSTALL_DIR/$name" "$expected"
    digests+=("$expected")
done

stage="$(mktemp -d "$INSTALL_DIR/.alan-cli-install.XXXXXX")"
keep_stage=0
cleanup() {
    [[ "$keep_stage" == 1 ]] || rm -rf "$stage"
    rmdir "$install_lock"
}
trap cleanup EXIT
mkdir "$stage/old"
install -m 0755 "$CLI_SOURCE" "$stage/new-cli"
new_digest="$(sha256 "$stage/new-cli")"
printf 'alan|%s\n' "$new_digest" >"$stage/new-manifest"

rollback() {
    local rollback_failed=0 path saved
    if [[ ! -e "$stage/new-cli" && -e "$INSTALL_DIR/alan" ]]; then
        if [[ -f "$INSTALL_DIR/alan" && ! -L "$INSTALL_DIR/alan" \
            && "$(sha256 "$INSTALL_DIR/alan")" == "$new_digest" ]]; then
            rm -f "$INSTALL_DIR/alan" || rollback_failed=1
        else
            rollback_failed=1
        fi
    fi
    for saved in "$stage/old/"* "$stage/old/".alan-cli-manifest*; do
        [[ -e "$saved" ]] || continue
        path="$INSTALL_DIR/${saved##*/}"
        if [[ -e "$path" || -L "$path" ]]; then
            rollback_failed=1
        elif ! mv "$saved" "$path"; then
            rollback_failed=1
        fi
    done
    if [[ "$rollback_failed" == 1 ]]; then
        keep_stage=1
        printf 'error: rollback incomplete; recovery files retained at %s\n' "$stage" >&2
    else
        printf 'error: installation failed; previous executables and receipts restored\n' >&2
    fi
}
handle_signal() {
    [[ ! -e "$stage/new-manifest" ]] || rollback
    exit "$1"
}
trap 'handle_signal 130' INT
trap 'handle_signal 129' HUP
trap 'handle_signal 143' TERM

for ((index=0; index<${#names[@]}; index++)); do
    path="$INSTALL_DIR/${names[index]}"
    if ! (preflight_owned_path "$path" "${digests[index]}"); then
        rollback
        fail "ownership changed before retiring $path"
    fi
    if [[ -e "$path" ]]; then
        mv "$path" "$stage/old/${names[index]}" || { rollback; fail "unable to retire $path"; }
    fi
done
for receipt in "$manifest" "$manifest-stable" "$manifest-dev"; do
    if [[ -e "$receipt" ]]; then
        mv "$receipt" "$stage/old/${receipt##*/}" || { rollback; fail "unable to retire $receipt"; }
    fi
done
mv "$stage/new-cli" "$INSTALL_DIR/alan" || { rollback; fail 'unable to publish alan'; }
mv "$stage/new-manifest" "$manifest" || { rollback; fail 'unable to publish ownership receipt'; }
printf 'Installed standalone Alan in %s. Product stores were left intact.\n' "$INSTALL_DIR"
