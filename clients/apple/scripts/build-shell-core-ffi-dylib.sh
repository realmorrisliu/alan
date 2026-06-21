#!/usr/bin/env bash
set -euo pipefail

if [[ "${PLATFORM_NAME:-}" != "macosx" ]]; then
    exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

DYLIB_NAME="libalan_shell_core_ffi.dylib"
CARGO_TARGET_ROOT="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
PROFILE_DIR="debug"
CARGO_BUILD_ARGS=()

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

resolve_xcode_arch() {
    local arch="${CURRENT_ARCH:-}"
    local archs
    local candidate

    if [[ "$arch" == "undefined_arch" ]]; then
        arch=""
    fi
    if [[ -z "$arch" || "$arch" == "undefined_arch" ]]; then
        archs="${ARCHS:-}"
        if [[ -n "$archs" ]]; then
            for candidate in $archs; do
                if [[ -n "$arch" && "$arch" != "$candidate" ]]; then
                    fail "Build Shell Core FFI expects one target architecture; got ARCHS='$archs'"
                fi
                arch="$candidate"
            done
        fi
    fi

    if [[ -z "$arch" || "$arch" == "undefined_arch" ]]; then
        arch="${NATIVE_ARCH_ACTUAL:-}"
    fi
    if [[ "$arch" == "undefined_arch" ]]; then
        arch=""
    fi
    if [[ -z "$arch" || "$arch" == "undefined_arch" ]]; then
        arch="$(uname -m)"
    fi

    printf '%s' "$arch"
}

resolve_cargo_target() {
    local arch

    if [[ -n "${ALAN_SHELL_CORE_FFI_CARGO_TARGET:-}" ]]; then
        printf '%s' "$ALAN_SHELL_CORE_FFI_CARGO_TARGET"
        return
    fi

    arch="$(resolve_xcode_arch)" || exit 1
    case "$arch" in
        arm64)
            printf 'aarch64-apple-darwin'
            ;;
        x86_64)
            printf 'x86_64-apple-darwin'
            ;;
        *)
            fail "unsupported shell-core FFI target architecture: $arch"
            ;;
    esac
}

codesign_dylib_if_needed() {
    local dylib="$1"
    local identity="${EXPANDED_CODE_SIGN_IDENTITY:-${CODE_SIGN_IDENTITY:-}}"

    if [[ "${CODE_SIGNING_ALLOWED:-NO}" != "YES" ]]; then
        return
    fi
    if [[ -z "$identity" ]]; then
        if [[ "${CODE_SIGNING_REQUIRED:-NO}" == "YES" ]]; then
            fail "code signing is required for $dylib but no signing identity was provided"
        fi
        return
    fi

    if [[ "${ENABLE_HARDENED_RUNTIME:-NO}" == "YES" ]]; then
        /usr/bin/codesign --force --options runtime --sign "$identity" "$dylib"
    else
        /usr/bin/codesign --force --sign "$identity" "$dylib"
    fi
}

verify_dylib_loadable() {
    local dylib="$1"

    if ! command -v python3 >/dev/null 2>&1; then
        fail "python3 is required to verify shell-core FFI dylib loading"
    fi

    python3 -c 'import ctypes, sys; ctypes.CDLL(sys.argv[1])' "$dylib" ||
        fail "shell-core FFI dylib is not loadable: $dylib"
}

rewrite_dylib_install_name_if_supported() {
    local dylib="$1"

    if [[ "${ENABLE_USER_SCRIPT_SANDBOXING:-NO}" == "YES" ]]; then
        printf 'Skipping install_name_tool for %s because Xcode user script sandboxing is enabled.\n' \
            "$dylib"
        return
    fi

    /usr/bin/install_name_tool -id "@rpath/$DYLIB_NAME" "$dylib"
}

if [[ "${CONFIGURATION:-Debug}" == "Release" ]]; then
    PROFILE_DIR="release"
fi

CARGO_BUILD_TARGET="$(resolve_cargo_target)"
CARGO_BUILD_ARGS=(-p alan-shell-core-ffi --target "$CARGO_BUILD_TARGET")
if [[ "$PROFILE_DIR" == "release" ]]; then
    CARGO_BUILD_ARGS+=(--release)
fi

if [[ -n "${ALAN_SHELL_CORE_FFI_LIBRARY:-}" ]]; then
    DYLIB="$ALAN_SHELL_CORE_FFI_LIBRARY"
else
    cd "$REPO_ROOT"
    cargo build "${CARGO_BUILD_ARGS[@]}"
    DYLIB="$CARGO_TARGET_ROOT/$CARGO_BUILD_TARGET/$PROFILE_DIR/$DYLIB_NAME"
fi
[[ -f "$DYLIB" ]] || fail "shell-core FFI dylib was not found at $DYLIB"
verify_dylib_loadable "$DYLIB"

BUNDLED_DYLIB="$TARGET_BUILD_DIR/$FRAMEWORKS_FOLDER_PATH/$DYLIB_NAME"
mkdir -p "$(dirname "$BUNDLED_DYLIB")"
install -m 755 "$DYLIB" "$BUNDLED_DYLIB"
rewrite_dylib_install_name_if_supported "$BUNDLED_DYLIB"
codesign_dylib_if_needed "$BUNDLED_DYLIB"
verify_dylib_loadable "$BUNDLED_DYLIB"
