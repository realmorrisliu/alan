#!/usr/bin/env bash

alan_path_exists() {
    local path="$1"

    [[ -e "$path" || -L "$path" ]]
}

alan_is_distinct_existing_path() {
    local candidate="$1"
    local reference="$2"

    alan_path_exists "$candidate" || return 1
    alan_path_exists "$reference" || return 0

    [[ ! "$candidate" -ef "$reference" ]]
}

alan_sparkle_version_dir() {
    local framework="$1"
    local versions_dir="$framework/Versions"
    local current="$versions_dir/Current"
    local candidate

    if [[ -d "$current" ]]; then
        (cd "$current" && pwd -P)
        return
    fi

    for candidate in "$versions_dir"/*; do
        [[ -d "$candidate" ]] || continue
        [[ "$(basename "$candidate")" == "Current" ]] && continue
        (cd "$candidate" && pwd -P)
        return
    done

    return 1
}
