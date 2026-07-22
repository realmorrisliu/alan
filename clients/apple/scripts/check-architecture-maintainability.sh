#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
APPLE_ROOT="$REPO_ROOT/clients/apple"
SOURCE_ROOT="$APPLE_ROOT/alan-macos"
HELPER_SOURCE_ROOT="$APPLE_ROOT/alan-macos-privileged-helper"
PROJECT_FILE="$APPLE_ROOT/alan-macos.xcodeproj/project.pbxproj"
README_FILE="$APPLE_ROOT/README.md"
ARCH_DOC="$APPLE_ROOT/ARCHITECTURE.md"
WARNING_BASELINE="$SCRIPT_DIR/architecture-warning-baseline.txt"
WARNING_BASELINE_REL="clients/apple/scripts/architecture-warning-baseline.txt"
STRICT=0

if [[ "${1:-}" == "--strict" ]]; then
    STRICT=1
fi

warnings=0
failures=0
warning_inventory="$(mktemp)"
warning_inventory_sorted="$(mktemp)"
warning_baseline_body="$(mktemp)"
warning_baseline_sorted="$(mktemp)"
base_warning_baseline="$(mktemp)"
trap 'rm -f "$warning_inventory" "$warning_inventory_sorted" "$warning_baseline_body" "$warning_baseline_sorted" "$base_warning_baseline"' EXIT

git_command=(git)
if [[ -n "${ALAN_QUALITY_GIT_DIR:-}" ]]; then
    git_command+=(--git-dir="$ALAN_QUALITY_GIT_DIR")
fi
base_ref="${ALAN_QUALITY_BASE_REF:-HEAD}"

"$SCRIPT_DIR/check-brand-identity.sh"

warn() {
    local key="$1"
    shift
    printf 'warning: %s\n' "$1"
    printf '%s\n' "$key" >>"$warning_inventory"
    warnings=$((warnings + 1))
}

fail() {
    printf 'error: %s\n' "$1" >&2
    failures=$((failures + 1))
}

xcode_source_phase_contains() {
    local phase_id="$1"
    local source_name="$2"

    awk -v phase_id="$phase_id" -v source_name="$source_name" '
        index($0, phase_id " /* Sources */ = {") {
            in_phase = 1
        }
        in_phase && index($0, source_name " in Sources") {
            found = 1
        }
        in_phase && /runOnlyForDeploymentPostprocessing = 0;/ {
            exit found ? 0 : 1
        }
        END {
            if (!in_phase || !found) {
                exit 1
            }
        }
    ' "$PROJECT_FILE"
}

require_xcode_source_phase() {
    local phase_id="$1"
    local source_name="$2"
    local owner="$3"

    if ! xcode_source_phase_contains "$phase_id" "$source_name"; then
        fail "$owner must compile $source_name"
    fi
}

reject_xcode_source_phase() {
    local phase_id="$1"
    local source_name="$2"
    local owner="$3"

    if xcode_source_phase_contains "$phase_id" "$source_name"; then
        fail "$owner must not compile $source_name"
    fi
}

validate_warning_baseline() {
    local duplicate

    if [[ ! -f "$WARNING_BASELINE" ]]; then
        fail "clients/apple/scripts/architecture-warning-baseline.txt must record accepted warnings"
        return
    fi

    if ! awk -F '|' '
        /^[[:space:]]*#/ || NF == 0 { next }
        $1 == "large" {
            if (NF != 3 || $2 == "" || $3 !~ /^[0-9]+$/) {
                printf "invalid Apple warning baseline entry: %s\n", $0 > "/dev/stderr"
                invalid = 1
                next
            }
            print
            next
        }
        $1 == "bridge" {
            if (NF != 3 || $2 == "" || $3 == "") {
                printf "invalid Apple warning baseline entry: %s\n", $0 > "/dev/stderr"
                invalid = 1
                next
            }
            print
            next
        }
        $1 == "missing-target-folder" ||
        $1 == "readme-missing-file" ||
        $1 == "readme-missing-folder" ||
        $1 == "readme-missing-command" {
            if (NF != 2 || $2 == "") {
                printf "invalid Apple warning baseline entry: %s\n", $0 > "/dev/stderr"
                invalid = 1
                next
            }
            print
            next
        }
        {
            printf "unknown Apple warning baseline entry: %s\n", $0 > "/dev/stderr"
            invalid = 1
        }
        END { exit invalid }
    ' "$WARNING_BASELINE" >"$warning_baseline_body"; then
        fail "Apple architecture warning baseline is malformed"
        return
    fi

    LC_ALL=C sort "$warning_inventory" >"$warning_inventory_sorted"
    LC_ALL=C sort "$warning_baseline_body" >"$warning_baseline_sorted"

    duplicate="$(uniq -d "$warning_baseline_sorted" | head -n 1)"
    if [[ -n "$duplicate" ]]; then
        fail "Apple architecture warning baseline contains duplicate entry: $duplicate"
    fi

    duplicate="$(uniq -d "$warning_inventory_sorted" | head -n 1)"
    if [[ -n "$duplicate" ]]; then
        fail "Apple architecture report emitted duplicate warning key: $duplicate"
    fi

    if ! cmp -s "$warning_baseline_body" "$warning_baseline_sorted"; then
        fail "Apple architecture warning baseline entries must stay sorted"
    fi

    if ! cmp -s "$warning_inventory_sorted" "$warning_baseline_sorted"; then
        printf 'Apple architecture warning ledger drift:\n' >&2
        diff -u "$warning_baseline_sorted" "$warning_inventory_sorted" >&2 || true
        fail "update the Apple warning baseline in the same reduction change"
    fi
}

compare_warning_baseline_with_base() {
    if ! "${git_command[@]}" cat-file -e "$base_ref^{commit}" 2>/dev/null; then
        fail "Apple architecture warning ratchet base is not a commit: $base_ref"
        return
    fi

    if ! "${git_command[@]}" cat-file -e "$base_ref:$WARNING_BASELINE_REL" 2>/dev/null; then
        printf 'Apple architecture warning baseline established relative to %s.\n' "$base_ref"
        return
    fi

    "${git_command[@]}" show "$base_ref:$WARNING_BASELINE_REL" >"$base_warning_baseline"
    if ! awk -F '|' '
        NR == FNR {
            if ($0 ~ /^[[:space:]]*#/ || NF == 0) {
                next
            }
            if ($1 == "large") {
                previous_large[$2] = $3
            } else {
                previous[$0] = 1
            }
            next
        }
        $1 == "large" {
            if (!($2 in previous_large)) {
                printf "error: new Apple large-file warning: %s\n", $2 > "/dev/stderr"
                failed = 1
            } else if ($3 > previous_large[$2]) {
                printf "error: Apple large-file debt grew for %s from %d to %d lines\n", $2, previous_large[$2], $3 > "/dev/stderr"
                failed = 1
            }
            next
        }
        !($0 in previous) {
            printf "error: new or broadened Apple architecture warning: %s\n", $0 > "/dev/stderr"
            failed = 1
        }
        END { exit failed }
    ' "$base_warning_baseline" "$warning_baseline_body"; then
        fail "Apple architecture warning debt may shrink but must not grow"
    fi
}

contains_line() {
    local needle="$1"
    shift
    local item
    for item in "$@"; do
        [[ "$item" == "$needle" ]] && return 0
    done
    return 1
}

swift_code_lines() {
    awk '
        function executable_code(value,    character, code, column, pair, triple) {
            code = ""
            column = 1
            while (column <= length(value)) {
                pair = substr(value, column, 2)
                triple = substr(value, column, 3)
                character = substr(value, column, 1)

                if (block_comment_depth > 0) {
                    if (pair == "/*") {
                        block_comment_depth++
                        column += 2
                    } else if (pair == "*/") {
                        block_comment_depth--
                        column += 2
                    } else {
                        column++
                    }
                    continue
                }

                if (string_width > 0 && interpolation_depth == 0) {
                    if (string_width == 3 && triple == "\"\"\"") {
                        string_width = 0
                        code = code " "
                        column += 3
                        continue
                    }
                    if (string_width == 1 && character == "\"") {
                        string_width = 0
                        code = code " "
                        column++
                        continue
                    }
                    if (pair == "\\(") {
                        interpolation_depth = 1
                        code = code " "
                        column += 2
                        continue
                    }
                    if (character == "\\") {
                        column += (column < length(value) ? 2 : 1)
                        continue
                    }
                    column++
                    continue
                }

                if (interpolation_string_width > 0) {
                    if (interpolation_string_width == 3 && triple == "\"\"\"") {
                        interpolation_string_width = 0
                        code = code " "
                        column += 3
                        continue
                    }
                    if (interpolation_string_width == 1 && character == "\"") {
                        interpolation_string_width = 0
                        code = code " "
                        column++
                        continue
                    }
                    if (character == "\\") {
                        column += (column < length(value) ? 2 : 1)
                        continue
                    }
                    column++
                    continue
                }

                if (pair == "//") {
                    break
                }
                if (pair == "/*") {
                    block_comment_depth = 1
                    column += 2
                    continue
                }
                if (triple == "\"\"\"") {
                    code = code " "
                    if (interpolation_depth > 0) {
                        interpolation_string_width = 3
                    } else {
                        string_width = 3
                    }
                    column += 3
                    continue
                }
                if (character == "\"") {
                    code = code " "
                    if (interpolation_depth > 0) {
                        interpolation_string_width = 1
                    } else {
                        string_width = 1
                    }
                    column++
                    continue
                }

                if (interpolation_depth > 0 && character == "(") {
                    interpolation_depth++
                } else if (interpolation_depth > 0 && character == ")") {
                    interpolation_depth--
                    if (interpolation_depth == 0) {
                        code = code " "
                        column++
                        continue
                    }
                }

                code = code character
                column++
            }
            return code
        }
        FNR == 1 {
            block_comment_depth = 0
            interpolation_depth = 0
            interpolation_string_width = 0
            string_width = 0
        }
        {
            printf "%s\t%d\t%s\n", FILENAME, FNR, executable_code($0)
        }
    ' "$@"
}

swift_code_matching_lines() {
    local pattern="$1"
    shift

    awk -v pattern="$pattern" '
        {
            first_separator = index($0, "\t")
            remainder = substr($0, first_separator + 1)
            second_separator = index(remainder, "\t")
            file = substr($0, 1, first_separator - 1)
            line_number = substr(remainder, 1, second_separator - 1)
            code = substr(remainder, second_separator + 1)
            if (code ~ pattern) {
                printf "%s:%s:%s\n", file, line_number, code
                matched = 1
            }
        }
        END { exit matched ? 0 : 1 }
    ' < <(swift_code_lines "$@")
}

swift_tree_code_matching_lines() {
    local pattern="$1"
    local root="$2"
    local file
    local matched=0

    while IFS= read -r file; do
        if swift_code_matching_lines "$pattern" "$file"; then
            matched=1
        fi
    done < <(grep -RIl --include='*.swift' -E "$pattern" "$root" || true)

    (( matched > 0 ))
}

reject_unapproved_symbol_lines() {
    local file="$1"
    local symbol_pattern="$2"
    local allowed_pattern="$3"
    local description="$4"
    local match_file
    local match_line
    local rejected=0
    local source_line

    while IFS=: read -r match_file match_line source_line; do
        if [[ ! "$source_line" =~ $allowed_pattern ]]; then
            printf '%s:%s:%s\n' "$match_file" "$match_line" "$source_line" >&2
            rejected=1
        fi
    done < <(swift_code_matching_lines "$symbol_pattern" "$file" || true)

    if (( rejected > 0 )); then
        fail "$description"
    fi
}

normalized_file_matches() {
    local file="$1"
    local pattern="$2"

    awk -v pattern="$pattern" '
        {
            first_separator = index($0, "\t")
            remainder = substr($0, first_separator + 1)
            second_separator = index(remainder, "\t")
            source = source " " substr(remainder, second_separator + 1)
        }
        END {
            gsub(/[[:space:]]+/, " ", source)
            exit source ~ pattern ? 0 : 1
        }
    ' < <(swift_code_lines "$file")
}

readonly OWNERSHIP_AWK_HELPERS='
    function brace_delta(value,    character, column, delta) {
        delta = 0
        for (column = 1; column <= length(value); column++) {
            character = substr(value, column, 1)
            if (character == "{") {
                delta++
            } else if (character == "}") {
                delta--
            }
        }
        return delta
    }
    function matching_call_end(value, opening,    character, column, depth) {
        depth = 0
        for (column = opening; column <= length(value); column++) {
            character = substr(value, column, 1)
            if (character == "(") {
                depth++
            } else if (character == ")") {
                depth--
                if (depth == 0) {
                    return column
                }
            }
        }
        return 0
    }
    function is_generic_angle_open(value, column,    next_character, previous_character) {
        previous_character = substr(value, column - 1, 1)
        next_character = substr(value, column + 1, 1)
        return (previous_character ~ /[A-Za-z0-9_.>]/ || previous_character == ")" ||
                previous_character == "]") &&
            next_character != "" && next_character !~ /[[:space:]=<>]/
    }
    function top_level_binding_end(value, start,    angle_depth, brace_depth, bracket_depth, character, column, parenthesis_depth) {
        for (column = start; column <= length(value); column++) {
            character = substr(value, column, 1)
            if (character == "(") {
                parenthesis_depth++
            } else if (character == ")" && parenthesis_depth > 0) {
                parenthesis_depth--
            } else if (character == "[") {
                bracket_depth++
            } else if (character == "]" && bracket_depth > 0) {
                bracket_depth--
            } else if (character == "{") {
                brace_depth++
            } else if (character == "}" && brace_depth > 0) {
                brace_depth--
            } else if (character == "<" && is_generic_angle_open(value, column)) {
                angle_depth++
            } else if (character == ">" && angle_depth > 0) {
                angle_depth--
            } else if (character == "," && parenthesis_depth == 0 &&
                bracket_depth == 0 && brace_depth == 0 && angle_depth == 0)
            {
                return column
            }
        }
        return length(value) + 1
    }
    function instance_receiver_invokes_factory(value, factories,    call_end, pattern, entry, fields, method, opening, receiver, remainder, suffix) {
        for (entry in factories) {
            split(entry, fields, "|")
            receiver = fields[1]
            method = fields[2]
            if (receiver == "" || method == "") {
                continue
            }
            pattern = "(^|[^A-Za-z0-9_])" receiver \
                "([ ]*<[^>]+>)?([ ]*[.][ ]*init)?[ ]*\\("
            remainder = value
            while (match(remainder, pattern)) {
                opening = RSTART + RLENGTH - 1
                call_end = matching_call_end(remainder, opening)
                if (call_end == 0) {
                    break
                }
                suffix = substr(remainder, call_end + 1)
                if (suffix ~ ("^[ ]*[!?]?[ ]*[.][ ]*" method "[ ]*\\(")) {
                    return 1
                }
                remainder = substr(remainder, RSTART + 1)
            }
        }
        return 0
    }
'

typed_factory_declarations() {
    local file="$1"
    local target_type="$2"

    awk -v target_type="$target_type" '
        function leading_space_count(value,    prefix) {
            prefix = value
            sub(/[^ ].*$/, "", prefix)
            return length(prefix)
        }
        function is_type_declaration(value) {
            return value ~ /^[[:space:]]*[^\/]*(class|struct|actor|enum|extension|protocol)[ ]+[A-Za-z_][A-Za-z0-9_.]*/
        }
        function declared_type_name(value,    name) {
            name = value
            sub(/^.*(class|struct|actor|enum|extension|protocol)[ ]+/, "", name)
            sub(/[^A-Za-z0-9_.].*$/, "", name)
            sub(/^.*[.]/, "", name)
            return name
        }
        function is_factory_candidate(value) {
            return value ~ /^[[:space:]]*[^\/]*func[ ]+[A-Za-z_][A-Za-z0-9_]*[ ]*\(/
        }
        function is_same_indent_function_signature_continuation(value) {
            return value ~ /^[[:space:]]*(\)|->|[{])/ ||
                value ~ /^[[:space:]]*(async|throws|rethrows|where)([^A-Za-z0-9_]|$)/
        }
        function record_buffered_factory(    header, name, return_type, signature) {
            if (factory_buffer == "") {
                return
            }
            signature = factory_buffer
            gsub(/[[:space:]]+/, " ", signature)
            header = signature
            sub(/[{].*$/, "", header)
            return_type = header
            if (return_type !~ /->/) {
                factory_buffer = ""
                return
            }
            sub(/^.*->[ ]*/, "", return_type)
            if (return_type !~ ("(^|[^A-Za-z0-9_])" target_type "([^A-Za-z0-9_]|$)") &&
                !(factory_owner == target_type &&
                    return_type ~ /(^|[^A-Za-z0-9_])Self([^A-Za-z0-9_]|$)/))
            {
                factory_buffer = ""
                return
            }
            name = header
            sub(/^.*func[ ]+/, "", name)
            sub(/[^A-Za-z0-9_].*$/, "", name)
            print factory_owner "|" name
            factory_buffer = ""
        }
        {
            first_separator = index($0, "\t")
            remainder = substr($0, first_separator + 1)
            second_separator = index(remainder, "\t")
            line = substr(remainder, second_separator + 1)
            line_indent = leading_space_count(line)
            if (line !~ /^[[:space:]]*$/) {
                while (type_depth > 0 && line_indent <= type_indent[type_depth]) {
                    delete type_indent[type_depth]
                    delete type_name[type_depth]
                    type_depth--
                }
                if (is_type_declaration(line)) {
                    type_depth++
                    type_indent[type_depth] = line_indent
                    type_name[type_depth] = declared_type_name(line)
                }
            }

            if (is_factory_candidate(line)) {
                record_buffered_factory()
                factory_buffer = line
                factory_indent = line_indent
                factory_owner = type_depth > 0 ? type_name[type_depth] : ""
            } else if (factory_buffer != "" &&
                (line ~ /^[[:space:]]*$/ ||
                    line_indent > factory_indent ||
                    (line_indent == factory_indent &&
                        is_same_indent_function_signature_continuation(line))))
            {
                factory_buffer = factory_buffer " " line
            } else {
                record_buffered_factory()
            }
        }
        END { record_buffered_factory() }
    ' < <(swift_code_lines "$file")
}

typed_factory_inventory() {
    local target_type="$1"
    local file

    while IFS= read -r file; do
        typed_factory_declarations "$file" "$target_type"
    done < <(find "$SOURCE_ROOT" -type f -name '*.swift' -print | sort)
}

manifest_state_declarations() {
    local file="$1"
    local factory_inventory="$2"

    awk -v factory_inventory="$factory_inventory" "$OWNERSHIP_AWK_HELPERS"'
        BEGIN {
            factory_count = split(factory_inventory, factory_entries, ";")
            for (factory_index = 1; factory_index <= factory_count; factory_index++) {
                if (factory_entries[factory_index] != "") {
                    manifest_factories[factory_entries[factory_index]] = 1
                }
            }
        }
        function leading_space_count(value,    prefix) {
            prefix = value
            sub(/[^ ].*$/, "", prefix)
            return length(prefix)
        }
        function trim(value) {
            sub(/^[[:space:]]+/, "", value)
            sub(/[[:space:]]+$/, "", value)
            return value
        }
        function is_type_declaration(value) {
            return value ~ /^[[:space:]]*[^\/]*(class|struct|actor|enum|extension|protocol)[ ]+[A-Za-z_][A-Za-z0-9_.]*/
        }
        function declared_type_name(value,    name) {
            name = value
            sub(/^.*(class|struct|actor|enum|extension|protocol)[ ]+/, "", name)
            sub(/[^A-Za-z0-9_.].*$/, "", name)
            sub(/^.*[.]/, "", name)
            return name
        }
        function current_type_owner(    depth, owner) {
            if (type_depth == 0) {
                return "<module>"
            }
            for (depth = 1; depth <= type_depth; depth++) {
                owner = owner (owner == "" ? "" : ".") type_name[depth]
            }
            return owner
        }
        function manifest_factory_call_matches(call, owner,    name, parts, qualifier, segment_count, qualifier_index) {
            segment_count = split(call, parts, ".")
            name = parts[segment_count]
            if (name == "ShellContentWorkspaceManifest") {
                return 1
            }
            if (segment_count == 1) {
                if (name == "init" && owner == "ShellContentWorkspaceManifest") {
                    return 1
                }
                return ((owner "|" name) in manifest_factories) ||
                    (("|" name) in manifest_factories)
            }
            for (qualifier_index = 1; qualifier_index < segment_count; qualifier_index++) {
                qualifier = parts[qualifier_index]
                if (qualifier == "Self" || qualifier == "self") {
                    qualifier = owner
                }
                if (name == "init" && qualifier == "ShellContentWorkspaceManifest") {
                    return 1
                }
                if ((qualifier "|" name) in manifest_factories) {
                    return 1
                }
            }
            return parts[1] == "shared" && ((owner "|" name) in manifest_factories)
        }
        function inferred_factory_contains_manifest(value, owner,    call, expression) {
            if (value !~ /=/) {
                return 0
            }
            expression = value
            sub(/^[^=]*=[ ]*/, "", expression)
            gsub(/[ ]*[.][ ]*/, ".", expression)
            if (instance_receiver_invokes_factory(expression, manifest_factories)) {
                return 1
            }
            while (match(expression, /[A-Za-z_][A-Za-z0-9_.]*[ ]*\(/)) {
                call = substr(expression, RSTART, RLENGTH)
                sub(/[ ]*\($/, "", call)
                if (manifest_factory_call_matches(call, owner)) {
                    return 1
                }
                expression = substr(expression, RSTART + RLENGTH)
            }
            return 0
        }
        function inferred_generic_contains_manifest(value,    expression) {
            if (value !~ /=/) {
                return 0
            }
            expression = value
            sub(/^[^=]*=[ ]*/, "", expression)
            sub(/^try[!?]?[ ]+/, "", expression)
            sub(/^await[ ]+/, "", expression)
            return expression ~ /^[A-Za-z_][A-Za-z0-9_.]*[ ]*<[^>]*ShellContentWorkspaceManifest/ ||
                expression ~ /^\[[^]]*ShellContentWorkspaceManifest/
        }
        function record_manifest_binding(declaration,    header, prefix, kind) {
            declaration = trim(declaration)
            if (declaration == "") {
                return
            }
            header = declaration
            sub(/[=].*$/, "", header)

            if (header ~ /:[^=]*ShellContentWorkspaceManifest([^A-Za-z0-9_]|$)/) {
                prefix = header
                sub(/[ ]*:.*/, "", prefix)
                kind = "typed"
            } else if (declaration ~ /=[ ]*(try[!?]?[ ]+)?ShellContentWorkspaceManifest([.]init)?[ ]*\(/) {
                prefix = declaration
                sub(/[ ]*=[ ]*ShellContentWorkspaceManifest.*/, "", prefix)
                kind = "constructed"
            } else if (inferred_generic_contains_manifest(declaration)) {
                prefix = declaration
                sub(/[ ]*=.*/, "", prefix)
                kind = "inferred-generic"
            } else if (inferred_factory_contains_manifest(declaration, declaration_owner)) {
                prefix = declaration
                sub(/[ ]*=.*/, "", prefix)
                kind = "inferred-factory"
            } else {
                return
            }

            printf "%d|%s|%d|%s|%s\n", \
                start_line, declaration_owner_key, declaration_indent, trim(prefix), kind
        }
        function record_declaration(    binding, binding_end, binding_start, declaration) {
            if (buffer == "") {
                return
            }
            declaration = buffer
            gsub(/[[:space:]]+/, " ", declaration)
            declaration = trim(declaration)
            binding_start = 1
            while (binding_start <= length(declaration)) {
                binding_end = top_level_binding_end(declaration, binding_start)
                binding = substr(declaration, binding_start, binding_end - binding_start)
                record_manifest_binding(binding)
                binding_start = binding_end + 1
            }
            buffer = ""
        }
        {
            first_separator = index($0, "\t")
            remainder = substr($0, first_separator + 1)
            second_separator = index(remainder, "\t")
            source_line_number = substr(remainder, 1, second_separator - 1)
            line = substr(remainder, second_separator + 1)
            line_indent = leading_space_count(line)
            if (line !~ /^[[:space:]]*$/) {
                while (type_depth > 0 && line_indent <= type_indent[type_depth]) {
                    delete type_indent[type_depth]
                    delete type_name[type_depth]
                    type_depth--
                }
                if (is_type_declaration(line)) {
                    type_depth++
                    type_indent[type_depth] = line_indent
                    type_name[type_depth] = declared_type_name(line)
                }
            }

            if (line ~ /^[[:space:]]*[^\/]*(let|var)[ ]+[A-Za-z_][A-Za-z0-9_]*/) {
                record_declaration()
                buffer = line
                start_line = source_line_number
                declaration_indent = line_indent
                declaration_owner = type_depth > 0 ? type_name[type_depth] : ""
                declaration_owner_key = current_type_owner()
            } else if (buffer != "" &&
                (line ~ /^[[:space:]]*$/ || line_indent > declaration_indent))
            {
                buffer = buffer " " line
            } else {
                record_declaration()
            }
        }
        END { record_declaration() }
    ' < <(swift_code_lines "$file")
}

static_property_declarations() {
    local file="$1"

    awk '
        {
            first_separator = index($0, "\t")
            remainder = substr($0, first_separator + 1)
            second_separator = index(remainder, "\t")
            line = substr(remainder, second_separator + 1)
            if (line !~ /^[[:space:]]*[^\/]*(static|class)[ ]+([^ ]+[ ]+)*(let|var)[ ]+[A-Za-z_][A-Za-z0-9_]*/) {
                next
            }
            gsub(/[[:space:]]+/, " ", line)
            sub(/^ /, "", line)
            sub(/ $/, "", line)
            print line
        }
    ' < <(swift_code_lines "$file")
}

shell_host_global_storage_declarations() {
    local file="$1"
    local factory_inventory="$2"

    awk -v factory_inventory="$factory_inventory" "$OWNERSHIP_AWK_HELPERS"'
        BEGIN {
            factory_count = split(factory_inventory, factory_entries, ";")
            for (factory_index = 1; factory_index <= factory_count; factory_index++) {
                if (factory_entries[factory_index] != "") {
                    shell_host_factories[factory_entries[factory_index]] = 1
                }
            }
        }
        function leading_space_count(value,    prefix) {
            prefix = value
            sub(/[^ ].*$/, "", prefix)
            return length(prefix)
        }
        function is_static_property_start(value) {
            return value ~ /^[[:space:]]*[^\/]*(static|class)[ ]+([^ ]+[ ]+)*(let|var)[ ]+[A-Za-z_][A-Za-z0-9_]*/
        }
        function is_module_property_start(value) {
            return value ~ /^[^\/{]*(let|var)[ ]+[A-Za-z_][A-Za-z0-9_]*/
        }
        function is_type_declaration(value) {
            return value ~ /^[[:space:]]*[^\/]*(class|struct|actor|enum|extension|protocol)[ ]+[A-Za-z_][A-Za-z0-9_.]*/
        }
        function declared_type_name(value,    name) {
            name = value
            sub(/^.*(class|struct|actor|enum|extension|protocol)[ ]+/, "", name)
            sub(/[^A-Za-z0-9_.].*$/, "", name)
            sub(/^.*[.]/, "", name)
            return name
        }
        function shell_host_factory_call_matches(call, owner,    name, parts, qualifier, segment_count, qualifier_index) {
            segment_count = split(call, parts, ".")
            name = parts[segment_count]
            if (name == "ShellHostController") {
                return 1
            }
            if (segment_count == 1) {
                if (name == "init" && owner == "ShellHostController") {
                    return 1
                }
                return ((owner "|" name) in shell_host_factories) ||
                    (("|" name) in shell_host_factories)
            }
            for (qualifier_index = 1; qualifier_index < segment_count; qualifier_index++) {
                qualifier = parts[qualifier_index]
                if (qualifier == "Self" || qualifier == "self") {
                    qualifier = owner
                }
                if (name == "init" && qualifier == "ShellHostController") {
                    return 1
                }
                if ((qualifier "|" name) in shell_host_factories) {
                    return 1
                }
            }
            return parts[1] == "shared" && ((owner "|" name) in shell_host_factories)
        }
        function uses_shell_host_factory(value, owner,    call, expression) {
            if (value !~ /=/) {
                return 0
            }
            expression = value
            sub(/^[^=]*=[ ]*/, "", expression)
            gsub(/[ ]*[.][ ]*/, ".", expression)
            if (instance_receiver_invokes_factory(expression, shell_host_factories)) {
                return 1
            }
            while (match(expression, /[A-Za-z_][A-Za-z0-9_.]*[ ]*\(/)) {
                call = substr(expression, RSTART, RLENGTH)
                sub(/[ ]*\($/, "", call)
                if (shell_host_factory_call_matches(call, owner)) {
                    return 1
                }
                expression = substr(expression, RSTART + RLENGTH)
            }
            return 0
        }
        function is_stored_property(value,    declaration_header) {
            declaration_header = value
            sub(/[{].*$/, "", declaration_header)
            if (declaration_header ~ /=/) {
                return 1
            }
            return value !~ /[{]/ ||
                value ~ /[{][ ]*(didSet|willSet)([^A-Za-z0-9_]|$)/
        }
        function record_buffered_property(    property) {
            if (property_buffer == "") {
                return
            }
            property = property_buffer
            gsub(/[[:space:]]+/, " ", property)
            sub(/^ /, "", property)
            sub(/ $/, "", property)
            if (is_stored_property(property) &&
                (property ~ /ShellHostController([^A-Za-z0-9_]|$)/ ||
                    uses_shell_host_factory(property, property_owner)))
            {
                printf "%d|%s\n", property_start_line, property
            }
            property_buffer = ""
        }
        {
            first_separator = index($0, "\t")
            remainder = substr($0, first_separator + 1)
            second_separator = index(remainder, "\t")
            source_line_number = substr(remainder, 1, second_separator - 1)
            line = substr(remainder, second_separator + 1)
            line_indent = leading_space_count(line)
            if (line !~ /^[[:space:]]*$/) {
                while (type_depth > 0 && line_indent <= type_indent[type_depth]) {
                    delete type_indent[type_depth]
                    delete type_name[type_depth]
                    type_depth--
                }
                if (is_type_declaration(line)) {
                    type_depth++
                    type_indent[type_depth] = line_indent
                    type_name[type_depth] = declared_type_name(line)
                }
            }

            if (is_static_property_start(line) ||
                (brace_depth == 0 && is_module_property_start(line)))
            {
                record_buffered_property()
                property_buffer = line
                property_start_line = source_line_number
                property_indent = line_indent
                property_owner = type_depth > 0 ? type_name[type_depth] : ""
            } else if (property_buffer != "" &&
                (line ~ /^[[:space:]]*$/ || line_indent > property_indent))
            {
                property_buffer = property_buffer " " line
            } else {
                record_buffered_property()
            }
            brace_depth += brace_delta(line)
        }
        END { record_buffered_property() }
    ' < <(swift_code_lines "$file")
}

check_appkit_import_gate() {
    local rel="$1"
    local file="$2"
    if ! awk '
        /^#if .*os\(macOS\)/ || /^#elseif .*os\(macOS\)/ || /^#if .*canImport\(AppKit\)/ {
            inside_appkit_gate = 1
            next
        }
        /^#else/ || /^#endif/ {
            inside_appkit_gate = 0
            next
        }
        /^import AppKit$/ && !inside_appkit_gate {
            exit 1
        }
    ' "$file"; then
        fail "$rel imports AppKit before a macOS/AppKit platform gate"
    fi
}

current_root_swift_allowlist=(
    "AlanAppSingletonGuard.swift"
    "AlanApp.swift"
    "GhosttyLiveHost.swift"
    "MacShellRootView.swift"
    "ShellControlPlane.swift"
    "ShellHostController.swift"
    "TerminalPaneView.swift"
    "TerminalRuntimeRegistry.swift"
)

target_dirs=(
    "App"
    "Views/Shell"
    "Models"
    "Controllers"
    "Services"
    "Support"
)

large_file_threshold=1200

printf 'Apple architecture maintainability report\n'
printf 'Source root: clients/apple/alan-macos\n\n'

if [[ ! -f "$ARCH_DOC" ]]; then
    fail "clients/apple/ARCHITECTURE.md must record the architecture inventory and target layout"
else
    if ! grep -q "## Shell Core Boundary" "$ARCH_DOC"; then
        fail "clients/apple/ARCHITECTURE.md must document the Shell Core Boundary"
    fi
    if ! grep -q "new reusable domain behavior belongs in" "$ARCH_DOC"; then
        fail "clients/apple/ARCHITECTURE.md must keep the Rust shell-core ownership rule"
    fi
fi

if grep -Fq "ShellStateMutations.swift in Sources" "$PROJECT_FILE" \
    || grep -Fq "ShellTreeMutations.swift in Sources" "$PROJECT_FILE" \
    || grep -Fq "ShellStateMutationParitySupport.swift" "$PROJECT_FILE" \
    || grep -Fq "ShellTreeMutationParitySupport.swift" "$PROJECT_FILE"
then
    fail "Swift reducer parity support must stay out of the alan-macos Xcode target"
fi

if find "$REPO_ROOT/clients/apple/scripts/support" -name '*ParitySupport.swift' -print -quit | grep -q .; then
    fail "Swift parity support files must not be reintroduced; use shell-core contract tests or FFI-backed test builders"
fi

if ! grep -Fq "ShellStateRuntimeSupport.swift in Sources" "$PROJECT_FILE"; then
    fail "alan-macos target must keep the narrow runtime shell-state support owner"
fi

app_source_phase="000000000000000000000202"
helper_source_phase="A11000000000000000000202"

require_xcode_source_phase \
    "$app_source_phase" \
    "Services/Shell/AlanPrivilegedHelperXPC.swift" \
    "Alan macOS app target"
require_xcode_source_phase \
    "$helper_source_phase" \
    "Services/Shell/AlanPrivilegedHelperXPC.swift" \
    "privileged-helper target"

for app_only_source in \
    "AlanDarwinPtySpawn.c" \
    "Services/Shell/AlanPrivilegedHelperAppClient.swift" \
    "Services/Shell/AlanPrivilegedHelperService.swift" \
    "Services/Shell/AlanPrivilegedHelperXPCClient.swift"
do
    require_xcode_source_phase "$app_source_phase" "$app_only_source" "Alan macOS app target"
    reject_xcode_source_phase "$helper_source_phase" "$app_only_source" "privileged-helper target"
done

for helper_only_source in \
    "AlanPrivilegedHelperPtySpawn.c" \
    "Services/Shell/AlanPrivilegedHelperXPCRequirementChecker.swift" \
    "Services/Shell/AlanPrivilegedHelperXPCListener.swift" \
    "Services/Shell/AlanPrivilegedHelperXPCService.swift" \
    "Services/Shell/AlanPrivilegedHelperManagedUserWire.swift" \
    "Services/Shell/AlanPrivilegedHelperManagedUserService.swift" \
    "Services/Shell/AlanPrivilegedHelperPTYSessionStore.swift" \
    "Services/Shell/AlanPrivilegedHelperPTYSupport.swift"
do
    reject_xcode_source_phase "$app_source_phase" "$helper_only_source" "Alan macOS app target"
    require_xcode_source_phase "$helper_source_phase" "$helper_only_source" "privileged-helper target"
done

require_rust_reducer_adapter() {
    local file="$1"
    shift
    local forbidden

    if [[ ! -f "$file" ]]; then
        return
    fi
    for forbidden in "$@"; do
        if grep -Fq "$forbidden" "$file"; then
            fail "${file#$SOURCE_ROOT/} must route ${forbidden#shellState.} through the Rust shell-core adapter"
        fi
    done
}

require_single_owner_pattern() {
    local pattern="$1"
    local owner="$2"
    local description="$3"
    local file
    local rel

    while IFS= read -r file; do
        rel="${file#$SOURCE_ROOT/}"
        if [[ "$rel" != "$owner" ]]; then
            fail "$description must stay in $owner; found in $rel"
        fi
    done < <(grep -RIl --include='*.swift' -F "$pattern" "$SOURCE_ROOT" || true)
}

require_existing_single_owner_pattern() {
    local pattern="$1"
    local owner="$2"
    local description="$3"
    local file
    local rel
    local matched=0

    while IFS= read -r file; do
        matched=1
        rel="${file#$SOURCE_ROOT/}"
        if [[ "$rel" != "$owner" ]]; then
            fail "$description must stay in $owner; found in $rel"
        fi
    done < <(grep -RIl --include='*.swift' -F "$pattern" "$SOURCE_ROOT" || true)

    if [[ "$matched" -eq 0 ]]; then
        fail "$description must exist in $owner"
    fi
}

shell_core_ffi_shared_callsite_owner_allowlist=(
    "Models/Shell/ShellSettingsSurfaceModel.swift"
    "Services/Shell/ShellCoreFFIManagedTerminalAccountAdapter.swift"
    "Services/Shell/ShellActionCoordinator.swift"
    "Services/Shell/ShellLocalCommandExecutor.swift"
    "Services/Shell/ShellCoreFFIReducerAdapter.swift"
    "Services/Shell/ShellWorkspacePersistenceStartup.swift"
    "Services/Shell/ShellWorkspaceManifestStore.swift"
    "Services/Shell/TerminalProfileStore.swift"
    "Services/Terminal/TerminalBootResolution.swift"
)

shell_core_ffi_direct_init_owner_allowlist=(
    "Services/Shell/ShellCoreFFILoader.swift"
)

shell_core_ffi_raw_symbol_owner_allowlist=(
    "Services/Shell/ShellCoreFFILoader.swift"
)

require_shell_core_ffi_shared_callsite_owners() {
    local file
    local rel

    while IFS= read -r file; do
        rel="${file#$SOURCE_ROOT/}"
        if ! contains_line "$rel" "${shell_core_ffi_shared_callsite_owner_allowlist[@]}"; then
            fail "shell-core FFI shared calls must stay in documented owner files; found in $rel"
        fi
    done < <(grep -RIl --include='*.swift' -F "ShellCoreFFIAdapter.shared" "$SOURCE_ROOT" || true)
}

require_shell_core_ffi_direct_init_owners() {
    local file
    local rel

    while IFS= read -r file; do
        rel="${file#$SOURCE_ROOT/}"
        if ! contains_line "$rel" "${shell_core_ffi_direct_init_owner_allowlist[@]}"; then
            fail "direct shell-core FFI adapter construction must stay in the loader owner; found in $rel"
        fi
    done < <(grep -RIl --include='*.swift' -F "ShellCoreFFIAdapter(" "$SOURCE_ROOT" || true)
}

require_shell_core_ffi_raw_symbol_owners() {
    local file
    local rel

    while IFS= read -r file; do
        rel="${file#$SOURCE_ROOT/}"
        if ! contains_line "$rel" "${shell_core_ffi_raw_symbol_owner_allowlist[@]}"; then
            fail "raw shell-core FFI symbols must stay in the loader owner; found in $rel"
        fi
    done < <(grep -RIl --include='*.swift' -F "alan_shell_core_ffi_" "$SOURCE_ROOT" || true)
}

require_shell_core_action_metadata_query_owners() {
    require_existing_single_owner_pattern \
        "ShellCoreFFIAdapter.shared.actionTitle" \
        "Services/Shell/ShellActionCoordinator.swift" \
        "shell-core action title lookup"

    require_existing_single_owner_pattern \
        "ShellCoreFFIAdapter.shared.actionAvailability" \
        "Services/Shell/ShellActionCoordinator.swift" \
        "shell-core action availability lookup"

    require_existing_single_owner_pattern \
        "ShellCoreFFIAdapter.shared.defaultActionShortcut" \
        "Services/Shell/ShellActionCoordinator.swift" \
        "shell-core action shortcut lookup"

    require_existing_single_owner_pattern \
        "ShellCoreFFIAdapter.shared.keyboardAction" \
        "Services/Shell/ShellActionCoordinator.swift" \
        "shell-core keyboard action lookup"

    require_existing_single_owner_pattern \
        "actions.standard_descriptors" \
        "Services/Shell/ShellCoreFFIActionAdapter.swift" \
        "shell-core action descriptor FFI operation"

    require_existing_single_owner_pattern \
        "actions.default_shortcut" \
        "Services/Shell/ShellCoreFFIActionAdapter.swift" \
        "shell-core default shortcut FFI operation"

    require_existing_single_owner_pattern \
        "actions.keyboard_action" \
        "Services/Shell/ShellCoreFFIActionAdapter.swift" \
        "shell-core keyboard action FFI operation"

    if grep -RIn --include='*.swift' \
        -e "ShellActionAvailabilityResolver\\.availability" \
        -e "ShellActionMetadataCatalog\\.shortcut" \
        -e "ShellActionMetadataCatalog\\.keyboardAction" \
        "$SOURCE_ROOT" >&2
    then
        fail "production shell action metadata must use shell-core FFI instead of Swift metadata fallback"
    fi
}

reject_shell_host_duplicate_terminal_runtime_state() {
    local controller="$SOURCE_ROOT/ShellHostController.swift"
    local controller_dir="$SOURCE_ROOT/Controllers/Shell"
    local registry="$SOURCE_ROOT/TerminalRuntimeRegistry.swift"
    local selection_owner="$controller_dir/ShellHostProjectionAndSelection.swift"

    if grep -En 'var[[:space:]]+terminalRuntime[[:space:]]*:' "$controller" >&2; then
        fail \
            "ShellHostController terminal runtime must derive from TerminalRuntimeRegistry instead of cached state"
    fi
    if grep -RIn --include='*.swift' -E \
        'terminalActiveTasksByPaneID|pendingVisibleBackgroundRuntimeByPaneID|visibleBackgroundRuntimeProjectionScheduled|setSelectedTerminalRuntime|scheduleVisibleBackgroundRuntimeProjection' \
        "$controller" "$controller_dir" >&2
    then
        fail \
            "shell host runtime, active-task, and projection queue state must remain in TerminalRuntimeRegistry"
    fi
    if ! grep -Fq 'private var activeTasksByContentID:' "$registry" \
        || ! grep -Fq 'private var pendingShellProjectionsByContentID:' "$registry"
    then
        fail \
            "TerminalRuntimeRegistry must own content-keyed active-task and shell-projection state"
    fi
    if ! grep -Fq 'var selectedPaneRuntime: TerminalHostRuntimeSnapshot {' "$selection_owner" \
        || ! grep -Fq 'terminalRuntimeRegistry.snapshot(for: paneID)' "$selection_owner"
    then
        fail \
            "selected terminal runtime must be a direct TerminalRuntimeRegistry projection"
    fi

    require_existing_single_owner_pattern \
        "TerminalRuntimePublicationPolicy.shouldProjectToShell" \
        "TerminalRuntimeRegistry.swift" \
        "shell-facing terminal runtime publication policy"
}

reject_shell_host_duplicate_selection_state() {
    local controller="$SOURCE_ROOT/ShellHostController.swift"
    local selection_owner="$SOURCE_ROOT/Controllers/Shell/ShellHostProjectionAndSelection.swift"

    if grep -En '@Published[^[:cntrl:]]*selected(Space|Tab)ID' "$controller" >&2; then
        fail \
            "ShellHostController selection IDs must derive from ShellStateSnapshot instead of duplicate @Published state"
    fi
    if grep -RIn --include='*.swift' -E \
        '^[[:space:]]*(self\.)?selected(Space|Tab)ID[[:space:]]*=' \
        "$controller" "$SOURCE_ROOT/Controllers/Shell" >&2
    then
        fail "shell host selection IDs must not regain independently mutable controller state"
    fi
    if grep -RIn --include='*.swift' -E \
        'func[[:space:]]+synchronizeSelection' \
        "$controller" "$SOURCE_ROOT/Controllers/Shell" >&2
    then
        fail \
            "shell host selection must derive from ShellStateSnapshot without synchronization logic"
    fi
    if ! grep -Fq 'var selectedSpaceID: String? {' "$selection_owner" \
        || ! grep -Fq 'var selectedTabID: String? {' "$selection_owner"
    then
        fail \
            "ShellHostProjectionAndSelection must expose selection IDs as derived snapshot projections"
    fi
}

reject_shell_host_duplicate_persistence_state() {
    local controller="$SOURCE_ROOT/ShellHostController.swift"
    local controller_dir="$SOURCE_ROOT/Controllers/Shell"
    local controller_sources=("$controller" "$controller_dir"/*.swift)
    local persistence_owner="Services/Shell/ShellWorkspacePersistenceCoordinator.swift"
    local scheduler_definition_owner="Services/Shell/ShellWorkspaceManifestStore.swift"
    local projector_definition_owner="Services/Shell/ShellWorkspaceManifestProjector.swift"
    local manifest_state_allowlist=(
        "Services/Shell/ShellCoreFFIManifestAdapter.swift|PruningExpiredTabsPayload|4|let manifest|typed"
        "Services/Shell/ShellCoreFFIManifestAdapter.swift|MaterializeManifestPayload|4|let manifest|typed"
        "Services/Shell/ShellCoreFFIManifestAdapter.swift|ManifestPayload|4|let manifest|typed"
        "Services/Shell/ShellWorkspaceManifestProjector.swift|ShellWorkspaceManifestProjector|8|var manifest|constructed"
        "Services/Shell/ShellWorkspaceManifestStore.swift|ShellWorkspaceManifestStore|8|let manifest|inferred-factory"
        "Services/Shell/ShellWorkspaceManifestStore.swift|ShellWorkspaceManifestStore|12|let manifest|inferred-factory"
        "Services/Shell/ShellWorkspaceManifestStore.swift|ShellWorkspaceManifestLoadResult|4|var manifest|typed"
        "Services/Shell/ShellWorkspacePersistenceCoordinator.swift|ShellWorkspacePersistenceCoordinator|4|private var workspaceManifest|typed"
        "Services/Shell/ShellWorkspacePersistenceStartup.swift|ShellWorkspacePersistenceCoordinator|12|let retainedManifest|inferred-factory"
    )
    local declaration
    local declaration_indent
    local declaration_kind
    local declaration_line
    local declaration_owner
    local factory_inventory
    local file
    local manifest_state
    local -a manifest_state_seen=("<inventory-sentinel>")
    local rel

    require_existing_single_owner_pattern \
        "var workspaceManifest: ShellContentWorkspaceManifest?" \
        "$persistence_owner" \
        "mutable workspace manifest state"
    require_existing_single_owner_pattern \
        "var latestContext: PersistenceContext?" \
        "$persistence_owner" \
        "latest workspace persistence context"
    require_existing_single_owner_pattern \
        "var contentFlushScheduled = false" \
        "$persistence_owner" \
        "workspace persistence scheduled-flush state"
    require_existing_single_owner_pattern \
        "var contentFlushPending = false" \
        "$persistence_owner" \
        "workspace persistence pending-content state"
    require_existing_single_owner_pattern \
        "manifestFlushScheduler.schedule" \
        "$persistence_owner" \
        "workspace persistence debounce scheduling"
    require_existing_single_owner_pattern \
        "manifestProjector.makeManifest(" \
        "$persistence_owner" \
        "workspace manifest projection invocation"

    factory_inventory="$(
        typed_factory_inventory "ShellContentWorkspaceManifest" | sort -u | tr '\n' ';'
    )"
    while IFS= read -r file; do
        rel="${file#$SOURCE_ROOT/}"
        while IFS='|' read -r \
            declaration_line declaration_owner declaration_indent declaration declaration_kind
        do
            manifest_state="$rel|$declaration_owner|$declaration_indent|$declaration|$declaration_kind"
            if ! contains_line "$manifest_state" "${manifest_state_allowlist[@]}"; then
                printf '%s:%s:%s\n' "$file" "$declaration_line" "$declaration" >&2
                fail \
                    "ShellContentWorkspaceManifest storage is not in the accepted ownership inventory: $manifest_state"
            elif contains_line "$manifest_state" "${manifest_state_seen[@]}"; then
                printf '%s:%s:%s\n' "$file" "$declaration_line" "$declaration" >&2
                fail \
                    "ShellContentWorkspaceManifest ownership inventory entries must be unique: $manifest_state"
            else
                manifest_state_seen+=("$manifest_state")
            fi
        done < <(manifest_state_declarations "$file" "$factory_inventory")
    done < <(find "$SOURCE_ROOT" -type f -name '*.swift' -print | sort)

    for manifest_state in "${manifest_state_allowlist[@]}"; do
        if ! contains_line "$manifest_state" "${manifest_state_seen[@]}"; then
            fail \
                "accepted ShellContentWorkspaceManifest inventory entry must remain present: $manifest_state"
        fi
    done

    while IFS= read -r file; do
        if ! swift_code_matching_lines \
            'ManifestFlushScheduling|DebouncedManifestFlushScheduler' \
            "$file" >/dev/null
        then
            continue
        fi
        rel="${file#$SOURCE_ROOT/}"
        case "$rel" in
            "$persistence_owner")
                ;;
            "$scheduler_definition_owner")
                reject_unapproved_symbol_lines \
                    "$file" \
                    'ManifestFlushScheduling|DebouncedManifestFlushScheduler' \
                    '^[[:space:]]*(protocol[[:space:]]+ManifestFlushScheduling([^A-Za-z0-9_]|$)|final[[:space:]]+class[[:space:]]+DebouncedManifestFlushScheduler[[:space:]]*:[[:space:]]*ManifestFlushScheduling([^A-Za-z0-9_]|$))' \
                    "workspace manifest store may define but must not use persistence scheduling"
                ;;
            *)
                fail \
                    "workspace persistence scheduler usage must stay in $persistence_owner; found in $rel"
                ;;
        esac
    done < <(grep -RIl --include='*.swift' -E \
        'ManifestFlushScheduling|DebouncedManifestFlushScheduler' \
        "$SOURCE_ROOT" || true)

    while IFS= read -r file; do
        if ! swift_code_matching_lines \
            'ShellWorkspaceManifestProjector' \
            "$file" >/dev/null
        then
            continue
        fi
        rel="${file#$SOURCE_ROOT/}"
        case "$rel" in
            "$persistence_owner")
                ;;
            "$projector_definition_owner")
                reject_unapproved_symbol_lines \
                    "$file" \
                    'ShellWorkspaceManifestProjector' \
                    '^[[:space:]]*struct[[:space:]]+ShellWorkspaceManifestProjector([^A-Za-z0-9_]|$)' \
                    "workspace manifest projector file may define but must not construct a second projector"
                ;;
            *)
                fail \
                    "workspace manifest projector usage must stay in $persistence_owner; found in $rel"
                ;;
        esac
    done < <(grep -RIl --include='*.swift' -E \
        'ShellWorkspaceManifestProjector' \
        "$SOURCE_ROOT" || true)

    if swift_code_matching_lines \
        '(^|[^A-Za-z0-9_])ShellContentWorkspaceManifest([^A-Za-z0-9_]|$)' \
        "${controller_sources[@]}" >&2
    then
        fail \
            "shell host must not retain or construct workspace manifests outside $persistence_owner"
    fi

    if swift_code_matching_lines \
        '^[[:space:]]*(private[[:space:]]+)?var[[:space:]]+workspaceManifest|ManifestFlushScheduling|DebouncedManifestFlushScheduler|manifestFlushScheduler|contentFlush(Scheduled|Pending)|scheduleContentFlush|flushPendingPersistence|syncManifestFromShellState|makeWorkspaceManifestFromShellState|ShellWorkspaceManifestProjector[[:space:]]*\\(' \
        "${controller_sources[@]}" >&2
    then
        fail \
            "shell host persistence state, projection, and scheduling must remain in ShellWorkspacePersistenceCoordinator"
    fi
}

shell_snapshot_stored_property_counts() {
    local file="$1"
    local factory_inventory="$2"

    awk -v factory_inventory="$factory_inventory" "$OWNERSHIP_AWK_HELPERS"'
        BEGIN {
            factory_count = split(factory_inventory, factory_entries, ";")
            for (factory_index = 1; factory_index <= factory_count; factory_index++) {
                if (factory_entries[factory_index] != "") {
                    snapshot_factories[factory_entries[factory_index]] = 1
                }
            }
        }
        function leading_space_count(value,    prefix) {
            prefix = value
            sub(/[^ ].*$/, "", prefix)
            return length(prefix)
        }
        function is_static_property_start(value) {
            return value ~ /^[[:space:]]*[^\/]*(static|class)[ ]+([^ ]+[ ]+)*(let|var)[ ]+[A-Za-z_][A-Za-z0-9_]*/
        }
        function is_module_property_start(value) {
            return value ~ /^[^\/{]*(let|var)[ ]+[A-Za-z_][A-Za-z0-9_]*/
        }
        function is_type_declaration(value) {
            return value ~ /^[[:space:]]*[^\/]*(class|struct|actor|enum|extension|protocol)[ ]+[A-Za-z_][A-Za-z0-9_.]*/
        }
        function declared_type_name(value,    name) {
            name = value
            sub(/^.*(class|struct|actor|enum|extension|protocol)[ ]+/, "", name)
            sub(/[^A-Za-z0-9_.].*$/, "", name)
            sub(/^.*[.]/, "", name)
            return name
        }
        function snapshot_factory_call_matches(call, owner,    name, parts, qualifier, segment_count, qualifier_index) {
            segment_count = split(call, parts, ".")
            name = parts[segment_count]
            if (name == "ShellStateSnapshot") {
                return 1
            }
            if (segment_count == 1) {
                if (name == "init" && owner == "ShellStateSnapshot") {
                    return 1
                }
                return ((owner "|" name) in snapshot_factories) ||
                    (("|" name) in snapshot_factories)
            }
            for (qualifier_index = 1; qualifier_index < segment_count; qualifier_index++) {
                qualifier = parts[qualifier_index]
                if (qualifier == "Self" || qualifier == "self") {
                    qualifier = owner
                }
                if (name == "init" && qualifier == "ShellStateSnapshot") {
                    return 1
                }
                if ((qualifier "|" name) in snapshot_factories) {
                    return 1
                }
            }
            return parts[1] == "shared" && ((owner "|" name) in snapshot_factories)
        }
        function uses_snapshot_factory(property, owner,    call, expression) {
            if (property !~ /=/) {
                return 0
            }
            expression = property
            sub(/^[^=]*=[ ]*/, "", expression)
            gsub(/[ ]*[.][ ]*/, ".", expression)
            if (instance_receiver_invokes_factory(expression, snapshot_factories)) {
                return 1
            }
            while (match(expression, /[A-Za-z_][A-Za-z0-9_.]*[ ]*\(/)) {
                call = substr(expression, RSTART, RLENGTH)
                sub(/[ ]*\($/, "", call)
                if (snapshot_factory_call_matches(call, owner)) {
                    return 1
                }
                expression = substr(expression, RSTART + RLENGTH)
            }
            return 0
        }
        function inferred_generic_contains_snapshot(property,    expression) {
            if (property !~ /=/) {
                return 0
            }
            expression = property
            sub(/^[^=]*=[ ]*/, "", expression)
            sub(/^try[!?]?[ ]+/, "", expression)
            sub(/^await[ ]+/, "", expression)
            return expression ~ /^[A-Za-z_][A-Za-z0-9_.]*[ ]*<[^>]*ShellStateSnapshot/ ||
                expression ~ /^\[[^]]*ShellStateSnapshot/
        }
        function typed_storage_contains_snapshot(property,    type) {
            if (property !~ /:/) {
                return 0
            }
            if (property !~ /=/ &&
                property ~ /[{]/ &&
                property !~ /[{][ ]*(didSet|willSet)([^A-Za-z0-9_]|$)/)
            {
                return 0
            }
            type = property
            sub(/^[^:]*:[ ]*/, "", type)
            sub(/[=].*$/, "", type)
            return type ~ /ShellStateSnapshot([^A-Za-z0-9_]|$)/ && type !~ /->/
        }
        function snapshot_binding_contains_storage(binding, owner) {
            return typed_storage_contains_snapshot(binding) ||
                binding ~ /=[ ]*ShellStateSnapshot[ ]*([.(]|$)/ ||
                inferred_generic_contains_snapshot(binding) ||
                uses_snapshot_factory(binding, owner)
        }
        function snapshot_storage_count(property, owner,    binding, binding_end, binding_start, count) {
            binding_start = 1
            while (binding_start <= length(property)) {
                binding_end = top_level_binding_end(property, binding_start)
                binding = substr(property, binding_start, binding_end - binding_start)
                if (snapshot_binding_contains_storage(binding, owner)) {
                    count++
                }
                binding_start = binding_end + 1
            }
            return count
        }
        function count_buffered_instance_property(    property) {
            if (instance_buffer == "") {
                return
            }
            property = instance_buffer
            gsub(/[[:space:]]+/, " ", property)
            if (property !~ /(^| )(static|class)[ ]/ &&
                property ~ /var[ ]+[A-Za-z_][A-Za-z0-9_]*/)
            {
                instance_count += snapshot_storage_count(property, instance_owner)
            }
            instance_buffer = ""
        }
        function count_buffered_global_property(    property) {
            if (global_buffer == "") {
                return
            }
            property = global_buffer
            gsub(/[[:space:]]+/, " ", property)
            if (property ~ /(^| )(let|var)[ ]+[A-Za-z_][A-Za-z0-9_]*/)
            {
                global_count += snapshot_storage_count(property, global_owner)
            }
            global_buffer = ""
        }
        {
            first_separator = index($0, "\t")
            remainder = substr($0, first_separator + 1)
            second_separator = index(remainder, "\t")
            line = substr(remainder, second_separator + 1)

            # Follow the formatted type nesting so instance members of both
            # top-level and nested types are counted, while method-local scratch
            # variables remain deeper than their enclosing type member level.
            line_indent = leading_space_count(line)
            if (line !~ /^[[:space:]]*$/) {
                while (type_depth > 0 && line_indent <= type_indent[type_depth]) {
                    delete type_indent[type_depth]
                    delete type_name[type_depth]
                    type_depth--
                }
                if (is_type_declaration(line)) {
                    type_depth++
                    type_indent[type_depth] = line_indent
                    type_name[type_depth] = declared_type_name(line)
                }
            }

            if (type_depth > 0 && line_indent == type_indent[type_depth] + 4) {
                count_buffered_instance_property()
                if (line ~ /(let|var)[ ]+[A-Za-z_][A-Za-z0-9_]*/) {
                    instance_buffer = line
                    instance_indent = line_indent
                    instance_owner = type_name[type_depth]
                }
            } else if (instance_buffer != "" &&
                (line ~ /^[[:space:]]*$/ || line_indent > instance_indent))
            {
                instance_buffer = instance_buffer " " line
            } else {
                count_buffered_instance_property()
            }

            # Globally addressable storage is either a module-scope declaration
            # or a static/class type member. Method-local variables remain out
            # of this inventory.
            if (is_static_property_start(line) ||
                (brace_depth == 0 && is_module_property_start(line)))
            {
                count_buffered_global_property()
                global_buffer = line
                global_indent = line_indent
                global_owner = type_depth > 0 ? type_name[type_depth] : ""
            } else if (global_buffer != "" &&
                (line ~ /^[[:space:]]*$/ || leading_space_count(line) > global_indent))
            {
                global_buffer = global_buffer " " line
            } else {
                count_buffered_global_property()
            }
            brace_depth += brace_delta(line)
        }
        END {
            count_buffered_instance_property()
            count_buffered_global_property()
            print instance_count, global_count
        }
    ' < <(swift_code_lines "$file")
}

reject_replacement_global_shell_store() {
    local controller_owner="ShellHostController.swift"
    local observable_owner_allowlist=(
        "App/AlanMacPrimaryShellOwner.swift|AlanMacPrimaryShellOwner"
        "App/AlanMacUpdateController.swift|AlanMacUpdateController"
        "Models/Shell/ShellSpaceCreationProfileOptions.swift|ShellSpaceCreationProfileOptionStore"
        "Services/AlanOS/AlanOSAttachmentService.swift|AlanOSAttachmentController"
        "ShellHostController.swift|ShellHostController"
        "Support/ShellSidebarSpaceSliderWheelMonitor.swift|ShellSidebarTabListWheelRouter"
        "Support/ShellVoiceCommandController.swift|ShellVoiceCommandController"
        "TerminalRuntimeRegistry.swift|TerminalRuntimeRegistry"
    )
    local published_projection_allowlist=(
        "App/AlanMacUpdateController.swift|decision"
        "Models/Shell/ShellSpaceCreationProfileOptions.swift|options"
        "Services/AlanOS/AlanOSAttachmentService.swift|state"
        "ShellHostController.swift|activityNotifications"
        "ShellHostController.swift|controlPlaneDiagnostics"
        "ShellHostController.swift|isPresentingSpaceCreation"
        "ShellHostController.swift|lastCopiedAt"
        "ShellHostController.swift|shellState"
        "ShellHostController.swift|spaceDraftIcon"
        "ShellHostController.swift|spaceDraftName"
        "ShellHostController.swift|spaceDraftProfileID"
        "ShellHostController.swift|zoomedPaneIDByTabID"
        "Support/ShellVoiceCommandController.swift|isListening"
    )
    local snapshot_static_member_allowlist=(
        "Services/Shell/ShellSocketServer.swift|private static let commandResponseTimeoutSeconds: TimeInterval = 5"
        "Services/Shell/ShellSocketServer.swift|private static let maxConcurrentClients = 4"
        "Services/Shell/ShellSocketServer.swift|private static let maxRequestBytes = 1_048_576"
        "Services/Shell/ShellSocketServer.swift|private static let readTimeoutSeconds = 5"
        "ShellHostController.swift|static let gracefulShutdownPollInterval: TimeInterval = 0.05"
        "ShellHostController.swift|static let iso8601Formatter = ISO8601DateFormatter()"
        "ShellHostController.swift|static let terminalSelectionFirst = ShellPaneMovementInteractionPolicy()"
    )
    local file
    local host_factory_inventory
    local line_number
    local observable_owner
    local published_projection
    local snapshot_property_counts
    local snapshot_factory_inventory
    local snapshot_stored_properties
    local static_member
    local static_member_key
    local global_snapshot_stored_properties
    local source_line
    local rel

    require_existing_single_owner_pattern \
        "var shellState: ShellStateSnapshot" \
        "$controller_owner" \
        "observable shell snapshot state"

    snapshot_factory_inventory="$(
        typed_factory_inventory "ShellStateSnapshot" | sort -u | tr '\n' ';'
    )"
    host_factory_inventory="$(
        typed_factory_inventory "ShellHostController" | sort -u | tr '\n' ';'
    )"

    while IFS= read -r file; do
        rel="${file#$SOURCE_ROOT/}"
        snapshot_property_counts="$(
            shell_snapshot_stored_property_counts "$file" "$snapshot_factory_inventory"
        )"
        snapshot_stored_properties="${snapshot_property_counts%% *}"
        global_snapshot_stored_properties="${snapshot_property_counts##* }"

        case "$rel" in
            "$controller_owner")
                if (( snapshot_stored_properties != 1 )); then
                    fail \
                        "ShellHostController must keep exactly one stored ShellStateSnapshot projection"
                fi
                ;;
            "ShellControlPlane.swift"|"Services/Shell/ShellSocketServer.swift")
                if (( snapshot_stored_properties > 2 )); then
                    fail \
                        "accepted shell transport snapshot caches must not grow in $rel"
                fi
                ;;
            *)
                if (( snapshot_stored_properties > 0 )); then
                    fail \
                        "mutable ShellStateSnapshot storage is not an accepted owner: $rel"
                fi
                ;;
        esac

        if (( global_snapshot_stored_properties > 0 )); then
            fail \
                "ShellStateSnapshot must not have a module/static/class stored owner; found in $rel"
        fi

        if (( snapshot_stored_properties > 0 )); then
            while IFS= read -r static_member; do
                static_member_key="$rel|$static_member"
                if ! contains_line \
                    "$static_member_key" \
                    "${snapshot_static_member_allowlist[@]}"
                then
                    printf '%s:%s\n' "$file" "$static_member" >&2
                    fail \
                        "mutable ShellStateSnapshot owners must not add static/class entry points: $static_member_key"
                fi
            done < <(static_property_declarations "$file")
        fi

        if [[ "$rel" != "$controller_owner" ]] \
            && swift_code_matching_lines \
                'ObservableObject|@Observable' "$file" >/dev/null \
            && normalized_file_matches "$file" \
                '->[ ]*ShellStateSnapshot[?!]?|static[ ]+(var|let)[ ]+[A-Za-z_][A-Za-z0-9_]*[ ]*:[ ]*ShellStateSnapshot[?!]?'
        then
            fail \
                "non-controller observable owners must not manufacture ShellStateSnapshot state; found in $rel"
        fi

    done < <(find "$SOURCE_ROOT" -type f -name '*.swift' -print | sort)

    while IFS= read -r file; do
        rel="${file#$SOURCE_ROOT/}"
        while IFS='|' read -r line_number source_line; do
            [[ -n "$line_number" ]] || continue
            printf '%s:%s:%s\n' "$file" "$line_number" "$source_line" >&2
            fail \
                "ShellHostController must not be retained by module/static/class storage; found in $rel"
        done < <(shell_host_global_storage_declarations "$file" "$host_factory_inventory")
    done < <(find "$SOURCE_ROOT" -type f -name '*.swift' -print | sort)

    while IFS=: read -r file line_number source_line; do
        rel="${file#$SOURCE_ROOT/}"

        if [[ "$source_line" == *"@Observable"* ]]; then
            printf '%s:%s:%s\n' "$file" "$line_number" "$source_line" >&2
            fail "new @Observable owners require an explicit architecture ownership decision"
        elif [[ "$source_line" =~ (class|struct|actor)[[:space:]]+([A-Za-z_][A-Za-z0-9_]*) ]]; then
            observable_owner="$rel|${BASH_REMATCH[2]}"
            if ! contains_line "$observable_owner" "${observable_owner_allowlist[@]}"; then
                printf '%s:%s:%s\n' "$file" "$line_number" "$source_line" >&2
                fail "new ObservableObject owner is not in the accepted architecture: $observable_owner"
            fi
        else
            printf '%s:%s:%s\n' "$file" "$line_number" "$source_line" >&2
            fail "unrecognized ObservableObject ownership declaration in $rel"
        fi
    done < <(
        swift_tree_code_matching_lines \
            'ObservableObject|@Observable' \
            "$SOURCE_ROOT" || true
    )

    while IFS=: read -r file line_number source_line; do
        rel="${file#$SOURCE_ROOT/}"

        if [[ "$source_line" =~ @Published[^[:cntrl:]]*var[[:space:]]+([A-Za-z_][A-Za-z0-9_]*) ]]; then
            published_projection="$rel|${BASH_REMATCH[1]}"
            if ! contains_line "$published_projection" "${published_projection_allowlist[@]}"; then
                printf '%s:%s:%s\n' "$file" "$line_number" "$source_line" >&2
                fail "new @Published projection is not in the accepted architecture: $published_projection"
            fi
        else
            printf '%s:%s:%s\n' "$file" "$line_number" "$source_line" >&2
            fail "unrecognized @Published property declaration in $rel"
        fi
    done < <(swift_tree_code_matching_lines '@Published' "$SOURCE_ROOT" || true)

    if swift_tree_code_matching_lines \
        '(class|struct|actor|enum)[[:space:]]+([A-Za-z_][A-Za-z0-9_]*)?Shell(State|Workspace)?(Store|Model)([^A-Za-z0-9_]|$)' \
        "$SOURCE_ROOT" >&2
    then
        fail \
            "a replacement global Shell store/model must not sit above the accepted shell state owners"
    fi
}

reject_swiftui_shell_hot_path_sync_boundaries() {
    local matched=0
    local pattern
    local search_roots=(
        "$SOURCE_ROOT/MacShellRootView.swift"
        "$SOURCE_ROOT/TerminalPaneView.swift"
        "$SOURCE_ROOT/Views/Shell"
        "$SOURCE_ROOT/Services/Terminal/TerminalHostFocusAndPointerInput.swift"
        "$SOURCE_ROOT/Services/Terminal/TerminalHostInputTracing.swift"
        "$SOURCE_ROOT/Services/Terminal/TerminalHostKeyboardInput.swift"
        "$SOURCE_ROOT/Services/Terminal/TerminalHostTextInput.swift"
        "$SOURCE_ROOT/Services/Terminal/TerminalInputTrace.swift"
        "$SOURCE_ROOT/Services/Terminal/TerminalKeyboardLayout.swift"
    )

    for pattern in \
        "ShellCoreFFIAdapter" \
        "ShellCoreReducerAdapter" \
        "reducerAdapter.apply" \
        "AlanShellLocalCommandExecutor.execute" \
        "actions.execute" \
        "actions.standard_descriptors" \
        "JSONEncoder" \
        "JSONDecoder"
    do
        if grep -RIn --include='*.swift' -F "$pattern" "${search_roots[@]}" >&2; then
            matched=1
        fi
    done

    if [[ "$matched" -ne 0 ]]; then
        fail "SwiftUI shell render/body/context-menu hot paths must not synchronously call shell-core FFI, JSON codecs, local command executors, or reducers"
    fi
}

if [[ -e "$SOURCE_ROOT/Controllers/Shell/ShellHostControlCommandHandling.swift" ]]; then
    fail "the duplicate ShellHostControlCommandHandling owner must stay deleted"
fi

platform_control_handler="$SOURCE_ROOT/Controllers/Shell/ShellHostPlatformControlCommandHandling.swift"
if ! grep -Fq "AlanShellLocalCommandExecutor.execute" "$platform_control_handler"; then
    fail "the shell host control entry must delegate portable commands to AlanShellLocalCommandExecutor"
fi
if grep -Eq 'reducerAdapter\.apply|performShellAutomationCommand' "$platform_control_handler"; then
    fail "the platform control handler must not regain portable mutation execution"
fi

require_rust_reducer_adapter \
    "$SOURCE_ROOT/Services/Shell/ShellLocalCommandExecutor.swift" \
    "state.creatingSpace(" \
    "state.settingTerminalProfile(" \
    "state.openingTerminalTab(" \
    "state.closingTab(" \
    "state.pinningTab(" \
    "state.unpinningTab(" \
    "state.organizingTab(" \
    "state.movingTabToSpace(" \
    "state.splittingPane(" \
    "state.closingPane(" \
    "state.movingPaneToNewTab(" \
    "state.movingPane(" \
    "state.focusingPane(" \
    "state.settingAttention("

require_rust_reducer_adapter \
    "$SOURCE_ROOT/ShellHostController.swift" \
    "shellState.creatingSpace(" \
    "shellState.settingTerminalProfile(" \
    "shellState.settingPresentationIcon(" \
    "shellState.deletingSpace(" \
    "shellState.organizingTab(" \
    "shellState.clearingInactiveTemporaryTabs(" \
    "shellState.closingPane(" \
    "shellState.closingTab(" \
    "shellState.duplicatingTab(" \
    "shellState.resizingSplit(" \
    "shellState.equalizingSplits(" \
    "shellState.focusingPane(" \
    "shellState.movingPane(" \
    "shellState.movingPaneToNewTab(" \
    "shellState.movingPaneWithinTab(" \
    "shellState.openingContentTab(" \
    "shellState.splittingPane("

require_existing_single_owner_pattern \
    'operation: "reducer.apply"' \
    "Services/Shell/ShellCoreFFIReducerAdapter.swift" \
    "shell-core reducer FFI operation"

if grep -RIl --include='*.swift' -F "ShellReducerCommandCoordinator" "$SOURCE_ROOT" \
    >/dev/null; then
    fail "shell-core reducer calls must not retain a shallow pass-through coordinator"
fi

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.executeAction" \
    "Services/Shell/ShellActionCoordinator.swift" \
    "shell-core action execution"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.handleControlCommand" \
    "Services/Shell/ShellLocalCommandExecutor.swift" \
    "shell-core local control command handling"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.defaultContentWorkspaceManifest" \
    "Services/Shell/ShellWorkspaceManifestStore.swift" \
    "shell-core workspace manifest defaulting"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.pruningExpiredTabs" \
    "Services/Shell/ShellWorkspacePersistenceStartup.swift" \
    "shell-core workspace manifest pruning"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.materializeContentWorkspaceManifest" \
    "Services/Shell/ShellWorkspacePersistenceStartup.swift" \
    "shell-core workspace manifest materialization"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.validateTerminalProfileDocument" \
    "Services/Shell/TerminalProfileStore.swift" \
    "shell-core Terminal Profile validation"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.makeTerminalProfileDefinition" \
    "Services/Shell/TerminalProfileStore.swift" \
    "shell-core Terminal Profile editor semantics"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.upsertTerminalProfileDraft" \
    "Services/Shell/TerminalProfileStore.swift" \
    "shell-core Terminal Profile document editor semantics"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.shouldCaptureGlobalDefaultTerminalProfile" \
    "Services/Shell/TerminalProfileStore.swift" \
    "shell-core global default Terminal Profile capture policy"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.resolveTerminalLaunchIntent" \
    "Services/Terminal/TerminalBootResolution.swift" \
    "shell-core Terminal Profile launch intent resolution"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.terminalProfileRows" \
    "Models/Shell/ShellSettingsSurfaceModel.swift" \
    "shell-core Terminal Profile settings row projection"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.managedTerminalAccountRows" \
    "Models/Shell/ShellSettingsSurfaceModel.swift" \
    "shell-core managed terminal account settings row projection"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.capabilityRows" \
    "Models/Shell/ShellSettingsSurfaceModel.swift" \
    "shell-core capability settings row projection"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.localRows" \
    "Models/Shell/ShellSettingsSurfaceModel.swift" \
    "shell-core local settings row projection"

require_existing_single_owner_pattern \
    'operation: "managed_terminal_account.validate_request"' \
    "Services/Shell/ShellCoreFFIManagedTerminalAccountAdapter.swift" \
    "shell-core managed terminal account validation FFI operation"

require_existing_single_owner_pattern \
    'operation: "managed_terminal_account.plan"' \
    "Services/Shell/ShellCoreFFIManagedTerminalAccountAdapter.swift" \
    "shell-core managed terminal account planning FFI operation"

require_existing_single_owner_pattern \
    "ShellCoreManagedTerminalAccountAdapter().managedTerminalAccountPlan" \
    "Services/Shell/ManagedTerminalAccountPlanning.swift" \
    "shell-core managed terminal account provisioning planner"

require_existing_single_owner_pattern \
    "ShellCoreManagedTerminalAccountAdapter().managedTerminalAccountRollbackPlan" \
    "Services/Shell/ManagedTerminalAccountPlanning.swift" \
    "shell-core managed terminal account rollback planner"

require_shell_core_ffi_shared_callsite_owners
require_shell_core_ffi_direct_init_owners
require_shell_core_ffi_raw_symbol_owners
require_shell_core_action_metadata_query_owners
reject_shell_host_duplicate_terminal_runtime_state
reject_shell_host_duplicate_selection_state
reject_shell_host_duplicate_persistence_state
reject_replacement_global_shell_store
reject_swiftui_shell_hot_path_sync_boundaries

printf 'Current Swift inventory:\n'
while IFS= read -r file; do
    rel="${file#$SOURCE_ROOT/}"
    lines="$(wc -l < "$file" | tr -d ' ')"
    imports="$(grep -E '^import ' "$file" | sed 's/^import //' | paste -sd ',' - || true)"
    gates="$(grep -E '^#if (os|canImport)' "$file" | sed 's/^#if //' | paste -sd ',' - || true)"
    if [[ -z "$imports" ]]; then
        imports="-"
    fi
    if [[ -z "$gates" ]]; then
        gates="-"
    fi
    printf '  %-36s %5s lines  imports=%s  gates=%s\n' "$rel" "$lines" "$imports" "$gates"
    check_appkit_import_gate "$rel" "$file"

    if [[ "$rel" != */* ]]; then
        if ! contains_line "$rel" "${current_root_swift_allowlist[@]}"; then
            fail "new root-level Swift file '$rel' should be placed in the target owner folder"
        fi
    fi

    if (( lines > large_file_threshold )); then
        warn "large|$rel|$lines" \
            "$rel is $lines lines; keep new behavior in the target owner or document the temporary boundary"
    fi

    if grep -Eq '^import (AppKit|Darwin)$' "$file"; then
        case "$rel" in
            App/*|Services/*|Support/*|Views/Shell/Terminal/*|AlanApp.swift|AlanAppSingletonGuard.swift|GhosttyLiveHost.swift|ShellControlPlane.swift)
                ;;
            MacShellRootView.swift|ShellHostController.swift|TerminalRuntimeRegistry.swift)
                warn "bridge|$rel|appkit-or-darwin-outside-bridge" \
                    "$rel imports AppKit or Darwin while it remains outside a narrow bridge owner"
                ;;
            *)
                fail "$rel imports AppKit or Darwin outside an accepted app, service, support, or terminal bridge boundary"
                ;;
        esac
    fi

    if ! grep -q "$rel" "$PROJECT_FILE"; then
        fail "$rel is not referenced by the Xcode project"
    fi
done < <(find "$SOURCE_ROOT" -name '*.swift' -type f | sort)

printf '\nTarget layout status:\n'
for dir in "${target_dirs[@]}"; do
    if [[ -d "$SOURCE_ROOT/$dir" ]]; then
        printf '  present: clients/apple/alan-macos/%s\n' "$dir"
    else
        warn "missing-target-folder|$dir" \
            "target folder clients/apple/alan-macos/$dir is not present yet"
    fi
    if [[ -f "$ARCH_DOC" ]] && ! grep -q "\`$dir/\`" "$ARCH_DOC"; then
        fail "clients/apple/ARCHITECTURE.md must document target folder $dir/"
    fi
done

printf '\nREADME layout drift:\n'
while IFS= read -r entry; do
    path="$(printf '%s' "$entry" | sed -E 's/^- `([^`]+)`.*/\1/')"
    [[ "$path" == "$entry" ]] && continue
    case "$path" in
        *.swift)
            [[ -f "$SOURCE_ROOT/$path" ]] || warn "readme-missing-file|$path" \
                "README lists $path but the file is not at clients/apple/alan-macos/$path"
            ;;
        */)
            [[ -d "$SOURCE_ROOT/${path%/}" ]] || warn "readme-missing-folder|$path" \
                "README lists $path but the folder is not present yet"
            ;;
    esac
done < <(grep -E '^- `[^`]+`' "$README_FILE" || true)

if ! grep -q "check-architecture-maintainability.sh" "$README_FILE"; then
    warn "readme-missing-command|check-architecture-maintainability.sh" \
        "README does not mention the architecture maintainability report command"
fi

printf '\nXcode project membership drift:\n'
while IFS= read -r ref; do
    name="$(printf '%s' "$ref" | sed -E 's/.*path = ([^;]+);.*/\1/')"
    [[ "$name" == "$ref" ]] && continue
    [[ "$name" == *.swift ]] || continue
    if [[ ! -f "$SOURCE_ROOT/$name" && ! -f "$HELPER_SOURCE_ROOT/$name" ]]; then
        fail "Xcode project references missing Swift file $name"
    fi
done < <(grep -E 'path = .*\.swift;' "$PROJECT_FILE" || true)

validate_warning_baseline
compare_warning_baseline_with_base

if [[ -f "$ARCH_DOC" ]] && ! grep -Eq "^${warnings} known large-file / bridge-boundary warning" "$ARCH_DOC"; then
    fail "clients/apple/ARCHITECTURE.md must record the current report-mode warning count ($warnings)"
fi

if (( failures > 0 )); then
    printf '\nArchitecture maintainability check failed with %d error(s) and %d warning(s).\n' "$failures" "$warnings" >&2
    exit 1
fi

if (( STRICT == 1 && warnings > 0 )); then
    printf '\nArchitecture maintainability strict check failed with %d warning(s).\n' "$warnings" >&2
    exit 1
fi

printf '\nArchitecture maintainability report completed with %d warning(s).\n' "$warnings"
