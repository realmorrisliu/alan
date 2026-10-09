#!/usr/bin/env bash

# Cargo resolves config files and both target-dir environment spellings for us.
alan_cli_target_dir() {
    local root="$1"
    if [[ -n "${ALAN_STANDALONE_TARGET_DIR:-}" ]]; then
        python3 -c 'import os, sys; print(os.path.abspath(sys.argv[1]))' "$ALAN_STANDALONE_TARGET_DIR"
    else
        cargo metadata --manifest-path "$root/Cargo.toml" --locked --offline \
            --no-deps --format-version 1 \
            | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
    fi
}

# Use the artifact from this invocation, including configured cross targets.
alan_build_cli() {
    local root="$1" target_dir="$2"
    shift 2
    cargo build --manifest-path "$root/Cargo.toml" --locked -p alan --bin alan \
        --target-dir "$target_dir" --message-format=json-render-diagnostics "$@" \
        | python3 -c '
import json, sys
executables = set()
for line in sys.stdin:
    message = json.loads(line)
    if (message.get("reason") == "compiler-artifact"
            and message.get("target", {}).get("name") == "alan"
            and "bin" in message.get("target", {}).get("kind", [])
            and message.get("executable")):
        executables.add(message["executable"])
if len(executables) != 1:
    sys.exit("error: Cargo did not report exactly one Alan executable")
print(executables.pop())'
}
