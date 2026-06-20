#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

DEFAULT_GHOSTTY_REPO="$REPO_ROOT/third_party/ghostty"
GHOSTTY_REPO="${ALAN_GHOSTTY_REPO:-$DEFAULT_GHOSTTY_REPO}"
CACHE_ROOT="${ALAN_GHOSTTY_CACHE_DIR:-$HOME/.cache/alan-shell/ghostty}"
METADATA_FILE_NAME="source-metadata.env"
GHOSTTY_XCFRAMEWORK_TARGET="${ALAN_GHOSTTY_XCFRAMEWORK_TARGET:-native}"
GHOSTTY_SIMD="${ALAN_GHOSTTY_SIMD:-false}"

OUTPUT_XCFRAMEWORK="$PROJECT_DIR/GhosttyKit.xcframework"
OUTPUT_RESOURCES="$PROJECT_DIR/ghostty-resources"
OUTPUT_TERMINFO="$PROJECT_DIR/ghostty-terminfo"

CACHE_KEY=""
CACHE_DIR=""
CACHE_XCFRAMEWORK=""
CACHE_RESOURCES=""
CACHE_TERMINFO=""
CACHE_METADATA=""

has_artifact_override() {
    [ -n "${ALAN_GHOSTTYKIT_PATH:-}" ] \
        || [ -n "${ALAN_GHOSTTY_RESOURCES_DIR:-}" ] \
        || [ -n "${ALAN_GHOSTTY_TERMINFO_DIR:-}" ]
}

source_kind() {
    if [ -n "${ALAN_GHOSTTY_REPO:-}" ]; then
        printf 'override-repo\n'
    elif has_artifact_override; then
        printf 'artifact-overrides\n'
    else
        printf 'submodule\n'
    fi
}

source_revision() {
    if [ -d "$GHOSTTY_REPO" ] \
        && git -C "$GHOSTTY_REPO" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        git -C "$GHOSTTY_REPO" rev-parse HEAD
        return 0
    fi

    if [ -n "${ALAN_GHOSTTY_CACHE_KEY:-}" ]; then
        printf 'cache-key:%s\n' "$ALAN_GHOSTTY_CACHE_KEY"
        return 0
    fi

    printf 'unknown\n'
}

print_source_summary() {
    local kind
    local revision

    kind="$(source_kind)"
    revision="$(source_revision)"

    printf '==> Ghostty source: %s at %s\n' "$kind" "$GHOSTTY_REPO"
    printf '==> Ghostty source revision: %s\n' "$revision"
    printf '==> Ghostty xcframework target: %s\n' "$GHOSTTY_XCFRAMEWORK_TARGET"
    printf '==> Ghostty SIMD: %s\n' "$GHOSTTY_SIMD"

    if [ -n "${ALAN_GHOSTTY_REPO:-}" ]; then
        printf '==> Using ALAN_GHOSTTY_REPO override: %s\n' "$ALAN_GHOSTTY_REPO"
    fi
    if [ -n "${ALAN_GHOSTTY_XCFRAMEWORK_TARGET:-}" ]; then
        printf '==> Using ALAN_GHOSTTY_XCFRAMEWORK_TARGET override: %s\n' "$ALAN_GHOSTTY_XCFRAMEWORK_TARGET"
    fi
    if [ -n "${ALAN_GHOSTTY_SIMD:-}" ]; then
        printf '==> Using ALAN_GHOSTTY_SIMD override: %s\n' "$ALAN_GHOSTTY_SIMD"
    fi
    if [ -n "${ALAN_GHOSTTYKIT_PATH:-}" ]; then
        printf '==> Using ALAN_GHOSTTYKIT_PATH override: %s\n' "$ALAN_GHOSTTYKIT_PATH"
    fi
    if [ -n "${ALAN_GHOSTTY_RESOURCES_DIR:-}" ]; then
        printf '==> Using ALAN_GHOSTTY_RESOURCES_DIR override: %s\n' "$ALAN_GHOSTTY_RESOURCES_DIR"
    fi
    if [ -n "${ALAN_GHOSTTY_TERMINFO_DIR:-}" ]; then
        printf '==> Using ALAN_GHOSTTY_TERMINFO_DIR override: %s\n' "$ALAN_GHOSTTY_TERMINFO_DIR"
    fi
    if [ -n "${ALAN_GHOSTTY_CACHE_KEY:-}" ]; then
        printf '==> Using ALAN_GHOSTTY_CACHE_KEY override: %s\n' "$ALAN_GHOSTTY_CACHE_KEY"
    fi
}

print_submodule_hint() {
    printf 'hint: initialize the pinned Ghostty fork with:\n' >&2
    printf '      git submodule update --init --recursive third_party/ghostty\n' >&2
}

ensure_default_ghostty_repo() {
    local mode="${1:-prepare}"

    if [ -n "${ALAN_GHOSTTY_REPO:-}" ]; then
        return 0
    fi

    if has_artifact_override; then
        return 0
    fi

    if [ -d "$DEFAULT_GHOSTTY_REPO" ] \
        && git -C "$DEFAULT_GHOSTTY_REPO" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        return 0
    fi

    if [ "$mode" = "check" ]; then
        printf 'error: pinned Ghostty fork submodule is missing or uninitialized at %s\n' "$DEFAULT_GHOSTTY_REPO" >&2
        print_submodule_hint
        exit 1
    fi

    printf '==> Initializing pinned Ghostty fork submodule at %s\n' "$DEFAULT_GHOSTTY_REPO"
    git -C "$REPO_ROOT" submodule update --init --recursive third_party/ghostty

    if [ ! -d "$DEFAULT_GHOSTTY_REPO" ] \
        || ! git -C "$DEFAULT_GHOSTTY_REPO" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        printf 'error: pinned Ghostty fork submodule is missing or uninitialized at %s\n' "$DEFAULT_GHOSTTY_REPO" >&2
        print_submodule_hint
        exit 1
    fi
}

validate_xcframework_target() {
    case "$GHOSTTY_XCFRAMEWORK_TARGET" in
        native | universal)
            ;;
        *)
            printf 'error: unsupported Ghostty xcframework target: %s\n' "$GHOSTTY_XCFRAMEWORK_TARGET" >&2
            printf 'hint: use ALAN_GHOSTTY_XCFRAMEWORK_TARGET=native or universal.\n' >&2
            exit 1
            ;;
    esac
}

validate_simd() {
    case "$GHOSTTY_SIMD" in
        true | false)
            ;;
        *)
            printf 'error: unsupported Ghostty SIMD value: %s\n' "$GHOSTTY_SIMD" >&2
            printf 'hint: use ALAN_GHOSTTY_SIMD=true or false.\n' >&2
            exit 1
            ;;
    esac
}

sync_tree() {
    local source="$1"
    local destination="$2"

    if [ ! -d "$source" ]; then
        printf 'error: source directory not found at %s\n' "$source" >&2
        exit 1
    fi

    rm -rf "$destination"
    mkdir -p "$destination"
    rsync -a --delete "$source"/ "$destination"/
}

sync_path() {
    local source="$1"
    local destination="$2"

    if [ -d "$source" ]; then
        sync_tree "$source" "$destination"
        return 0
    fi

    printf 'error: source directory not found at %s\n' "$source" >&2
    exit 1
}

link_output() {
    local source="$1"
    local destination="$2"

    rm -rf "$destination"
    ln -sfn "$source" "$destination"
}

normalize_ghosttykit_modulemaps() {
    local framework="$1"
    local modulemap
    local tmp

    while IFS= read -r -d '' modulemap; do
        if grep -q 'umbrella header "ghostty\.h"' "$modulemap"; then
            tmp="$modulemap.tmp.$$"
            sed 's/umbrella header "ghostty\.h"/header "ghostty.h"/' "$modulemap" > "$tmp"
            mv "$tmp" "$modulemap"
        fi
    done < <(find -L "$framework" -name module.modulemap -type f -print0)
}

required_ghostty_zig_version() {
    awk -F\" '/minimum_zig_version/ {
        print $2
        exit
    }' "$GHOSTTY_REPO/build.zig.zon"
}

zig_version() {
    "$1" version 2>/dev/null | head -n 1
}

ghostty_zig_candidates() {
    if [ -n "${ALAN_GHOSTTY_ZIG:-}" ]; then
        printf '%s\n' "$ALAN_GHOSTTY_ZIG"
    fi

    printf '%s\n' \
        "/opt/homebrew/opt/zig@0.15/bin/zig" \
        "/usr/local/opt/zig@0.15/bin/zig"

    if command -v brew >/dev/null 2>&1; then
        local brew_prefix
        if brew_prefix="$(brew --prefix zig@0.15 2>/dev/null)"; then
            printf '%s/bin/zig\n' "$brew_prefix"
        fi
    fi

    if command -v zig >/dev/null 2>&1; then
        command -v zig
    fi
}

print_zig_install_hint() {
    printf 'hint: install the matching Ghostty Zig toolchain with `brew install zig@0.15`,\n' >&2
    printf '      or set ALAN_GHOSTTY_ZIG=/absolute/path/to/zig.\n' >&2
}

has_proxy_environment() {
    [ -n "${http_proxy:-}${https_proxy:-}${all_proxy:-}${HTTP_PROXY:-}${HTTPS_PROXY:-}${ALL_PROXY:-}" ]
}

find_xcode_tool() {
    local sdk="$1"
    local tool="$2"

    xcodebuild -find-executable "$tool" -sdk "$sdk" 2>/dev/null \
        | awk 'NF {
            print
            exit
        }'
}

require_metal_tool() {
    local sdk="$1"
    local tool="$2"
    local scope="$3"
    local resolved

    resolved="$(find_xcode_tool "$sdk" "$tool")"
    if [ -x "$resolved" ] && "$resolved" -v >/dev/null 2>&1; then
        return 0
    fi

    printf 'error: Metal Toolchain is required to build GhosttyKit.xcframework for %s\n' "$scope" >&2
    printf 'hint: run `xcodebuild -downloadComponent MetalToolchain`\n' >&2
    exit 1
}

require_metal_toolchain() {
    require_metal_tool macosx metal macOS
    require_metal_tool macosx metallib macOS

    if [ "$GHOSTTY_XCFRAMEWORK_TARGET" = "universal" ]; then
        require_metal_tool iphoneos metal iOS
        require_metal_tool iphoneos metallib iOS
    fi
}

resolve_ghostty_zig() {
    local required
    local candidate
    local version
    local seen=""

    required="$(required_ghostty_zig_version)"
    if [ -z "$required" ]; then
        printf 'error: unable to read minimum_zig_version from %s/build.zig.zon\n' "$GHOSTTY_REPO" >&2
        exit 1
    fi

    while IFS= read -r candidate; do
        if [ -z "$candidate" ]; then
            continue
        fi
        if [ ! -x "$candidate" ]; then
            continue
        fi

        version="$(zig_version "$candidate")"
        seen="${seen}${candidate} (${version:-unknown})"$'\n'
        if [ "$version" = "$required" ]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done < <(ghostty_zig_candidates)

    printf 'error: Ghostty source at %s expects Zig %s, but no matching zig was found.\n' "$GHOSTTY_REPO" "$required" >&2
    if [ -n "$seen" ]; then
        printf 'checked Zig candidates:\n%b' "$seen" >&2
    fi
    print_zig_install_hint
    exit 1
}

run_ghostty_zig_build() {
    local zig_bin="$1"
    local zig_global_cache="$2"
    local zig_local_cache="$3"
    local xcframework_target="$4"
    local simd="$5"

    if [ "${ALAN_GHOSTTY_ZIG_KEEP_PROXY:-}" = "1" ]; then
        ZIG_GLOBAL_CACHE_DIR="$zig_global_cache" \
            ZIG_LOCAL_CACHE_DIR="$zig_local_cache" \
            "$zig_bin" build -Demit-xcframework=true -Dxcframework-target="$xcframework_target" -Dsimd="$simd" -Doptimize=ReleaseFast
        return 0
    fi

    http_proxy= \
        https_proxy= \
        all_proxy= \
        HTTP_PROXY= \
        HTTPS_PROXY= \
        ALL_PROXY= \
        ZIG_GLOBAL_CACHE_DIR="$zig_global_cache" \
        ZIG_LOCAL_CACHE_DIR="$zig_local_cache" \
        "$zig_bin" build -Demit-xcframework=true -Dxcframework-target="$xcframework_target" -Dsimd="$simd" -Doptimize=ReleaseFast
}

find_existing_framework() {
    local path
    for path in \
        "${ALAN_GHOSTTYKIT_PATH:-}" \
        "$GHOSTTY_REPO/macos/GhosttyKit.xcframework"; do
        if [ -d "$path" ]; then
            printf '%s\n' "$path"
            return 0
        fi
    done
    return 1
}

resolve_cache_key() {
    if [ -n "${ALAN_GHOSTTY_CACHE_KEY:-}" ]; then
        printf '%s\n' "$ALAN_GHOSTTY_CACHE_KEY"
        return 0
    fi

    if has_artifact_override; then
        printf '%s\n' "$GHOSTTY_REPO|$(source_revision)|$GHOSTTY_XCFRAMEWORK_TARGET|$GHOSTTY_SIMD|${ALAN_GHOSTTYKIT_PATH:-}|${ALAN_GHOSTTY_RESOURCES_DIR:-}|${ALAN_GHOSTTY_TERMINFO_DIR:-}" \
            | shasum -a 256 \
            | awk '{print "override-" substr($1, 1, 16)}'
        return 0
    fi

    if [ -d "$GHOSTTY_REPO" ] && git -C "$GHOSTTY_REPO" rev-parse --is-inside-work-tree >/dev/null 2>&1;
    then
        local simd_suffix
        if [ "$GHOSTTY_SIMD" = "true" ]; then
            simd_suffix="simd"
        else
            simd_suffix="nosimd"
        fi
        printf '%s-%s-%s\n' "$(git -C "$GHOSTTY_REPO" rev-parse HEAD)" "$GHOSTTY_XCFRAMEWORK_TARGET" "$simd_suffix"
        return 0
    fi

    printf '%s\n' "$GHOSTTY_XCFRAMEWORK_TARGET|$GHOSTTY_SIMD|${ALAN_GHOSTTYKIT_PATH:-}|${ALAN_GHOSTTY_RESOURCES_DIR:-}|${ALAN_GHOSTTY_TERMINFO_DIR:-}" \
        | shasum -a 256 \
        | awk '{print "manual-" substr($1, 1, 16)}'
}

prepare_cache_paths() {
    CACHE_KEY="$(resolve_cache_key)"
    CACHE_DIR="$CACHE_ROOT/$CACHE_KEY"
    CACHE_XCFRAMEWORK="$CACHE_DIR/GhosttyKit.xcframework"
    CACHE_RESOURCES="$CACHE_DIR/ghostty-resources"
    CACHE_TERMINFO="$CACHE_DIR/ghostty-terminfo"
    CACHE_METADATA="$CACHE_DIR/$METADATA_FILE_NAME"

    mkdir -p "$CACHE_DIR"
}

write_cache_metadata() {
    {
        printf 'schema=alan-ghostty-artifacts-v1\n'
        printf 'source_kind=%s\n' "$(source_kind)"
        printf 'source_path=%s\n' "$GHOSTTY_REPO"
        printf 'source_revision=%s\n' "$(source_revision)"
        printf 'xcframework_target=%s\n' "$GHOSTTY_XCFRAMEWORK_TARGET"
        printf 'simd=%s\n' "$GHOSTTY_SIMD"
        printf 'cache_key=%s\n' "$CACHE_KEY"
        printf 'xcframework=%s\n' "$CACHE_XCFRAMEWORK"
        printf 'resources=%s\n' "$CACHE_RESOURCES"
        printf 'terminfo=%s\n' "$CACHE_TERMINFO"
    } > "$CACHE_METADATA"
}

metadata_value() {
    local file="$1"
    local key="$2"

    awk -F= -v key="$key" '$1 == key {
        print substr($0, index($0, "=") + 1)
        exit
    }' "$file"
}

metadata_for_output() {
    local resolved
    resolved="$(readlink "$OUTPUT_XCFRAMEWORK" || true)"
    if [ -z "$resolved" ]; then
        return 1
    fi

    printf '%s/%s\n' "$(dirname "$resolved")" "$METADATA_FILE_NAME"
}

ensure_ghosttykit() {
    local resolved=""
    if resolved="$(find_existing_framework)"; then
        printf '==> Reusing GhosttyKit source at %s\n' "$resolved"
    else
        local zig_bin
        local zig_global_cache
        local zig_local_cache

        require_metal_toolchain

        if [ ! -d "$GHOSTTY_REPO" ]; then
            printf 'error: Ghostty repo not found at %s\n' "$GHOSTTY_REPO" >&2
            exit 1
        fi

        zig_bin="$(resolve_ghostty_zig)"
        printf '==> Using Zig %s at %s\n' "$(zig_version "$zig_bin")" "$zig_bin"

        zig_global_cache="$CACHE_DIR/zig-global-cache"
        zig_local_cache="$CACHE_DIR/zig-local-cache"
        mkdir -p "$zig_global_cache" "$zig_local_cache"

        if [ "${ALAN_GHOSTTY_ZIG_KEEP_PROXY:-}" = "1" ]; then
            printf '==> Keeping proxy environment for Zig because ALAN_GHOSTTY_ZIG_KEEP_PROXY=1\n'
        elif has_proxy_environment; then
            printf '==> Running Zig with proxy variables cleared for Ghostty dependency downloads\n'
        fi

        printf '==> Building GhosttyKit.xcframework from %s\n' "$GHOSTTY_REPO"
        (
            cd "$GHOSTTY_REPO"
            run_ghostty_zig_build "$zig_bin" "$zig_global_cache" "$zig_local_cache" "$GHOSTTY_XCFRAMEWORK_TARGET" "$GHOSTTY_SIMD"
        )

        resolved="$GHOSTTY_REPO/macos/GhosttyKit.xcframework"
        if [ ! -d "$resolved" ]; then
            printf 'error: expected GhosttyKit.xcframework at %s\n' "$resolved" >&2
            exit 1
        fi
    fi

    printf '==> Syncing %s -> %s\n' "$resolved" "$CACHE_XCFRAMEWORK"
    sync_path "$resolved" "$CACHE_XCFRAMEWORK"
    normalize_ghosttykit_modulemaps "$CACHE_XCFRAMEWORK"

    printf '==> Linking %s -> %s\n' "$OUTPUT_XCFRAMEWORK" "$CACHE_XCFRAMEWORK"
    link_output "$CACHE_XCFRAMEWORK" "$OUTPUT_XCFRAMEWORK"
}

ensure_resources() {
    local candidate
    for candidate in \
        "${ALAN_GHOSTTY_RESOURCES_DIR:-}" \
        "$GHOSTTY_REPO/zig-out/share/ghostty"; do
        if [ -d "$candidate" ]; then
            printf '==> Syncing %s -> %s\n' "$candidate" "$CACHE_RESOURCES"
            sync_path "$candidate" "$CACHE_RESOURCES"
            printf '==> Linking %s -> %s\n' "$OUTPUT_RESOURCES" "$CACHE_RESOURCES"
            link_output "$CACHE_RESOURCES" "$OUTPUT_RESOURCES"
            return 0
        fi
    done

    printf 'warning: no Ghostty resources directory found; continuing without %s\n' "$OUTPUT_RESOURCES" >&2
}

ensure_terminfo() {
    local candidate
    for candidate in \
        "${ALAN_GHOSTTY_TERMINFO_DIR:-}" \
        "$GHOSTTY_REPO/zig-out/share/terminfo"; do
        if [ -d "$candidate" ]; then
            printf '==> Syncing %s -> %s\n' "$candidate" "$CACHE_TERMINFO"
            sync_path "$candidate" "$CACHE_TERMINFO"
            printf '==> Linking %s -> %s\n' "$OUTPUT_TERMINFO" "$CACHE_TERMINFO"
            link_output "$CACHE_TERMINFO" "$OUTPUT_TERMINFO"
            return 0
        fi
    done

    printf 'warning: no Ghostty terminfo directory found; continuing without %s\n' "$OUTPUT_TERMINFO" >&2
}

check_artifacts() {
    local missing=0
    local metadata
    local expected_cache_key
    local expected_revision
    local expected_target
    local expected_simd
    local actual_cache_key
    local actual_revision
    local actual_target
    local actual_simd
    local path

    ensure_default_ghostty_repo check
    validate_xcframework_target
    validate_simd
    print_source_summary

    for path in "$OUTPUT_XCFRAMEWORK" "$OUTPUT_RESOURCES" "$OUTPUT_TERMINFO"; do
        if [ -d "$path" ]; then
            printf 'ok: %s\n' "$path"
        else
            printf 'missing: %s\n' "$path" >&2
            missing=1
        fi
    done

    if [ "$missing" -ne 0 ]; then
        printf '\nRun %s to prepare the local Ghostty artifacts.\n' "$0" >&2
        exit 1
    fi

    if ! metadata="$(metadata_for_output)" || [ ! -f "$metadata" ]; then
        printf 'stale: Ghostty artifact metadata is missing.\n' >&2
        printf 'hint: run %s to refresh artifacts from the current Ghostty source.\n' "$0" >&2
        exit 1
    fi

    expected_cache_key="$(resolve_cache_key)"
    expected_revision="$(source_revision)"
    expected_target="$GHOSTTY_XCFRAMEWORK_TARGET"
    expected_simd="$GHOSTTY_SIMD"
    actual_cache_key="$(metadata_value "$metadata" cache_key)"
    actual_revision="$(metadata_value "$metadata" source_revision)"
    actual_target="$(metadata_value "$metadata" xcframework_target)"
    actual_simd="$(metadata_value "$metadata" simd)"

    if [ "$actual_cache_key" != "$expected_cache_key" ]; then
        printf 'stale: Ghostty artifacts were built from cache key %s, expected %s.\n' "$actual_cache_key" "$expected_cache_key" >&2
        printf 'hint: run %s to refresh artifacts from the current Ghostty source.\n' "$0" >&2
        exit 1
    fi

    if [ -z "${ALAN_GHOSTTY_CACHE_KEY:-}" ] && [ "$actual_revision" != "$expected_revision" ]; then
        printf 'stale: Ghostty artifacts were built from revision %s, expected %s.\n' "$actual_revision" "$expected_revision" >&2
        printf 'hint: run %s to refresh artifacts from the current Ghostty source.\n' "$0" >&2
        exit 1
    fi

    if [ "$actual_target" != "$expected_target" ]; then
        printf 'stale: Ghostty artifacts were built for xcframework target %s, expected %s.\n' "$actual_target" "$expected_target" >&2
        printf 'hint: run %s to refresh artifacts from the current Ghostty target.\n' "$0" >&2
        exit 1
    fi

    if [ "$actual_simd" != "$expected_simd" ]; then
        printf 'stale: Ghostty artifacts were built with simd=%s, expected %s.\n' "$actual_simd" "$expected_simd" >&2
        printf 'hint: run %s to refresh artifacts from the current Ghostty SIMD setting.\n' "$0" >&2
        exit 1
    fi

    printf 'ok: Ghostty artifact metadata %s\n' "$metadata"
}

case "${1:-}" in
    --check)
        check_artifacts
        exit 0
        ;;
    "")
        ;;
    *)
        printf 'usage: %s [--check]\n' "$0" >&2
        exit 2
        ;;
esac

ensure_default_ghostty_repo prepare
validate_xcframework_target
validate_simd
print_source_summary
prepare_cache_paths
ensure_ghosttykit
ensure_resources
ensure_terminfo
write_cache_metadata

printf '\nReady.\n'
printf 'Ghostty artifacts are cached outside the repo at %s.\n' "$CACHE_DIR"
printf 'Ghostty artifact metadata is recorded at %s.\n' "$CACHE_METADATA"
printf 'Ignored developer links under clients/apple/ now point at that cache.\n'
