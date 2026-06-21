#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
SETUP_SCRIPT="$SCRIPT_DIR/setup-local-ghosttykit.sh"

REQUIRE_GHOSTTY="${ALAN_REQUIRE_GHOSTTY_INTEGRATION:-0}"
DERIVED_DATA_PATH="${ALAN_GHOSTTY_DERIVED_DATA_PATH:-$REPO_ROOT/debug/DerivedData/apple-shell-ghostty-integration}"
DESTINATION="${ALAN_GHOSTTY_XCODE_DESTINATION:-platform=macOS}"
CLONED_SOURCE_PACKAGES_DIR="${ALAN_GHOSTTY_CLONED_SOURCE_PACKAGES_DIR:-}"
PACKAGE_ARGS=(-skipPackageUpdates -disableAutomaticPackageResolution)
if [ -n "$CLONED_SOURCE_PACKAGES_DIR" ]; then
    PACKAGE_ARGS+=(-clonedSourcePackagesDirPath "$CLONED_SOURCE_PACKAGES_DIR")
fi

if ! check_output="$("$SETUP_SCRIPT" --check 2>&1)"; then
    printf '%s\n' "$check_output" >&2
    if [ "$REQUIRE_GHOSTTY" = "1" ]; then
        printf '\nerror: Ghostty integration lane requires prepared local artifacts.\n' >&2
        exit 1
    fi

    printf '\nskip: Ghostty integration lane requires prepared local artifacts.\n' >&2
    printf 'hint: run %s first, or set ALAN_REQUIRE_GHOSTTY_INTEGRATION=1 to fail instead of skip.\n' "$SETUP_SCRIPT" >&2
    exit 0
fi

printf '%s\n' "$check_output"
bash "$SCRIPT_DIR/check-ghostty-external-pty-seam.sh"

bash "$SCRIPT_DIR/test-terminal-runtime-service.sh"
bash "$SCRIPT_DIR/test-terminal-surface-controller.sh"

xcodebuild \
    -project "$REPO_ROOT/clients/apple/alan-macos.xcodeproj" \
    -scheme alan-macos \
    -configuration Debug \
    -destination "$DESTINATION" \
    -derivedDataPath "$DERIVED_DATA_PATH" \
    "${PACKAGE_ARGS[@]}" \
    build
