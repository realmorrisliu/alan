#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture"/{.github,crates,docs,openspec/changes,openspec/specs,packaging,scripts}
touch "$fixture"/{AGENTS.md,CLAUDE.md,CONTEXT.md,CONTRIBUTING.md,Cargo.toml,README.md,justfile}
cp "$script_dir/check-daemon-era-absence.sh" "$fixture/scripts/"
printf '%s\n' '#!/usr/bin/env bash' \
    'if [[ "$1" == --help ]]; then echo "Alan CLI"; exit 0; fi' \
    'echo "unrecognized subcommand daemon"; exit 1' >"$fixture/alan"
chmod +x "$fixture/alan"

check() {
    bash "$fixture/scripts/check-daemon-era-absence.sh" "$fixture/alan" >"$fixture/result" 2>&1
}

# A fresh checkout has no clients directory. All three scans must still work.
check
for forbidden in ALAN_AGENTD_URL daemon_session AgentSession; do
    printf '%s\n' "$forbidden" >"$fixture/crates/regression.rs"
    if check; then
        printf 'guard accepted forbidden source: %s\n' "$forbidden" >&2
        exit 1
    fi
    grep -q 'crates/regression.rs' "$fixture/result"
done
rm "$fixture/crates/regression.rs"

# Missing roots are errors, not clean scan results, including with a match.
rmdir "$fixture/docs"
for content in clean ALAN_AGENTD_URL; do
    printf '%s\n' "$content" >"$fixture/crates/regression.rs"
    if check; then
        printf 'guard accepted a failed repository search\n' >&2
        exit 1
    fi
    grep -q 'repository search failed with status 2' "$fixture/result"
done
printf 'daemon-era absence regression checks passed\n'
