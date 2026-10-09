#!/usr/bin/env bash

require_jq() {
    if ! command -v jq >/dev/null 2>&1; then
        echo "jq is required to parse harness fixture JSON." >&2
        exit 1
    fi
}

extract_json_string_field() {
    local file="$1"
    local key="$2"
    jq -er --arg key "$key" '.[$key] | strings' "$file"
}

extract_json_bool_field() {
    local file="$1"
    local key="$2"
    local value
    value="$(jq -r --arg key "$key" '.[$key]' "$file" 2>/dev/null || true)"
    if [[ "$value" != "true" && "$value" != "false" ]]; then
        return 1
    fi
    printf "%s\n" "$value"
}

record_executed_scenario() {
    local scenario_id="$1"
    local output_file="$2"
    printf "%s\n" "$scenario_id" >>"$output_file"
}

record_fixture_kpi_tags() {
    local fixture_path="$1"
    local output_file="$2"
    jq -r '.kpi_tags[]?' "$fixture_path" >>"$output_file"
}

build_json_string_array() {
    local values_file="$1"
    if [[ -s "$values_file" ]]; then
        jq -Rn '[inputs | select(length > 0)]' <"$values_file"
    else
        printf '[]\n'
    fi
}

build_kpi_tag_counts_json() {
    local tags_file="$1"
    if [[ -s "$tags_file" ]]; then
        jq -Rn '
            [inputs | select(length > 0)]
            | group_by(.)
            | map({key: .[0], value: length})
            | from_entries
        ' <"$tags_file"
    else
        printf '{}\n'
    fi
}

run_harness_command() {
    local root="$1" command="$2"
    python3 "$root/scripts/build_artifacts.py" --owner "${ALAN_BUILD_OWNER:-$root}" \
        run --workspace "$root" -- bash -lc "$command"
}

validate_exact_cargo_filters() {
    local repo_root="$1"
    local scenario_id="$2"
    local scenario_cmd="$3"
    local segment list_output

    while IFS= read -r segment; do
        segment="$(printf "%s" "$segment" | sed -E 's/^[[:space:]]+|[[:space:]]+$//g')"
        if [[ -z "$segment" ]]; then
            continue
        fi
        if [[ "$segment" == cargo\ test* && "$segment" == *"-- --exact"* ]]; then
            if ! list_output="$(run_harness_command "$repo_root" "$segment --list" 2>&1)"; then
                echo "Scenario ${scenario_id} has invalid exact cargo test filter: ${segment}" >&2
                echo "$list_output" >&2
                return 1
            fi
            if ! printf "%s\n" "$list_output" | grep -Eq ':[[:space:]]+test$'; then
                echo "Scenario ${scenario_id} exact cargo filter matched zero tests: ${segment}" >&2
                return 1
            fi
        fi
    done < <(printf "%s" "$scenario_cmd" | sed 's/&&/\n/g')
}
