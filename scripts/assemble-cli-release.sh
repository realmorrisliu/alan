#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET="${ALAN_TARGET:-$(rustc -vV | awk '/^host: / { print $2 }')}"
TARGET_DIR="${ALAN_STANDALONE_TARGET_DIR:-$PROJECT_ROOT/target/standalone-release}"
VERSION="${ALAN_RELEASE_VERSION:-$(git describe --tags --always --dirty)}"
OUT_DIR="${ALAN_RELEASE_OUT_DIR:-$PROJECT_ROOT/target/distributions}"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

[[ -n "$TARGET" ]] || fail "could not resolve Rust target"
mkdir -p "$OUT_DIR"

cargo build --locked --release -p alan --bin alan --target "$TARGET" --target-dir "$TARGET_DIR"
cargo build --locked --release -p alan-os-host \
    --bin alan-os-host --bin alan-os-host-dev --target "$TARGET" --target-dir "$TARGET_DIR"

BIN_DIR="$TARGET_DIR/$TARGET/release"
for binary in alan alan-os-host alan-os-host-dev; do
    [[ -x "$BIN_DIR/$binary" ]] || fail "missing release binary: $BIN_DIR/$binary"
done

stage="$(mktemp -d "$TARGET_DIR/cli-stage.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
for binary in alan alan-os-host alan-os-host-dev; do
    install -m 0755 "$BIN_DIR/$binary" "$stage/$binary"
done
ln -s alan "$stage/alan-dev"

manifest="$stage/manifest.json"
printf '{\n  "product": "alan-cli",\n  "version": "%s",\n  "target": "%s",\n  "binaries": ["alan", "alan-dev", "alan-os-host", "alan-os-host-dev"]\n}\n' \
    "$VERSION" "$TARGET" >"$manifest"

archive="$OUT_DIR/alan-$VERSION-$TARGET.tar.gz"
tar -czf "$archive" -C "$stage" .
if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$archive" >"$archive.sha256"
else
    sha256sum "$archive" >"$archive.sha256"
fi
printf 'Created standalone release: %s\n' "$archive"
