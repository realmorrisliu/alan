#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

PATTERN='AlanNative|Alan Shell|Ask Alan|Open in Alan|New Alan|Alan Space|Alan App|alanterm|dev\.alan\.macos|dev\.alan\.native|com\.realmorrisliu\.AlanNative|\balan\.app\b|\balan for macOS\b|CFBundleDisplayName = alan|"INFOPLIST_KEY_CFBundleDisplayName\[sdk=macosx\*\]" = alan|"PRODUCT_NAME\[sdk=macosx\*\]" = alan|path = alan\.app'

is_allowed_occurrence() {
    local line="$1"

    case "$line" in
        openspec/changes/normalize-alan-branding-and-macos-app-name/*)
            return 0
            ;;
        openspec/changes/capitalize-alan-macos-app-brand/*)
            return 0
            ;;
        openspec/specs/product-brand-identity/spec.md:*)
            return 0
            ;;
        openspec/specs/macos-app-architecture-maintainability/spec.md:*)
            return 0
            ;;
        openspec/specs/macos-app-instance-lifecycle/spec.md:*)
            return 0
            ;;
        openspec/specs/macos-shell-build-test-contract/spec.md:*)
            return 0
            ;;
        openspec/specs/macos-shell-ui-ux-conformance/spec.md:*)
            return 0
            ;;
        *"/Users/"*"Developer/Alan"*|*"~/Developer/Alan"*|*"cd Alan"*)
            return 0
            ;;
        *"docs/spec/sub_agent_lifecycle_contract.md:"*"/Users/name/Developer/Alan"*)
            return 0
            ;;
        *'workspace_root": "Alan"'*)
            return 0
            ;;
        *"ShellStatePersistenceStore.swift:"*"AlanNative"*)
            return 0
            ;;
        *"clients/apple/README.md:"*"Application Support/AlanNative"*)
            return 0
            ;;
        *"scripts/install.sh:"*"LEGACY_APP_TARGET"*|*"scripts/install.sh:"*"/alan.app/Contents/Resources/bin/"*)
            return 0
            ;;
        *"scripts/install-channel.sh:"*"ALAN_LEGACY_APP_BUNDLE_NAME=\"alan.app\""*)
            return 0
            ;;
        *"scripts/test-install-channel-descriptor.sh:"*"ALAN_LEGACY_APP_BUNDLE_NAME"*'"alan.app"'*)
            return 0
            ;;
        *"scripts/uninstall.sh:"*"LEGACY_APP_TARGET"*|*"scripts/uninstall.sh:"*"/alan.app/Contents/Resources/bin/"*)
            return 0
            ;;
        *"AlanCommandLineToolInstaller.swift:"*"\"alan.app\""*)
            return 0
            ;;
        *"scripts/test-app-bundle-paths.sh:"*"alan.app"*)
            return 0
            ;;
        *"AlanCommandLineToolInstaller.swift:"*"/alan.app/Contents/Resources/bin/"*)
            return 0
            ;;
        *"clients/apple/scripts/test-command-line-tool-installer.swift:"*"/Applications/alan.app"*)
            return 0
            ;;
        *"clients/apple/scripts/check-brand-identity.sh:"*)
            return 0
            ;;
        *"github.com/realmorrisliu/Alan"*|*"git@github.com:realmorrisliu/Alan.git"*)
            return 0
            ;;
    esac

    return 1
}

violations=0

while IFS= read -r line; do
    rel="${line#$REPO_ROOT/}"
    if is_allowed_occurrence "$rel"; then
        continue
    fi

    printf 'error: non-canonical alan brand occurrence: %s\n' "$rel" >&2
    violations=$((violations + 1))
done < <(
    cd "$REPO_ROOT"
    rg -n --pcre2 "$PATTERN" \
        --glob '!target/**' \
        --glob '!.git/**' \
        --glob '!plans/**' \
        --glob '!openspec/changes/archive/**' \
        --glob '!clients/apple/alan-macos.xcodeproj/project.xcworkspace/**' \
        --glob '!clients/apple/alan-macos.xcodeproj/xcuserdata/**'
)

if (( violations > 0 )); then
    printf 'Brand identity check failed with %d violation(s).\n' "$violations" >&2
    printf 'Use `Alan`, `Alan for macOS`, `Alan.app`, `alanworks.app`, and `app.alanworks.macos` unless the occurrence is explicit command syntax, a machine identifier, or a migration fixture.\n' >&2
    exit 1
fi

printf 'Brand identity check passed.\n'
