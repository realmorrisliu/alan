#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=scripts/release-env.sh
source "$SCRIPT_DIR/release-env.sh"
# shellcheck source=scripts/install-channel.sh
source "$SCRIPT_DIR/install-channel.sh"
# shellcheck source=scripts/app-bundle-paths.sh
source "$SCRIPT_DIR/app-bundle-paths.sh"

alan_install_channel_load "${ALAN_INSTALL_CHANNEL:-stable}"

DERIVED_DATA="${ALAN_XCODE_DERIVED_DATA:-$PROJECT_ROOT/target/xcode-derived}"
APP_SOURCE="$DERIVED_DATA/Build/Products/Release/$ALAN_APP_BUNDLE_NAME"
APP_INSTALL_DIR="${ALAN_APP_INSTALL_DIR:-$HOME/Applications}"
APP_TARGET="$APP_INSTALL_DIR/$ALAN_APP_BUNDLE_NAME"
LEGACY_APP_TARGET=""
if [[ -n "$ALAN_LEGACY_APP_BUNDLE_NAME" ]]; then
    LEGACY_APP_TARGET="$APP_INSTALL_DIR/$ALAN_LEGACY_APP_BUNDLE_NAME"
fi
CLI_INSTALL_DIR="${ALAN_CLI_INSTALL_DIR:-/usr/local/bin}"
APP_WAS_RUNNING=0

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

verify_shell_core_ffi_loadable() {
    local dylib="$1"

    if ! command -v python3 >/dev/null 2>&1; then
        fail "python3 is required to verify shell-core FFI dylib loading"
    fi

    python3 -c 'import ctypes, sys; ctypes.CDLL(sys.argv[1])' "$dylib" ||
        fail "installed shell-core FFI dylib is not loadable: $dylib"
}

is_app_running() {
    local app_pattern="${ALAN_APP_BUNDLE_NAME//./\\.}"

    pgrep -f "/$app_pattern/Contents/MacOS/" >/dev/null 2>&1 && return 0

    if [[ -n "$ALAN_LEGACY_APP_BUNDLE_NAME" ]]; then
        local legacy_pattern="${ALAN_LEGACY_APP_BUNDLE_NAME//./\\.}"
        pgrep -f "/$legacy_pattern/Contents/MacOS/" >/dev/null 2>&1 && return 0
    fi

    return 1
}

is_alan_owned_link() {
    local path="$1"
    local target

    if [[ ! -L "$path" ]]; then
        return 1
    fi

    target="$(readlink "$path")"
    case "$target" in
        *"/$ALAN_APP_BUNDLE_NAME/Contents/Resources/bin/"*)
            return 0
            ;;
        *)
            ;;
    esac

    if [[ -n "$ALAN_LEGACY_APP_BUNDLE_NAME" ]]; then
        case "$target" in
            *"/$ALAN_LEGACY_APP_BUNDLE_NAME/Contents/Resources/bin/"*)
                return 0
                ;;
        esac
    fi

    return 1
}

is_homebrew_prefix_target() {
    local target_dir="$1"
    local prefix
    local prefixes=()

    if command -v brew >/dev/null 2>&1; then
        prefix="$(brew --prefix 2>/dev/null || true)"
        if [[ -n "$prefix" ]]; then
            prefixes+=("$prefix")
        fi
    fi

    [[ -d /opt/homebrew ]] && prefixes+=("/opt/homebrew")
    [[ -d /usr/local/Homebrew ]] && prefixes+=("/usr/local")

    for prefix in "${prefixes[@]}"; do
        case "$target_dir/" in
            "$prefix/"*)
                return 0
                ;;
        esac
    done

    return 1
}

has_homebrew_managed_tool_links() {
    local prefix
    local tool
    local link
    local target
    local prefixes=()

    if command -v brew >/dev/null 2>&1; then
        prefix="$(brew --prefix 2>/dev/null || true)"
        if [[ -n "$prefix" ]]; then
            prefixes+=("$prefix")
        fi
    fi

    [[ -d /opt/homebrew ]] && prefixes+=("/opt/homebrew")
    [[ -d /usr/local/Homebrew ]] && prefixes+=("/usr/local")

    for prefix in "${prefixes[@]}"; do
        tool="$ALAN_CLI_NAME"
        link="$prefix/bin/$tool"
        if [[ ! -L "$link" ]]; then
            continue
        fi
        target="$(readlink "$link")"
        case "$target" in
            *"/$ALAN_APP_BUNDLE_NAME/Contents/Resources/bin/$tool")
                printf '%s\n' "$link"
                return 0
                ;;
        esac
        if [[ -n "$ALAN_LEGACY_APP_BUNDLE_NAME" ]]; then
            case "$target" in
                *"/$ALAN_LEGACY_APP_BUNDLE_NAME/Contents/Resources/bin/$tool")
                    printf '%s\n' "$link"
                    return 0
                    ;;
            esac
        fi
    done

    return 1
}

link_tool() {
    local tool="$1"
    local source="$APP_TARGET/Contents/Resources/bin/$tool"
    local target="$CLI_INSTALL_DIR/$tool"

    if [[ ! -x "$source" ]]; then
        printf 'error: embedded tool is missing or not executable: %s\n' "$source" >&2
        exit 1
    fi

    if [[ -e "$target" || -L "$target" ]]; then
        if ! is_alan_owned_link "$target"; then
            printf 'error: refusing to overwrite non-alan command at %s\n' "$target" >&2
            printf '       set ALAN_CLI_INSTALL_DIR to a different PATH directory or remove the conflicting file manually\n' >&2
            exit 1
        fi
        if [[ "$(readlink "$target")" == "$source" ]]; then
            return
        fi
        rm -f "$target"
    fi

    ln -s "$source" "$target"
}

if is_app_running; then
    APP_WAS_RUNNING=1
fi

"$SCRIPT_DIR/assemble-release-app.sh"

if [[ ! -d "$APP_SOURCE" ]]; then
    printf 'error: release assembly did not produce %s\n' "$APP_SOURCE" >&2
    exit 1
fi

printf 'Installing %s to %s...\n' "$ALAN_APP_BUNDLE_NAME" "$APP_TARGET"
mkdir -p "$APP_INSTALL_DIR"
rm -rf "$APP_TARGET"
cp -R "$APP_SOURCE" "$APP_TARGET"
if [[ -f "$APP_TARGET/Contents/Frameworks/libalan_shell_core_ffi.dylib" ]]; then
    codesign --verify --strict --verbose=2 \
        "$APP_TARGET/Contents/Frameworks/libalan_shell_core_ffi.dylib" >/dev/null
    verify_shell_core_ffi_loadable "$APP_TARGET/Contents/Frameworks/libalan_shell_core_ffi.dylib"
fi
codesign --verify --strict --verbose=2 "$APP_TARGET" >/dev/null
if [[ -n "$LEGACY_APP_TARGET" ]] && alan_is_distinct_existing_path "$LEGACY_APP_TARGET" "$APP_TARGET"; then
    printf 'Removing legacy lowercase app bundle at %s...\n' "$LEGACY_APP_TARGET"
    rm -rf "$LEGACY_APP_TARGET"
fi

printf 'Linking CLI into %s...\n' "$CLI_INSTALL_DIR"
if is_homebrew_prefix_target "$CLI_INSTALL_DIR"; then
    printf 'error: %s is inside a Homebrew prefix.\n' "$CLI_INSTALL_DIR" >&2
    printf '       use the Homebrew cask for Homebrew-managed links, or set ALAN_CLI_INSTALL_DIR to a non-Homebrew PATH directory.\n' >&2
    exit 1
fi
if homebrew_link="$(has_homebrew_managed_tool_links)"; then
    printf 'error: Homebrew already manages %s command-line links at %s\n' "$ALAN_DISPLAY_NAME" "$homebrew_link" >&2
    printf '       use the Homebrew cask to update stable Alan, or remove the Homebrew links before creating direct-install symlinks.\n' >&2
    exit 1
fi
mkdir -p "$CLI_INSTALL_DIR"
link_tool "$ALAN_CLI_NAME"

printf '\n%s installed:\n' "$ALAN_DISPLAY_NAME"
printf '  app: %s\n' "$APP_TARGET"
printf '  cli: %s/%s -> %s/Contents/Resources/bin/%s\n' "$CLI_INSTALL_DIR" "$ALAN_CLI_NAME" "$APP_TARGET" "$ALAN_CLI_NAME"

if [[ "$APP_WAS_RUNNING" -eq 1 ]]; then
    printf '\n%s was running during install. It was not stopped or relaunched; restart it manually to use the newly installed app.\n' "$ALAN_APP_BUNDLE_NAME"
fi

printf '\nEnsure %s is on PATH if you want shell access.\n' "$CLI_INSTALL_DIR"
