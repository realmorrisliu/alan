#!/usr/bin/env bash
set -euo pipefail
shopt -s nocasematch

repo_root="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
config_file="$repo_root/openspec/config.yaml"
spec_root="$repo_root/openspec/specs"
changes_root="$repo_root/openspec/changes"
allowlist_file="$repo_root/scripts/openspec-current-surface-allowlist.txt"
violations=()

if ! command -v rg >/dev/null 2>&1; then
    printf 'error: rg is required by the OpenSpec current-surface guard\n' >&2
    exit 1
fi

relative_path() {
    local path="$1"
    printf '%s' "${path#"$repo_root"/}"
}

is_allowlisted() {
    local rule="$1"
    local file="$2"
    local text="$3"
    local allowed_rule allowed_file literal rationale

    [[ -f "$allowlist_file" ]] || return 1
    while IFS='|' read -r allowed_rule allowed_file literal rationale; do
        [[ -n "$allowed_rule" && "${allowed_rule#\#}" == "$allowed_rule" ]] || continue
        [[ "$allowed_rule" == "$rule" ]] || continue
        [[ "$allowed_file" == "$file" ]] || continue
        [[ -n "$literal" && "$text" == *"$literal"* ]] || continue
        return 0
    done <"$allowlist_file"
    return 1
}

record_violation() {
    local rule="$1"
    local file="$2"
    local line_number="$3"
    local text="$4"
    local relative
    relative="$(relative_path "$file")"

    if is_allowlisted "$rule" "$relative" "$text"; then
        return
    fi
    violations+=("$relative:$line_number: [$rule] $text")
}

scan_purpose_placeholders() {
    local file line line_number pattern
    [[ -d "$spec_root" ]] || return

    pattern='^TBD[[:space:]]*-[[:space:]]*created[[:space:]]+by[[:space:]]+archiving[[:space:]]+change.*Update[[:space:]]+Purpose[[:space:]]+after[[:space:]]+archive\.?$'
    while IFS=: read -r file line_number line; do
        [[ -n "$file" ]] || continue
        record_violation "purpose-placeholder" "$file" "$line_number" "$line"
    done < <(rg --no-heading --color never -n -i --glob 'spec.md' "$pattern" "$spec_root" || true)
}

scan_rule_keys() {
    local line line_number in_rules key
    local supported=" proposal design specs tasks "
    [[ -f "$config_file" ]] || {
        violations+=("openspec/config.yaml:0: [missing-config] OpenSpec config is missing")
        return
    }

    in_rules=0
    line_number=0
    while IFS= read -r line || [[ -n "$line" ]]; do
        line_number=$((line_number + 1))
        if [[ "$line" =~ ^rules:[[:space:]]*$ ]]; then
            in_rules=1
            continue
        fi
        ((in_rules)) || continue
        if [[ "$line" =~ ^[^[:space:]#] ]]; then
            in_rules=0
            continue
        fi
        if [[ "$line" =~ ^[[:space:]][[:space:]]([[:alnum:]_-]+):[[:space:]]*$ ]]; then
            key="${BASH_REMATCH[1]}"
            if [[ "$supported" != *" $key "* ]]; then
                record_violation "unsupported-rule-key" "$config_file" "$line_number" "$line"
            fi
        fi
    done <"$config_file"
}

is_deleted_source_reference() {
    local line="$1"
    local pattern='(^|[^[:alnum:]_])(clients/apple/alan-macos/)?(Views/Console|Controllers/Console|Models/Console|Services/Daemon)(/|[^[:alnum:]_]|$)|crates/alan/src/(cli/daemon\.rs|daemon(/|[^[:alnum:]_]|$))|crates/tui/src/daemon_client\.rs'
    [[ "$line" =~ $pattern ]]
}

is_bridge_identifier() {
    local line="$1"
    local identifier_pattern='ShellContentInstance|GrooveMasterHostCompatibilityBridge|UPDFPreviewHostCompatibilityBridge|AlanVoiceHostCompatibilityBridge'

    [[ "$line" =~ $identifier_pattern ]]
}

is_bridge_authorization() {
    local text="$1"
    local namespace_bootstrap_pattern='namespace[- ]bootstrap[[:space:]-]+compatibility[[:space:]-]+projection'
    local authorization_verbs='add|adds|added|adding|introduce|introduces|introduced|introducing|implement|implements|implemented|implementing|create|creates|created|creating|use|uses|used|using|provide|provides|provided|providing|permit|permits|permitted|permitting|allow|allows|allowed|allowing|authorize|authorizes|authorized|authorizing|land|lands|landed|landing|schedule|schedules|scheduled|scheduling|retain|retains|retained|retaining|keep|keeps|kept|keeping'
    local authorized_pattern="(^|[^[:alnum:]_])($authorization_verbs)([^[:alnum:]_]|$)[^.;]*(callback|dto|contentinstance|host[- ]action|host[- ]compatibility|compatibility|event[- ]broadcast)[^.;]*(bridge|facade|façade|projection|translation layer)"
    local normative_pattern='(^|[^[:alnum:]_])(shall|must|may|will)([^[:alnum:]_]|$)[^.;]*(callback|dto|contentinstance|host[- ]action|host[- ]compatibility|compatibility)[^.;]*(bridge|facade|façade|projection|translation layer)'

    [[ "$text" =~ $namespace_bootstrap_pattern ]] && return 0
    [[ "$text" =~ $authorized_pattern ]] && return 0
    [[ "$text" =~ $normative_pattern ]]
}

scan_bridge_paragraphs() {
    local file="$1"
    local line normalized_line line_number paragraph paragraph_start block_start_pattern

    line_number=0
    paragraph=""
    paragraph_start=0
    block_start_pattern='^(#{1,6}[[:space:]]|[-*][[:space:]]|[[:digit:]]+\.[[:space:]])'
    while IFS= read -r line || [[ -n "$line" ]]; do
        line_number=$((line_number + 1))
        if [[ -z "$line" ]]; then
            if [[ -n "$paragraph" ]] && ! is_bridge_identifier "$paragraph" && is_bridge_authorization "$paragraph"; then
                record_violation "bridge-authorization" "$file" "$paragraph_start" "$paragraph"
            fi
            paragraph=""
            paragraph_start=0
            continue
        fi
        if [[ -n "$paragraph" && "$line" =~ $block_start_pattern ]]; then
            if ! is_bridge_identifier "$paragraph" && is_bridge_authorization "$paragraph"; then
                record_violation "bridge-authorization" "$file" "$paragraph_start" "$paragraph"
            fi
            paragraph=""
            paragraph_start=0
        fi
        normalized_line="${line#"${line%%[![:space:]]*}"}"
        if [[ -z "$paragraph" ]]; then
            paragraph_start="$line_number"
            paragraph="$normalized_line"
        else
            paragraph="$paragraph $normalized_line"
        fi
    done <"$file"

    if [[ -n "$paragraph" ]] && ! is_bridge_identifier "$paragraph" && is_bridge_authorization "$paragraph"; then
        record_violation "bridge-authorization" "$file" "$paragraph_start" "$paragraph"
    fi
}

scan_current_documents() {
    local file line line_number candidate_pattern
    candidate_pattern='Views/Console|Controllers/Console|Models/Console|Services/Daemon|crates/alan/src/daemon|crates/alan/src/cli/daemon\.rs|daemon_client\.rs|ShellContentInstance|HostCompatibilityBridge|namespace[- ]bootstrap|callback|\bdtos?\b|ContentInstance|host[- ]action|compatibility|event[- ]broadcast'

    while IFS= read -r file; do
        [[ -n "$file" ]] || continue
        scan_bridge_paragraphs "$file"
    done < <(rg --no-heading --color never -l -i --glob '*.md' --glob '!**/archive/**' "$candidate_pattern" "$spec_root" "$changes_root" || true)

    while IFS=: read -r file line_number line; do
        [[ -n "$file" ]] || continue
        if is_deleted_source_reference "$line"; then
            record_violation "deleted-source-reference" "$file" "$line_number" "$line"
        fi
        if is_bridge_identifier "$line"; then
            record_violation "bridge-identifier" "$file" "$line_number" "$line"
        fi
    done < <(rg --no-heading --color never -n -i --glob '*.md' --glob '!**/archive/**' "$candidate_pattern" "$spec_root" "$changes_root" || true)
}

check_instruction_lookup() {
    local change artifact output
    [[ "${OPEN_SPEC_CURRENT_SURFACE_SKIP_INSTRUCTIONS:-0}" == "1" ]] && return

    if ! command -v openspec >/dev/null 2>&1; then
        violations+=("openspec/config.yaml:0: [instruction-check] openspec CLI is required")
        return
    fi

    change="${OPEN_SPEC_CURRENT_SURFACE_CHANGE:-clean-canonical-spec-debt}"
    if [[ ! -d "$changes_root/$change" ]]; then
        change="$(find "$changes_root" -mindepth 1 -maxdepth 1 -type d ! -name archive -print | sort | head -n 1)"
        change="${change##*/}"
    fi
    if [[ -z "$change" ]]; then
        violations+=("openspec/changes:0: [instruction-check] no active change is available for instruction lookup")
        return
    fi

    for artifact in proposal design specs tasks; do
        if ! output="$(cd "$repo_root" && openspec instructions "$artifact" --change "$change" --json 2>&1)"; then
            violations+=("openspec/config.yaml:0: [instruction-check] $artifact instructions failed: $output")
            continue
        fi
        if printf '%s\n' "$output" | grep -Eiq '^warning:|unknown[[:space:]-]+artifact|not[[:space:]]+recognized'; then
            violations+=("openspec/config.yaml:0: [instruction-check] $artifact instructions emitted a schema warning: $output")
        fi
    done
}

scan_purpose_placeholders
scan_rule_keys
scan_current_documents
check_instruction_lookup

if ((${#violations[@]})); then
    printf 'OpenSpec current-surface violations:\n' >&2
    printf '%s\n' "${violations[@]}" >&2
    printf '\nArchived changes are intentionally excluded. Remove current debt or add an exact, reasoned allowlist entry.\n' >&2
    exit 1
fi

printf 'OpenSpec current-surface guard passed\n'
