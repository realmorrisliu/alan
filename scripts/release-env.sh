#!/usr/bin/env bash

# Source this file from release/install scripts before reading signing or
# notarization variables. It loads only allowlisted KEY=value assignments from a
# private local env file; it does not execute arbitrary shell code from that file.

alan_release_env_allowed_key() {
    case "$1" in
        ALAN_DEVELOPER_ID_APPLICATION | \
        ALAN_SIGNING_IDENTITY | \
        ALAN_NOTARY_KEYCHAIN_PROFILE | \
        APPLE_ID | \
        APPLE_TEAM_ID | \
        APPLE_APP_SPECIFIC_PASSWORD | \
        ALAN_NOTARIZE | \
        ALAN_CREATE_RELEASE_ARCHIVE | \
        ALAN_INSTALL_CHANNEL | \
        ALAN_CARGO_TARGET_DIR | \
        ALAN_XCODE_DERIVED_DATA | \
        ALAN_RELEASE_ARTIFACT_DIR | \
        ALAN_APP_INSTALL_DIR | \
        ALAN_CLI_INSTALL_DIR)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

alan_release_env_trim() {
    local value="$1"
    value="${value#"${value%%[![:space:]]*}"}"
    value="${value%"${value##*[![:space:]]}"}"
    printf '%s' "$value"
}

alan_release_env_unquote() {
    local value="$1"
    if [[ "$value" == \"*\" && "$value" == *\" && "${#value}" -ge 2 ]]; then
        value="${value:1:${#value}-2}"
    elif [[ "$value" == \'*\' && "$value" == *\' && "${#value}" -ge 2 ]]; then
        value="${value:1:${#value}-2}"
    fi
    case "$value" in
        "~/"*)
            value="$HOME/${value#~/}"
            ;;
    esac
    printf '%s' "$value"
}

alan_release_env_load_file() {
    local file="$1"
    local line key raw value

    while IFS= read -r line || [[ -n "$line" ]]; do
        line="$(alan_release_env_trim "$line")"
        [[ -z "$line" || "$line" == \#* ]] && continue
        [[ "$line" == export[[:space:]]* ]] && line="$(alan_release_env_trim "${line#export}")"

        if [[ "$line" =~ ^([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*=[[:space:]]*(.*)$ ]]; then
            key="${BASH_REMATCH[1]}"
            raw="${BASH_REMATCH[2]}"
            alan_release_env_allowed_key "$key" || continue

            if [[ -z "${!key+x}" ]]; then
                value="$(alan_release_env_unquote "$(alan_release_env_trim "$raw")")"
                printf -v "$key" '%s' "$value"
                export "$key"
            fi
        fi
    done <"$file"

    export ALAN_RELEASE_ENV_FILE_RESOLVED="$file"
}

alan_release_env_load() {
    local script_dir
    local repo_root
    local candidate

    if [[ -n "${ALAN_RELEASE_ENV_FILE:-}" ]]; then
        if [[ ! -f "$ALAN_RELEASE_ENV_FILE" ]]; then
            printf 'error: ALAN_RELEASE_ENV_FILE does not exist: %s\n' "$ALAN_RELEASE_ENV_FILE" >&2
            return 1
        fi
        alan_release_env_load_file "$ALAN_RELEASE_ENV_FILE"
        return
    fi

    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    repo_root="$(cd "$script_dir/.." && pwd)"

    for candidate in \
        "$repo_root/.env.release.local" \
        "$repo_root/.env.release" \
        "$repo_root/.release.env.local" \
        "$repo_root/.release.env" \
        "$repo_root/.env.local" \
        "$repo_root/.env" \
        "$HOME/.alan/release.env"
    do
        if [[ -f "$candidate" ]]; then
            alan_release_env_load_file "$candidate"
            return
        fi
    done
}

alan_release_env_load
