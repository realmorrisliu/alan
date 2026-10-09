#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"
# shellcheck source=scripts/cargo-cli-output.sh
source "$SCRIPT_DIR/cargo-cli-output.sh"
TARGET="${ALAN_TARGET:-$(rustc -vV | awk '/^host: / { print $2 }')}"
TARGET_DIR="$(alan_cli_target_dir "$PROJECT_ROOT")"
VERSION="${ALAN_RELEASE_VERSION:-$(git describe --tags --always --dirty)}"
OUT_DIR="${ALAN_RELEASE_OUT_DIR:-$PROJECT_ROOT/target/distributions}"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

[[ -n "$TARGET" ]] || fail "could not resolve Rust target"
mkdir -p "$OUT_DIR"

CLI_SOURCE="$(alan_build_cli "$PROJECT_ROOT" "$TARGET_DIR" --release --target "$TARGET")"
[[ -x "$CLI_SOURCE" ]] || fail "missing release binary: $CLI_SOURCE"

stage="$(mktemp -d "$TARGET_DIR/cli-stage.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
install -m 0755 "$CLI_SOURCE" "$stage/alan"
ln -s alan "$stage/alan-dev"

manifest="$stage/manifest.json"
printf '{\n  "product": "alan-cli",\n  "version": "%s",\n  "target": "%s",\n  "binaries": ["alan", "alan-dev"]\n}\n' \
    "$VERSION" "$TARGET" >"$manifest"

archive="$OUT_DIR/alan-$VERSION-$TARGET.tar.gz"
tar -czf "$archive" -C "$stage" .
if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$archive" >"$archive.sha256"
else
    sha256sum "$archive" >"$archive.sha256"
fi
printf 'Created standalone release: %s\n' "$archive"
