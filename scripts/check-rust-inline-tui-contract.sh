#!/usr/bin/env bash
set -euo pipefail

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

require_file() {
    [[ -f "$1" ]] || fail "missing required file: $1"
}

reject_path() {
    [[ ! -e "$1" ]] || fail "legacy path still exists: $1"
}

reject_pattern() {
    local pattern="$1"
    shift
    if rg -n "$pattern" "$@" >/tmp/alan-rust-inline-tui-contract-rg.txt; then
        cat /tmp/alan-rust-inline-tui-contract-rg.txt >&2
        fail "legacy pattern matched: $pattern"
    fi
}

require_pattern() {
    local pattern="$1"
    shift
    rg -n "$pattern" "$@" >/dev/null || fail "required pattern missing: $pattern"
}

require_file "crates/tui/Cargo.toml"
require_file "crates/tui/src/lib.rs"
require_file "crates/tui/src/daemon_client.rs"
require_file "crates/tui/src/terminal.rs"

reject_path "clients/tui"
reject_path "crates/alan/src/cli/chat.rs"
reject_path "crates/alan/src/cli/ask.rs"
reject_path "scripts/entitlements/alan-tui.entitlements"

reject_pattern 'ALAN_TUI_PATH|clients/tui|\bBun\b|\bInk\b' \
    .github \
    Cargo.toml \
    crates \
    clients/apple \
    README.md \
    AGENTS.md \
    openspec/specs

reject_pattern 'alan chat|alan ask' \
    .github \
    crates \
    clients/apple \
    README.md \
    AGENTS.md \
    openspec/specs

reject_pattern 'TUI Lint / Test / Typecheck|clients/tui|ALAN_TUI_PATH' \
    .github \
    docs/maintainer/github_automation.md

reject_pattern 'ALAN_TUI_NAME|ALAN_TUI_BINARY_OUTFILE|alan-tui\.entitlements|clients/tui' \
    scripts/assemble-release-app.sh \
    scripts/install-channel.sh \
    scripts/install.sh \
    scripts/uninstall.sh \
    scripts/release-check.sh

reject_pattern 'Contents/Resources/bin/alan-tui|binary .*alan-tui' \
    clients/apple \
    packaging \
    README.md \
    AGENTS.md

require_pattern 'alan_tui::run' crates/alan/src/main.rs
require_pattern 'Contents/Resources/bin/alan' scripts/assemble-release-app.sh scripts/validate-release-app.sh
require_pattern 'program: "alan"' clients/apple/alan-macos
