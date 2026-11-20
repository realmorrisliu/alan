#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"
# shellcheck source=scripts/cargo-cli-output.sh
source "$SCRIPT_DIR/cargo-cli-output.sh"

usage() {
    cat <<'EOF'
Usage: scripts/assemble-cli-release.sh [options]

Options:
  --target <triple>          Target triple (env: ALAN_TARGET)
  --version <version>        Release version string (env: ALAN_RELEASE_VERSION)
  --out-dir <dir>            Output directory (env: ALAN_RELEASE_OUT_DIR)
  --binary <path>            Prebuilt alan executable to package (env: ALAN_RELEASE_BINARY)
  --sign-identity <id>       macOS signing identity for *-apple-darwin (env: ALAN_SIGN_IDENTITY)
  --keychain <path>          macOS keychain path (env: ALAN_SIGN_KEYCHAIN)
  --notary-key <path>        App Store Connect .p8 key file (env: ALAN_NOTARY_KEY)
  --notary-key-id <id>       App Store Connect API Key ID (env: ALAN_NOTARY_KEY_ID, ASC_API_KEY_ID)
  --notary-issuer <uuid>     App Store Connect Issuer ID (env: ALAN_NOTARY_ISSUER, ASC_API_ISSUER_ID)
  --notary-profile <name>    notarytool keychain profile (env: ALAN_NOTARY_PROFILE)
  --proof-out <path>         Signing proof output file (env: ALAN_SIGN_PROOF_OUT)
  --notary-json-out <path>   Notary JSON output file (env: ALAN_NOTARY_JSON_OUT)
  --skip-notarize            Sign macOS binary without notarizing (env: ALAN_SKIP_NOTARIZE=1)
  --require-notarize         Require notarization when signing macOS binary (env: ALAN_REQUIRE_NOTARIZE=1)
  -h, --help                 Show this help message
EOF
}

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    for arg in "$@"; do
        case "$arg" in
            -h|--help)
                usage
                exit 0
                ;;
        esac
    done
    TARGET_DIR="$(alan_cli_target_dir "$PROJECT_ROOT")"
    alan_cli_build_lease "$PROJECT_ROOT" "$TARGET_DIR" checkout "$SCRIPT_DIR/assemble-cli-release.sh" --alan-leased "$@"
fi

TARGET_DIR="$(alan_cli_target_dir "$PROJECT_ROOT")"
TARGET="${ALAN_TARGET:-}"
VERSION="${ALAN_RELEASE_VERSION:-}"
OUT_DIR="${ALAN_RELEASE_OUT_DIR:-$PROJECT_ROOT/target/distributions}"
RELEASE_BINARY="${ALAN_RELEASE_BINARY:-}"
SIGN_IDENTITY="${ALAN_SIGN_IDENTITY:-}"
SIGN_KEYCHAIN="${ALAN_SIGN_KEYCHAIN:-}"
NOTARY_KEY="${ALAN_NOTARY_KEY:-}"
NOTARY_KEY_ID="${ALAN_NOTARY_KEY_ID:-${ASC_API_KEY_ID:-}}"
NOTARY_ISSUER="${ALAN_NOTARY_ISSUER:-${ASC_API_ISSUER_ID:-}}"
NOTARY_PROFILE="${ALAN_NOTARY_PROFILE:-}"
PROOF_OUT="${ALAN_SIGN_PROOF_OUT:-}"
NOTARY_JSON_OUT="${ALAN_NOTARY_JSON_OUT:-}"
SKIP_NOTARIZE="${ALAN_SKIP_NOTARIZE:-0}"
REQUIRE_NOTARIZE="${ALAN_REQUIRE_NOTARIZE:-0}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --alan-leased)
            shift
            ;;
        --target)
            [[ $# -ge 2 ]] || fail "--target requires a value"
            TARGET="$2"
            shift 2
            ;;
        --version)
            [[ $# -ge 2 ]] || fail "--version requires a value"
            VERSION="$2"
            shift 2
            ;;
        --out-dir)
            [[ $# -ge 2 ]] || fail "--out-dir requires a value"
            OUT_DIR="$2"
            shift 2
            ;;
        --binary)
            [[ $# -ge 2 ]] || fail "--binary requires a value"
            RELEASE_BINARY="$2"
            shift 2
            ;;
        --sign-identity)
            [[ $# -ge 2 ]] || fail "--sign-identity requires a value"
            SIGN_IDENTITY="$2"
            shift 2
            ;;
        --keychain)
            [[ $# -ge 2 ]] || fail "--keychain requires a value"
            SIGN_KEYCHAIN="$2"
            shift 2
            ;;
        --notary-key)
            [[ $# -ge 2 ]] || fail "--notary-key requires a value"
            NOTARY_KEY="$2"
            shift 2
            ;;
        --notary-key-id)
            [[ $# -ge 2 ]] || fail "--notary-key-id requires a value"
            NOTARY_KEY_ID="$2"
            shift 2
            ;;
        --notary-issuer)
            [[ $# -ge 2 ]] || fail "--notary-issuer requires a value"
            NOTARY_ISSUER="$2"
            shift 2
            ;;
        --notary-profile)
            [[ $# -ge 2 ]] || fail "--notary-profile requires a value"
            NOTARY_PROFILE="$2"
            shift 2
            ;;
        --proof-out)
            [[ $# -ge 2 ]] || fail "--proof-out requires a value"
            PROOF_OUT="$2"
            shift 2
            ;;
        --notary-json-out)
            [[ $# -ge 2 ]] || fail "--notary-json-out requires a value"
            NOTARY_JSON_OUT="$2"
            shift 2
            ;;
        --skip-notarize)
            SKIP_NOTARIZE=1
            shift
            ;;
        --require-notarize)
            REQUIRE_NOTARIZE=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "unknown option: $1"
            ;;
    esac
done

[[ -n "$TARGET" ]] || TARGET="$(rustc -vV | awk '/^host: / { print $2 }')"
[[ -n "$VERSION" ]] || VERSION="$(git describe --tags --always --dirty)"
[[ -n "$TARGET" ]] || fail "could not resolve Rust target"
[[ -n "$VERSION" ]] || fail "could not resolve release version"
mkdir -p "$OUT_DIR"

if [[ -n "$RELEASE_BINARY" ]]; then
    CLI_SOURCE="$RELEASE_BINARY"
else
    CLI_SOURCE="$(alan_build_cli "$PROJECT_ROOT" "$TARGET_DIR" --release --target "$TARGET")"
fi
[[ -x "$CLI_SOURCE" ]] || fail "missing release binary: $CLI_SOURCE"

stage="$(mktemp -d "$TARGET_DIR/cli-stage.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
install -m 0755 "$CLI_SOURCE" "$stage/alan"

if [[ "$TARGET" == *-apple-darwin && -n "$SIGN_IDENTITY" ]]; then
    sign_script="$SCRIPT_DIR/sign-notarize-macos.sh"
    [[ -f "$sign_script" ]] || fail "missing macOS signing script: $sign_script"

    sign_args=(--binary "$stage/alan" --identity "$SIGN_IDENTITY")
    if [[ -n "$SIGN_KEYCHAIN" ]]; then
        sign_args+=(--keychain "$SIGN_KEYCHAIN")
    fi

    has_notary=0
    if [[ -n "$NOTARY_PROFILE" || -n "$NOTARY_KEY" || -n "$NOTARY_KEY_ID" || -n "$NOTARY_ISSUER" ]]; then
        has_notary=1
    fi

    if [[ "$SKIP_NOTARIZE" == "1" ]]; then
        [[ "$REQUIRE_NOTARIZE" != "1" ]] || fail "--skip-notarize cannot be combined with --require-notarize"
        sign_args+=(--skip-notarize)
    elif [[ "$has_notary" == "1" ]]; then
        if [[ -n "$NOTARY_PROFILE" ]]; then
            sign_args+=(--notary-profile "$NOTARY_PROFILE")
        else
            sign_args+=(--notary-key "$NOTARY_KEY" --notary-key-id "$NOTARY_KEY_ID" --notary-issuer "$NOTARY_ISSUER")
        fi
        sign_args+=(--notary-json-out "${NOTARY_JSON_OUT:-$OUT_DIR/notary-$TARGET.json}")
    else
        [[ "$REQUIRE_NOTARIZE" != "1" ]] || fail "notarization required for $TARGET, but no notary credentials were provided"
        sign_args+=(--skip-notarize)
    fi

    sign_args+=(--proof-out "${PROOF_OUT:-$OUT_DIR/signing-proof-$TARGET.txt}")
    bash "$sign_script" "${sign_args[@]}"
fi

manifest="$stage/manifest.json"
printf '{\n  "product": "alan-cli",\n  "version": "%s",\n  "target": "%s",\n  "binaries": ["alan"]\n}\n' \
    "$VERSION" "$TARGET" >"$manifest"

archive_name="alan-$VERSION-$TARGET.tar.gz"
archive="$OUT_DIR/$archive_name"
COPYFILE_DISABLE=1 tar -czf "$archive" -C "$stage" .
if command -v shasum >/dev/null 2>&1; then
    (cd "$OUT_DIR" && shasum -a 256 "$archive_name" >"$archive_name.sha256")
else
    (cd "$OUT_DIR" && sha256sum "$archive_name" >"$archive_name.sha256")
fi
printf 'Created standalone release: %s\n' "$archive"
