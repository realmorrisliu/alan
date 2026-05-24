#!/usr/bin/env bash

alan_install_channel_load() {
    local channel="${1:-stable}"

    case "$channel" in
        stable)
            ALAN_CHANNEL_ID="stable"
            ALAN_APP_BUNDLE_NAME="Alan.app"
            ALAN_DISPLAY_NAME="Alan"
            ALAN_BUNDLE_ID="app.alanworks.macos"
            ALAN_CLI_NAME="alan"
            ALAN_HOME_DISPLAY="~/.alan"
            ALAN_GLOBAL_SKILLS_DIR_DISPLAY="~/.agents/skills"
            ALAN_DAEMON_BIND="0.0.0.0:8090"
            ALAN_DAEMON_URL="http://127.0.0.1:8090"
            ALAN_SHELL_CONTROL_NAMESPACE="alan-shell-control"
            ALAN_LEGACY_APP_BUNDLE_NAME="alan.app"
            ;;
        dev)
            ALAN_CHANNEL_ID="dev"
            ALAN_APP_BUNDLE_NAME="Alan Dev.app"
            ALAN_DISPLAY_NAME="Alan Dev"
            ALAN_BUNDLE_ID="app.alanworks.macos.dev"
            ALAN_CLI_NAME="alan-dev"
            ALAN_HOME_DISPLAY="~/.alan-dev"
            ALAN_GLOBAL_SKILLS_DIR_DISPLAY="~/.agents-dev/skills"
            ALAN_DAEMON_BIND="127.0.0.1:8091"
            ALAN_DAEMON_URL="http://127.0.0.1:8091"
            ALAN_SHELL_CONTROL_NAMESPACE="alan-dev-shell-control"
            ALAN_LEGACY_APP_BUNDLE_NAME=""
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
    export ALAN_CLI_NAME
    export ALAN_HOME_DISPLAY
    export ALAN_GLOBAL_SKILLS_DIR_DISPLAY
    export ALAN_DAEMON_BIND
    export ALAN_DAEMON_URL
    export ALAN_SHELL_CONTROL_NAMESPACE
    export ALAN_LEGACY_APP_BUNDLE_NAME
}

alan_install_channel_is_stable() {
    [[ "${ALAN_CHANNEL_ID:-}" == "stable" ]]
}

alan_install_channel_is_dev() {
    [[ "${ALAN_CHANNEL_ID:-}" == "dev" ]]
}
