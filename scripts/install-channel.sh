#!/usr/bin/env bash

alan_install_channel_load() {
    local channel="${1:-stable}"

    case "$channel" in
        stable)
            ALAN_CHANNEL_ID="stable"
            ALAN_CLI_NAME="alan"
            ALAN_OS_HOST_NAME="alan-os-host"
            ALAN_SYSTEM_STORE_DISPLAY="~/Library/Application Support/Alan/System Store/stable"
            ALAN_HOST_STORE_DISPLAY="~/Library/Application Support/Alan/Host Store/stable"
            ALAN_SHELL_CONTROL_NAMESPACE="alan-shell-control"
            ;;
        dev)
            ALAN_CHANNEL_ID="dev"
            ALAN_CLI_NAME="alan-dev"
            ALAN_OS_HOST_NAME="alan-os-host-dev"
            ALAN_SYSTEM_STORE_DISPLAY="~/Library/Application Support/Alan/System Store/dev"
            ALAN_HOST_STORE_DISPLAY="~/Library/Application Support/Alan/Host Store/dev"
            ALAN_SHELL_CONTROL_NAMESPACE="alan-dev-shell-control"
            ;;
        *)
            printf 'error: unknown alan install channel: %s\n' "$channel" >&2
            return 1
            ;;
    esac

    export ALAN_CHANNEL_ID
    export ALAN_CLI_NAME
    export ALAN_OS_HOST_NAME
    export ALAN_SYSTEM_STORE_DISPLAY
    export ALAN_HOST_STORE_DISPLAY
    export ALAN_SHELL_CONTROL_NAMESPACE
}

alan_install_channel_is_stable() {
    [[ "${ALAN_CHANNEL_ID:-}" == "stable" ]]
}

alan_install_channel_is_dev() {
    [[ "${ALAN_CHANNEL_ID:-}" == "dev" ]]
}
