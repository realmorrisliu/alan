#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT

git_dir="${ALAN_QUALITY_GIT_DIR:-$(git -C "$REPO_ROOT" rev-parse --absolute-git-dir)}"
git_common_dir="$(
    git --git-dir="$git_dir" rev-parse --path-format=absolute --git-common-dir
)"
head="$(git --git-dir="$git_dir" rev-parse HEAD)"

while IFS= read -r git_variable; do
    unset "$git_variable"
done < <(git --git-dir="$git_dir" rev-parse --local-env-vars)

git clone --quiet --shared --no-checkout "$git_common_dir" "$fixture"
git -C "$fixture" checkout --quiet --detach "$head"
cp "$REPO_ROOT/clients/apple/ARCHITECTURE.md" \
    "$fixture/clients/apple/ARCHITECTURE.md"
cp "$REPO_ROOT/clients/apple/scripts/check-architecture-maintainability.sh" \
    "$fixture/clients/apple/scripts/check-architecture-maintainability.sh"
git -C "$fixture" add \
    clients/apple/ARCHITECTURE.md \
    clients/apple/scripts/check-architecture-maintainability.sh
git -C "$fixture" \
    -c user.name="Alan Architecture Test" \
    -c user.email="architecture-test@alan.invalid" \
    commit --quiet --allow-empty -m baseline
git -C "$fixture" \
    -c user.name="Alan Architecture Test" \
    -c user.email="architecture-test@alan.invalid" \
    commit --quiet --allow-empty -m head

probe="$fixture/clients/apple/alan-macos/App/AlanMacUpdateController.swift"
cat >>"$probe" <<'SWIFT'

#if os(macOS)
extension AlanMacUpdateController {
    private static func architectureRatchetTestHelper() {}
}
#endif
SWIFT

ALAN_QUALITY_GIT_DIR= ALAN_QUALITY_BASE_REF=HEAD^ \
    bash "$fixture/clients/apple/scripts/check-architecture-maintainability.sh" \
    --strict >/dev/null

cat >>"$probe" <<'SWIFT'

#if os(macOS)
extension AlanMacUpdateController {
    @MainActor private static
    let architectureRatchetTestStore = ShellStateSnapshot.bootstrapDefault()
}
#endif
SWIFT

if output="$(
    ALAN_QUALITY_GIT_DIR= ALAN_QUALITY_BASE_REF=HEAD^ \
        bash "$fixture/clients/apple/scripts/check-architecture-maintainability.sh" \
        --strict 2>&1
)"; then
    printf 'error: static shell storage probe unexpectedly passed\n' >&2
    exit 1
fi

if [[ "$output" != *"static-storage|App/AlanMacUpdateController.swift"* ]]; then
    printf 'error: static shell storage probe failed for the wrong reason\n%s\n' \
        "$output" >&2
    exit 1
fi

printf 'Architecture maintainability self-test passed.\n'
