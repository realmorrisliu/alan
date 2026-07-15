#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
failed=0

cd "$ROOT"
while IFS= read -r file; do
    if ! awk '
        function code_without_comments_or_strings(line,    out, i, current, following) {
            out = ""
            for (i = 1; i <= length(line); i++) {
                current = substr(line, i, 1)
                following = substr(line, i + 1, 1)

                if (block_comment_depth > 0) {
                    if (current == "/" && following == "*") {
                        block_comment_depth++
                        i++
                    } else if (current == "*" && following == "/") {
                        block_comment_depth--
                        i++
                    }
                    continue
                }

                if (in_string) {
                    if (escaped) {
                        escaped = 0
                    } else if (current == "\\") {
                        escaped = 1
                    } else if (current == "\"") {
                        in_string = 0
                        out = out current
                    }
                    continue
                }

                if (current == "\"") {
                    in_string = 1
                    out = out current
                } else if (current == "/" && following == "/") {
                    break
                } else if (current == "/" && following == "*") {
                    block_comment_depth++
                    out = out " "
                    i++
                } else {
                    out = out current
                }
            }
            return out
        }
        function finish_attribute() {
            if (attribute ~ /(allow|expect)[[:space:]]*\(/ && attribute !~ /reason[[:space:]]*=/) {
                printf "%s:%d: explicit allow/expect attribute requires reason = \"...\"\n", FILENAME, start
                failed = 1
            }
            collecting = 0
            attribute = ""
        }
        /^[[:space:]]*#[[:space:]]*!?[[:space:]]*\[/ {
            collecting = 1
            start = NR
            block_comment_depth = 0
            in_string = 0
            escaped = 0
            code = code_without_comments_or_strings($0)
            attribute = code
            if (code ~ /\]/) {
                finish_attribute()
            }
            next
        }
        collecting {
            code = code_without_comments_or_strings($0)
            attribute = attribute " " code
            if (code ~ /\]/) {
                finish_attribute()
            }
        }
        END { exit failed }
    ' "$file"; then
        failed=1
    fi
done < <(rg --files crates -g '*.rs' | sort)

if [[ "$failed" -ne 0 ]]; then
    exit 1
fi

printf 'Rust lint-suppression reasons passed.\n'
