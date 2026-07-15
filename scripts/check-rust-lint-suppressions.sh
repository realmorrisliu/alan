#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
failed=0

cd "$ROOT"
while IFS= read -r file; do
    if ! awk '
        function finish_attribute() {
            if (attribute ~ /(allow|expect)[[:space:]]*\(/ && attribute !~ /reason[[:space:]]*=/) {
                printf "%s:%d: explicit allow/expect attribute requires reason = \"...\"\n", FILENAME, start
                failed = 1
            }
            collecting = 0
            attribute = ""
        }
        /^[[:space:]]*#!?\[/ {
            collecting = 1
            start = NR
            attribute = $0
            if ($0 ~ /\][[:space:]]*$/) {
                finish_attribute()
            }
            next
        }
        collecting {
            attribute = attribute " " $0
            if ($0 ~ /\][[:space:]]*$/) {
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
