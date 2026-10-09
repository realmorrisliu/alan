#!/usr/bin/env bash

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

[[ ! ${ALAN_INSTALL_CHANNEL+x} ]] || fail 'ALAN_INSTALL_CHANNEL is retired; unset it and install alan'

sha256() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        sha256sum "$1" | awk '{print $1}'
    fi
}

# Fixed names only: ownership receipts never authorize an arbitrary path.
manifest_digest() {
    local manifest="$1" wanted="$2" name digest extra found="" seen="|"
    [[ -e "$manifest" || -L "$manifest" ]] || { printf ''; return 0; }
    [[ -f "$manifest" && ! -L "$manifest" ]] || fail "non-regular manifest at $manifest"
    while IFS='|' read -r name digest extra || [[ -n "${name:-}${digest:-}${extra:-}" ]]; do
        [[ -z "${name:-}${digest:-}${extra:-}" ]] && continue
        case "${manifest##*/}:$name" in
            .alan-cli-manifest:alan | .alan-cli-manifest-stable:alan | \
            .alan-cli-manifest-stable:alan-os-host | .alan-cli-manifest-dev:alan-dev | \
            .alan-cli-manifest-dev:alan-os-host-dev) ;;
            *) fail "invalid ownership entry in $manifest" ;;
        esac
        [[ "${digest:-}" =~ ^[[:xdigit:]]{64}$ && -z "${extra:-}" ]] \
            || fail "invalid ownership digest in $manifest"
        [[ "$seen" != *"|$name|"* ]] || fail "duplicate ownership entry in $manifest"
        seen="$seen$name|"
        [[ "$name" != "$wanted" ]] || found="$digest"
    done <"$manifest"
    printf '%s' "$found"
}

preflight_owned_path() {
    local path="$1" expected="$2"
    [[ -e "$path" || -L "$path" ]] || return 0
    [[ -f "$path" && ! -L "$path" ]] || fail "non-regular file at $path"
    [[ -n "$expected" ]] || fail "unowned file at $path"
    [[ "$(sha256 "$path")" == "$expected" ]] || fail "modified owned file at $path"
}

acquire_install_lock() {
    mkdir -p "$INSTALL_DIR"
    install_lock="$INSTALL_DIR/.alan-cli-install.lock"
    mkdir "$install_lock" 2>/dev/null \
        || fail "installation lock exists at $install_lock; stop any installer before recovering an interrupted installation"
    trap 'rmdir "$install_lock"' EXIT
}
