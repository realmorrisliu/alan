#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"
# shellcheck source=scripts/cargo-cli-output.sh
source "$SCRIPT_DIR/cargo-cli-output.sh"
mkdir -p "$PROJECT_ROOT/target"
TEST_ROOT="$(mktemp -d "$PROJECT_ROOT/target/standalone-test.XXXXXX")"
trap 'rm -rf "$TEST_ROOT"' EXIT

ALAN_STANDALONE_TARGET_DIR="$(alan_cli_target_dir "$PROJECT_ROOT")"
export ALAN_STANDALONE_TARGET_DIR
export ALAN_CLI_INSTALL_DIR="$TEST_ROOT/bin"
export ALAN_BUILD_PROFILE="${ALAN_BUILD_PROFILE:-release}"

if [[ "${ALAN_SKIP_BUILD:-0}" != 1 ]]; then
    build_flags=()
    [[ "$ALAN_BUILD_PROFILE" != release ]] || build_flags+=(--release)
    ALAN_CLI_SOURCE="$(alan_build_cli "$PROJECT_ROOT" "$ALAN_STANDALONE_TARGET_DIR" "${build_flags[@]}")"
    export ALAN_CLI_SOURCE
    export ALAN_SKIP_BUILD=1
fi

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

[[ ! -e "$TEST_ROOT/bin/alan-dev" ]] || fail "installer created a dev alias"
"$SCRIPT_DIR/install-cli.sh"
if ALAN_INSTALL_CHANNEL=dev "$SCRIPT_DIR/install-cli.sh" >"$TEST_ROOT/obsolete-output" 2>&1; then
    fail "installer accepted retired channel selection"
fi

locked="$TEST_ROOT/locked"
mkdir -p "$locked/.alan-cli-install.lock"
expect_install_failure "$locked" "$locked/.alan-cli-install.lock"
[[ ! -e "$locked/alan" ]] || fail "installer ignored another installer lock"

conflict="$TEST_ROOT/conflict"
mkdir -p "$conflict"
printf 'user-owned\n' >"$conflict/alan"
expect_install_failure "$conflict" "$conflict/alan"
[[ "$(cat "$conflict/alan")" == "user-owned" ]] || fail "installer changed an unrelated file"
[[ ! -e "$conflict/.alan-cli-manifest" ]] || fail "installer created a manifest after conflict"

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
for retired in alan-dev alan-os-host-dev .alan-cli-manifest-stable .alan-cli-manifest-dev; do
    [[ ! -e "$upgrade/$retired" ]] || fail "upgrade retained owned $retired"
done
[[ "$(cat "$upgrade/.alan-cli-manifest")" == "alan|$(sha256 "$upgrade/alan")" ]] \
    || fail "upgrade did not write the canonical ownership manifest"
for source in stable dev; do
    single="$TEST_ROOT/only-$source"
    mkdir -p "$single"
    name=alan
    [[ "$source" != dev ]] || name=alan-dev
    printf 'old CLI\n' >"$single/$name"
    printf '%s|%s\n' "$name" "$(sha256 "$single/$name")" >"$single/.alan-cli-manifest-$source"
    ALAN_CLI_INSTALL_DIR="$single" "$SCRIPT_DIR/install-cli.sh"
    [[ -x "$single/alan" && ! -e "$single/alan-dev" ]] || fail "$source-only upgrade failed"
done

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

signal_bin="$TEST_ROOT/signal-bin"
mkdir -p "$signal_bin"
cat >"$signal_bin/mv" <<'EOF'
#!/usr/bin/env bash
/bin/mv "$@"
status=$?
if [[ "$status" == 0 && "${2:-}" == "${ALAN_TEST_SIGNAL_AFTER_MV_DEST:-}" \
    && ! -e "${ALAN_TEST_SIGNAL_SENT:-}" ]]; then
    : >"$ALAN_TEST_SIGNAL_SENT"
    kill -s "$ALAN_TEST_SIGNAL_NAME" "$PPID"
fi
exit "$status"
EOF
chmod +x "$signal_bin/mv"
for signal_name in TERM HUP INT; do
    signal_upgrade="$TEST_ROOT/signal-upgrade-$signal_name"
    mkdir -p "$signal_upgrade"
    printf 'old CLI before interruption\n' >"$signal_upgrade/alan"
    printf 'old Host before interruption\n' >"$signal_upgrade/alan-os-host"
    printf 'old dev CLI before interruption\n' >"$signal_upgrade/alan-dev"
    printf 'alan-dev|%s\n' "$(sha256 "$signal_upgrade/alan-dev")" >"$signal_upgrade/.alan-cli-manifest-dev"
    cp "$signal_upgrade/alan-dev" "$TEST_ROOT/signal-dev-before"
    cp "$signal_upgrade/.alan-cli-manifest-dev" "$TEST_ROOT/signal-dev-manifest-before"
    printf 'alan|%s\nalan-os-host|%s\n' \
        "$(sha256 "$signal_upgrade/alan")" "$(sha256 "$signal_upgrade/alan-os-host")" \
        >"$signal_upgrade/.alan-cli-manifest-stable"
    cp "$signal_upgrade/alan" "$TEST_ROOT/signal-cli-before"
    cp "$signal_upgrade/alan-os-host" "$TEST_ROOT/signal-host-before"
    cp "$signal_upgrade/.alan-cli-manifest-stable" "$TEST_ROOT/signal-manifest-before"
    if PATH="$signal_bin:$PATH" \
        ALAN_CLI_INSTALL_DIR="$signal_upgrade" \
        ALAN_SKIP_BUILD=1 \
        ALAN_TEST_SIGNAL_AFTER_MV_DEST="$signal_upgrade/alan" \
        ALAN_TEST_SIGNAL_SENT="$TEST_ROOT/signal-sent-$signal_name" \
        ALAN_TEST_SIGNAL_NAME="$signal_name" \
        "$SCRIPT_DIR/install-cli.sh" >"$TEST_ROOT/signal-output.txt" 2>&1; then
        fail "installer ignored SIG$signal_name during the upgrade"
    else
        signal_status=$?
        expected_status=143
        [[ "$signal_name" == HUP ]] && expected_status=129
        [[ "$signal_name" == INT ]] && expected_status=130
        [[ "$signal_status" == "$expected_status" ]] \
            || fail "installer exited with unexpected SIG$signal_name status: $signal_status"
    fi
    cmp -s "$TEST_ROOT/signal-cli-before" "$signal_upgrade/alan" \
        || fail "SIG$signal_name left the old CLI replaced"
    cmp -s "$TEST_ROOT/signal-host-before" "$signal_upgrade/alan-os-host" \
        || fail "SIG$signal_name left the old Host removed"
    cmp -s "$TEST_ROOT/signal-dev-before" "$signal_upgrade/alan-dev" \
        || fail "SIG$signal_name changed old dev CLI"
    cmp -s "$TEST_ROOT/signal-dev-manifest-before" "$signal_upgrade/.alan-cli-manifest-dev" \
        || fail "SIG$signal_name changed old dev receipt"
    [[ ! -e "$signal_upgrade/.alan-cli-manifest" ]] || fail "signal left canonical receipt"
    cmp -s "$TEST_ROOT/signal-manifest-before" "$signal_upgrade/.alan-cli-manifest-stable" \
        || fail "SIG$signal_name changed the old manifest"
done

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

for bad_name in alan-dev alan-os-host-dev; do
    conflict="$TEST_ROOT/unowned-$bad_name"
    mkdir -p "$conflict"
    printf 'unowned\n' >"$conflict/$bad_name"
    expect_install_failure "$conflict" "$conflict/$bad_name"
    [[ ! -e "$conflict/alan" ]] || fail "dev conflict installed alan anyway"
done
modified_dev="$TEST_ROOT/modified-dev"
mkdir -p "$modified_dev"
printf 'stable before conflict\n' >"$modified_dev/alan"
printf 'dev before edit\n' >"$modified_dev/alan-dev"
printf 'alan|%s\n' "$(sha256 "$modified_dev/alan")" >"$modified_dev/.alan-cli-manifest-stable"
printf 'alan-dev|%s\n' "$(sha256 "$modified_dev/alan-dev")" >"$modified_dev/.alan-cli-manifest-dev"
printf 'local edit\n' >>"$modified_dev/alan-dev"
cp "$modified_dev/alan" "$TEST_ROOT/stable-before-dev-conflict"
expect_install_failure "$modified_dev" "$modified_dev/alan-dev"
cmp -s "$modified_dev/alan" "$TEST_ROOT/stable-before-dev-conflict" \
    || fail "dev conflict partially replaced stable CLI"
for malformed in traversal duplicate symlink; do
    bad="$TEST_ROOT/manifest-$malformed"
    mkdir -p "$bad"
    case "$malformed" in
        traversal) printf '../outside|%064d\n' 0 >"$bad/.alan-cli-manifest-dev" ;;
        duplicate) printf 'alan-dev|%064d\nalan-dev|%064d\n' 0 0 >"$bad/.alan-cli-manifest-dev" ;;
        symlink) ln -s "$modified_dev/.alan-cli-manifest-dev" "$bad/.alan-cli-manifest-dev" ;;
    esac
    expect_install_failure "$bad" "$bad/.alan-cli-manifest-dev"
    [[ ! -e "$bad/alan" ]] || fail "malformed manifest partially installed alan"
done

printf 'modified CLI\n' >>"$TEST_ROOT/bin/alan"
if "$SCRIPT_DIR/uninstall-cli.sh" >"$TEST_ROOT/uninstall-output" 2>&1; then
    fail "uninstaller removed modified owned CLI"
fi
[[ -f "$TEST_ROOT/bin/alan" && -f "$TEST_ROOT/bin/.alan-cli-manifest" ]] \
    || fail "uninstaller lost modified CLI or its receipt"
ALAN_CLI_INSTALL_DIR="$upgrade" "$SCRIPT_DIR/uninstall-cli.sh"
[[ ! -e "$upgrade/alan" && ! -e "$upgrade/.alan-cli-manifest" ]] \
    || fail "uninstall retained owned canonical artifacts"

ALAN_RELEASE_OUT_DIR="$TEST_ROOT/release" "$SCRIPT_DIR/assemble-cli-release.sh"
archive=("$TEST_ROOT"/release/*.tar.gz)
[[ -f "${archive[0]}" ]] || fail "release archive was not created"
listing="$(tar -tzf "${archive[0]}")"
[[ "$listing" == *"./alan"* && "$listing" != *"alan-dev"* && "$listing" == *"./manifest.json"* ]] \
    || fail "release archive is missing a CLI entry or manifest"
[[ "$listing" != *"alan-os-host"* ]] || fail "release archive contains a separate Host executable"
manifest_contents="$(tar -xOzf "${archive[0]}" ./manifest.json)"
[[ "$manifest_contents" == *'"binaries": ["alan"]'* ]] \
    || fail "release manifest does not list only CLI entry points"

printf 'Standalone CLI distribution checks passed.\n'
