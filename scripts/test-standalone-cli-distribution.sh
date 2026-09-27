#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
mkdir -p "$PROJECT_ROOT/target"
TEST_ROOT="$(mktemp -d "$PROJECT_ROOT/target/standalone-test.XXXXXX")"
trap 'rm -rf "$TEST_ROOT"' EXIT

export ALAN_STANDALONE_TARGET_DIR="${ALAN_STANDALONE_TARGET_DIR:-$TEST_ROOT/target}"
export ALAN_CLI_INSTALL_DIR="$TEST_ROOT/bin"
export ALAN_BUILD_PROFILE="${ALAN_BUILD_PROFILE:-release}"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

sha256() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        sha256sum "$1" | awk '{print $1}'
    fi
}

expect_install_failure() {
    local install_dir="$1"
    local expected_path="$2"
    local output="$TEST_ROOT/install-error.txt"

    if ALAN_CLI_INSTALL_DIR="$install_dir" ALAN_SKIP_BUILD=1 \
        "$SCRIPT_DIR/install-cli.sh" >"$TEST_ROOT/install-output.txt" 2>"$output"; then
        fail "installer accepted a conflicting path: $expected_path"
    fi
    rg -F -q "$expected_path" "$output" || fail "installer did not report conflict path: $expected_path"
}

"$SCRIPT_DIR/install-cli.sh"
"$TEST_ROOT/bin/alan" --version >/dev/null
[[ ! -e "$TEST_ROOT/bin/alan-os-host" ]] || fail "stable installer created a separate Host executable"

ALAN_INSTALL_CHANNEL=dev "$SCRIPT_DIR/install-cli.sh"
"$TEST_ROOT/bin/alan-dev" --version >/dev/null
[[ ! -e "$TEST_ROOT/bin/alan-os-host-dev" ]] || fail "dev installer created a separate Host executable"

conflict="$TEST_ROOT/conflict"
mkdir -p "$conflict"
printf 'user-owned\n' >"$conflict/alan"
expect_install_failure "$conflict" "$conflict/alan"
[[ "$(cat "$conflict/alan")" == "user-owned" ]] || fail "installer changed an unrelated file"
[[ ! -e "$conflict/.alan-cli-manifest-stable" ]] || fail "installer created a manifest after conflict"

upgrade="$TEST_ROOT/upgrade"
mkdir -p "$upgrade"
printf 'old stable CLI\n' >"$upgrade/alan"
printf 'old stable Host\n' >"$upgrade/alan-os-host"
printf 'old dev CLI\n' >"$upgrade/alan-dev"
printf 'old dev Host\n' >"$upgrade/alan-os-host-dev"
printf 'alan|%s\nalan-os-host|%s\n' \
    "$(sha256 "$upgrade/alan")" "$(sha256 "$upgrade/alan-os-host")" \
    >"$upgrade/.alan-cli-manifest-stable"
printf 'alan-dev|%s\nalan-os-host-dev|%s\n' \
    "$(sha256 "$upgrade/alan-dev")" "$(sha256 "$upgrade/alan-os-host-dev")" \
    >"$upgrade/.alan-cli-manifest-dev"
cp "$upgrade/alan-dev" "$TEST_ROOT/dev-cli-before"
cp "$upgrade/alan-os-host-dev" "$TEST_ROOT/dev-host-before"
cp "$upgrade/.alan-cli-manifest-dev" "$TEST_ROOT/dev-manifest-before"

ALAN_CLI_INSTALL_DIR="$upgrade" ALAN_SKIP_BUILD=1 "$SCRIPT_DIR/install-cli.sh"
cmp -s "$TEST_ROOT/bin/alan" "$upgrade/alan" || fail "upgrade did not replace the owned CLI"
[[ ! -e "$upgrade/alan-os-host" ]] || fail "upgrade retained its owned legacy Host executable"
cmp -s "$TEST_ROOT/dev-cli-before" "$upgrade/alan-dev" || fail "stable upgrade changed the dev CLI"
cmp -s "$TEST_ROOT/dev-host-before" "$upgrade/alan-os-host-dev" || fail "stable upgrade changed the dev Host"
cmp -s "$TEST_ROOT/dev-manifest-before" "$upgrade/.alan-cli-manifest-dev" \
    || fail "stable upgrade changed the dev manifest"
[[ "$(cat "$upgrade/.alan-cli-manifest-stable")" == "alan|$(sha256 "$upgrade/alan")" ]] \
    || fail "upgrade did not write the CLI-only ownership manifest"

modified_host="$TEST_ROOT/modified-host"
mkdir -p "$modified_host"
printf 'owned CLI\n' >"$modified_host/alan"
printf 'modified Host\n' >"$modified_host/alan-os-host"
printf 'recorded old Host\n' >"$TEST_ROOT/recorded-old-host"
printf 'alan|%s\nalan-os-host|%s\n' \
    "$(sha256 "$modified_host/alan")" "$(sha256 "$TEST_ROOT/recorded-old-host")" \
    >"$modified_host/.alan-cli-manifest-stable"
cp "$modified_host/alan" "$TEST_ROOT/modified-cli-before"
cp "$modified_host/alan-os-host" "$TEST_ROOT/modified-host-before"
cp "$modified_host/.alan-cli-manifest-stable" "$TEST_ROOT/modified-manifest-before"
expect_install_failure "$modified_host" "$modified_host/alan-os-host"
cmp -s "$TEST_ROOT/modified-cli-before" "$modified_host/alan" \
    || fail "host conflict partially replaced the CLI"
cmp -s "$TEST_ROOT/modified-host-before" "$modified_host/alan-os-host" \
    || fail "host conflict changed the Host file"
cmp -s "$TEST_ROOT/modified-manifest-before" "$modified_host/.alan-cli-manifest-stable" \
    || fail "host conflict changed the existing manifest"

unowned_host="$TEST_ROOT/unowned-host"
mkdir -p "$unowned_host"
printf 'owned CLI\n' >"$unowned_host/alan"
printf 'user-owned Host\n' >"$unowned_host/alan-os-host"
printf 'alan|%s\n' "$(sha256 "$unowned_host/alan")" >"$unowned_host/.alan-cli-manifest-stable"
cp "$unowned_host/alan" "$TEST_ROOT/unowned-cli-before"
cp "$unowned_host/alan-os-host" "$TEST_ROOT/unowned-host-before"
cp "$unowned_host/.alan-cli-manifest-stable" "$TEST_ROOT/unowned-manifest-before"
expect_install_failure "$unowned_host" "$unowned_host/alan-os-host"
cmp -s "$TEST_ROOT/unowned-cli-before" "$unowned_host/alan" \
    || fail "unowned Host conflict partially replaced the CLI"
cmp -s "$TEST_ROOT/unowned-host-before" "$unowned_host/alan-os-host" \
    || fail "unowned Host conflict changed the Host file"
cmp -s "$TEST_ROOT/unowned-manifest-before" "$unowned_host/.alan-cli-manifest-stable" \
    || fail "unowned Host conflict changed the existing manifest"

ALAN_RELEASE_OUT_DIR="$TEST_ROOT/release" "$SCRIPT_DIR/assemble-cli-release.sh"
archive=("$TEST_ROOT"/release/*.tar.gz)
[[ -f "${archive[0]}" ]] || fail "release archive was not created"
listing="$(tar -tzf "${archive[0]}")"
[[ "$listing" == *"./alan"* && "$listing" == *"./alan-dev"* && "$listing" == *"./manifest.json"* ]] \
    || fail "release archive is missing a CLI entry or manifest"
[[ "$listing" != *"alan-os-host"* ]] || fail "release archive contains a separate Host executable"
manifest_contents="$(tar -xOzf "${archive[0]}" ./manifest.json)"
[[ "$manifest_contents" == *'"binaries": ["alan", "alan-dev"]'* ]] \
    || fail "release manifest does not list only CLI entry points"

printf 'Standalone CLI distribution checks passed.\n'
