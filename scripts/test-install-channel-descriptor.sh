#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# shellcheck source=scripts/install-channel.sh
source "$SCRIPT_DIR/install-channel.sh"
# shellcheck source=scripts/release-env.sh
source "$SCRIPT_DIR/release-env.sh"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

require_equal() {
    local actual="$1"
    local expected="$2"
    local label="$3"

    [[ "$actual" == "$expected" ]] || fail "$label: expected '$expected', got '$actual'"
}

alan_install_channel_load stable
require_equal "$ALAN_APP_BUNDLE_NAME" "Alan.app" "stable app bundle"
require_equal "$ALAN_DISPLAY_NAME" "Alan" "stable display name"
require_equal "$ALAN_BUNDLE_ID" "app.alanworks.macos" "stable bundle id"
require_equal "$ALAN_PRIVILEGED_HELPER_LABEL" "app.alanworks.macos.privileged-helper" "stable privileged helper label"
require_equal "$ALAN_CLI_NAME" "alan" "stable CLI"
require_equal "$ALAN_SYSTEM_STORE_DISPLAY" "~/Library/Application Support/Alan/System Store/stable" "stable System Store"
require_equal "$ALAN_HOST_STORE_DISPLAY" "~/Library/Application Support/Alan/Host Store/stable" "stable Host Store"
require_equal "$ALAN_SHELL_CONTROL_NAMESPACE" "alan-shell-control" "stable shell namespace"

alan_install_channel_load dev
require_equal "$ALAN_APP_BUNDLE_NAME" "Alan Dev.app" "dev app bundle"
require_equal "$ALAN_DISPLAY_NAME" "Alan Dev" "dev display name"
require_equal "$ALAN_BUNDLE_ID" "app.alanworks.macos.dev" "dev bundle id"
require_equal "$ALAN_PRIVILEGED_HELPER_LABEL" "app.alanworks.macos.dev.privileged-helper" "dev privileged helper label"
require_equal "$ALAN_CLI_NAME" "alan-dev" "dev CLI"
require_equal "$ALAN_SYSTEM_STORE_DISPLAY" "~/Library/Application Support/Alan/System Store/dev" "dev System Store"
require_equal "$ALAN_HOST_STORE_DISPLAY" "~/Library/Application Support/Alan/Host Store/dev" "dev Host Store"
require_equal "$ALAN_SHELL_CONTROL_NAMESPACE" "alan-dev-shell-control" "dev shell namespace"

if alan_install_channel_load nightly 2>/dev/null; then
    fail "unknown install channels must fail"
fi

if ! alan_release_env_allowed_key ALAN_INSTALL_CHANNEL; then
    fail "release env allowlist must include ALAN_INSTALL_CHANNEL"
fi
if ! alan_release_env_allowed_key ALAN_CARGO_TARGET_DIR; then
    fail "release env allowlist must include ALAN_CARGO_TARGET_DIR"
fi
if ! alan_release_env_allowed_key ALAN_BUNDLE_VERSION; then
    fail "release env allowlist must include ALAN_BUNDLE_VERSION"
fi

printf 'Install channel descriptor checks passed.\n'
