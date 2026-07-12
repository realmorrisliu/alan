#!/usr/bin/env bash

alan_install_channel_load() {
    local channel="${1:-stable}"

    case "$channel" in
        stable)
            ALAN_CHANNEL_ID="stable"
            ALAN_APP_BUNDLE_NAME="Alan.app"
            ALAN_DISPLAY_NAME="Alan"
            ALAN_BUNDLE_ID="app.alanworks.macos"
            ALAN_PRIVILEGED_HELPER_LABEL="app.alanworks.macos.privileged-helper"
            ALAN_CLI_NAME="alan"
            ALAN_HOME_DISPLAY="~/.alan"
            ALAN_GLOBAL_SKILLS_DIR_DISPLAY="~/.agents/skills"
            ALAN_SHELL_CONTROL_NAMESPACE="alan-shell-control"
            ;;
        dev)
            ALAN_CHANNEL_ID="dev"
            ALAN_APP_BUNDLE_NAME="Alan Dev.app"
            ALAN_DISPLAY_NAME="Alan Dev"
            ALAN_BUNDLE_ID="app.alanworks.macos.dev"
            ALAN_PRIVILEGED_HELPER_LABEL="app.alanworks.macos.dev.privileged-helper"
            ALAN_CLI_NAME="alan-dev"
            ALAN_HOME_DISPLAY="~/.alan-dev"
            ALAN_GLOBAL_SKILLS_DIR_DISPLAY="~/.agents-dev/skills"
            ALAN_SHELL_CONTROL_NAMESPACE="alan-dev-shell-control"
            ;;
        *)
            printf 'error: unknown alan install channel: %s\n' "$channel" >&2
            return 1
            ;;
    esac

    export ALAN_CHANNEL_ID
    export ALAN_APP_BUNDLE_NAME
    export ALAN_DISPLAY_NAME
    export ALAN_BUNDLE_ID
    export ALAN_PRIVILEGED_HELPER_LABEL
    export ALAN_CLI_NAME
    export ALAN_HOME_DISPLAY
    export ALAN_GLOBAL_SKILLS_DIR_DISPLAY
    export ALAN_SHELL_CONTROL_NAMESPACE
}

alan_install_channel_is_stable() {
    [[ "${ALAN_CHANNEL_ID:-}" == "stable" ]]
}

alan_install_channel_is_dev() {
    [[ "${ALAN_CHANNEL_ID:-}" == "dev" ]]
}
