#!/usr/bin/env bash
# Idempotently configure the 6 Apple Developer ID & notarization secrets on GitHub Actions.
set -euo pipefail

DEFAULT_REPO="realmorrisliu/alan"
REQUIRED_SECRETS=(
    MACOS_CERT_P12_BASE64
    MACOS_CERT_P12_PASSWORD
    APPLE_TEAM_ID
    ASC_API_KEY_P8
    ASC_API_KEY_ID
    ASC_API_ISSUER_ID
)

usage() {
    cat <<'EOF'
Usage: scripts/setup-release-secrets.sh [options]

Idempotently sets the 6 Apple Developer ID & App Store Connect notarization
secrets required by .github/workflows/release.yml using `gh secret set`:
  - MACOS_CERT_P12_BASE64
  - MACOS_CERT_P12_PASSWORD
  - APPLE_TEAM_ID
  - ASC_API_KEY_P8
  - ASC_API_KEY_ID
  - ASC_API_ISSUER_ID

Note: GitHub does not permit reading secret values back from another repository
(such as realmorrisliu/slowpage). Reuse your local Developer ID .p12 and
App Store Connect AuthKey_*.p8 files or environment variables with this helper.

Options:
  --repo <owner/repo>       Target GitHub repository (default: realmorrisliu/alan)
  --p12 <path>              Path to Developer ID Application .p12 certificate
  --p12-password <pass>     Password for the .p12 file (prefer MACOS_CERT_P12_PASSWORD env or prompt)
  --team-id <id>            10-char Apple Team ID (env: APPLE_TEAM_ID; auto-detected from .p12 if possible)
  --p8 <path>               Path to App Store Connect API AuthKey_<ID>.p8 PEM file
  --key-id <id>             App Store Connect Key ID (env: ASC_API_KEY_ID; auto-detected from AuthKey_<ID>.p8)
  --issuer-id <uuid>        App Store Connect Issuer UUID (env: ASC_API_ISSUER_ID)
  --from-env                Also accept MACOS_CERT_P12_BASE64 and ASC_API_KEY_P8 from env
  --partial                 Allow setting only the secrets currently provided instead of all 6
  --check                   Read-only: inspect which secrets are already defined via `gh secret list`
  --dry-run                 Validate inputs and print planned `gh secret set` commands without calling `gh`
  -h, --help                Show this help message

Examples:
  # Dry-run plan preview (no GitHub calls):
  scripts/setup-release-secrets.sh --dry-run

  # Upload from local Slowpage/Apple Developer certificate and API key files:
  scripts/setup-release-secrets.sh \
    --repo realmorrisliu/alan \
    --p12 ~/Certificates/DeveloperID.p12 \
    --p8 ~/Certificates/AuthKey_ABC123DEFG.p8 \
    --issuer-id 00000000-0000-0000-0000-000000000000
EOF
}

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

repo="$DEFAULT_REPO"
p12_path=""
p12_password="${MACOS_CERT_P12_PASSWORD:-}"
p12_password_set=0
if [[ -n "${MACOS_CERT_P12_PASSWORD:-}" ]]; then
    p12_password_set=1
fi
team_id="${APPLE_TEAM_ID:-}"
p8_path=""
key_id="${ASC_API_KEY_ID:-}"
issuer_id="${ASC_API_ISSUER_ID:-}"
from_env=0
partial=0
check_only=0
dry_run=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --repo)
            [[ $# -ge 2 ]] || fail "--repo requires a value"
            repo="$2"
            shift 2
            ;;
        --p12)
            [[ $# -ge 2 ]] || fail "--p12 requires a value"
            p12_path="$2"
            shift 2
            ;;
        --p12-password)
            [[ $# -ge 2 ]] || fail "--p12-password requires a value"
            p12_password="$2"
            p12_password_set=1
            shift 2
            ;;
        --team-id)
            [[ $# -ge 2 ]] || fail "--team-id requires a value"
            team_id="$2"
            shift 2
            ;;
        --p8)
            [[ $# -ge 2 ]] || fail "--p8 requires a value"
            p8_path="$2"
            shift 2
            ;;
        --key-id)
            [[ $# -ge 2 ]] || fail "--key-id requires a value"
            key_id="$2"
            shift 2
            ;;
        --issuer-id)
            [[ $# -ge 2 ]] || fail "--issuer-id requires a value"
            issuer_id="$2"
            shift 2
            ;;
        --from-env)
            from_env=1
            shift
            ;;
        --partial)
            partial=1
            shift
            ;;
        --check)
            check_only=1
            shift
            ;;
        --dry-run)
            dry_run=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "unknown option: $1"
            ;;
    esac
done

[[ "$repo" =~ ^[^/[:space:]]+/[^/[:space:]]+$ ]] || fail "--repo must be in owner/repo format (got: $repo)"

if [[ "$check_only" -eq 1 ]]; then
    if [[ "$dry_run" -eq 1 ]]; then
        printf '[dry-run] gh secret list --repo %q\n' "$repo"
        exit 0
    fi
    command -v gh >/dev/null 2>&1 || fail "GitHub CLI (gh) is required"
    existing="$(gh secret list --repo "$repo" | awk '{print $1}')"
    printf 'Secret status for %s:\n' "$repo"
    missing=0
    for name in "${REQUIRED_SECRETS[@]}"; do
        if printf '%s\n' "$existing" | grep -Fxq "$name"; then
            printf '  [ok]      %s\n' "$name"
        else
            printf '  [missing] %s\n' "$name"
            missing=1
        fi
    done
    exit "$missing"
fi

p12_base64=""
if [[ -n "$p12_path" ]]; then
    [[ -f "$p12_path" ]] || fail ".p12 file not found: $p12_path"
    magic="$(python3 - "$p12_path" <<'PY'
import pathlib, sys
data = pathlib.Path(sys.argv[1]).read_bytes()
print(f"{data[0]:02x}" if data else "")
PY
)"
    [[ "$magic" == "30" ]] || fail "$p12_path does not appear to be a valid PKCS#12 file (magic=$magic, expected 30)"
    p12_base64="$(base64 < "$p12_path" | tr -d '[:space:]')"
elif [[ "$from_env" -eq 1 && -n "${MACOS_CERT_P12_BASE64:-}" ]]; then
    p12_base64="$(printf '%s' "$MACOS_CERT_P12_BASE64" | tr -d '[:space:]')"
    python3 - "$p12_base64" <<'PY'
import base64, sys
try:
    raw = base64.b64decode(sys.argv[1], validate=True)
except Exception as exc:
    sys.exit(f"MACOS_CERT_P12_BASE64 is not valid base64: {exc}")
if not raw or raw[0] != 0x30:
    sys.exit("MACOS_CERT_P12_BASE64 did not decode to a PKCS#12 certificate")
PY
fi

if [[ -n "$p12_base64" && "$p12_password_set" -ne 1 && "$dry_run" -ne 1 && -t 0 ]]; then
    printf 'Enter password for %s: ' "${p12_path:-MACOS_CERT_P12_BASE64}" >&2
    IFS= read -rs p12_password
    printf '\n' >&2
    p12_password_set=1
fi

if [[ -z "$team_id" && -n "$p12_path" && "$p12_password_set" -eq 1 ]] && command -v openssl >/dev/null 2>&1; then
    detected_team="$(
        P12_PASS="$p12_password" openssl pkcs12 -in "$p12_path" -clcerts -nokeys -passin env:P12_PASS 2>/dev/null \
            | sed -n 's/.*(\([A-Z0-9]\{10\}\)).*/\1/p' | head -n 1 || true
    )"
    if [[ -n "$detected_team" ]]; then
        team_id="$detected_team"
    fi
fi

p8_content=""
if [[ -n "$p8_path" ]]; then
    [[ -f "$p8_path" ]] || fail ".p8 file not found: $p8_path"
    p8_content="$(cat "$p8_path")"
    if ! printf '%s' "$p8_content" | grep -q "BEGIN " || ! printf '%s' "$p8_content" | grep -q "PRIVATE KEY"; then
        fail "$p8_path is not a PEM private key"
    fi
    if [[ -z "$key_id" ]]; then
        base_p8="$(basename "$p8_path")"
        if [[ "$base_p8" =~ ^AuthKey_([A-Za-z0-9]+)\.p8$ ]]; then
            key_id="${BASH_REMATCH[1]}"
        fi
    fi
elif [[ "$from_env" -eq 1 && -n "${ASC_API_KEY_P8:-}" ]]; then
    p8_content="$ASC_API_KEY_P8"
    if ! printf '%s' "$p8_content" | grep -q "PRIVATE KEY"; then
        python3 - "$p8_content" <<'PY'
import base64, sys
try:
    decoded = base64.b64decode("".join(sys.argv[1].split()), validate=True).decode("utf-8", errors="replace")
except Exception as exc:
    sys.exit(f"ASC_API_KEY_P8 is neither PEM nor base64 PEM: {exc}")
if "PRIVATE KEY" not in decoded:
    sys.exit("ASC_API_KEY_P8 is not a PEM private key")
PY
    fi
fi

secret_value() {
    case "$1" in
        MACOS_CERT_P12_BASE64) printf '%s' "$p12_base64" ;;
        MACOS_CERT_P12_PASSWORD) printf '%s' "$p12_password" ;;
        APPLE_TEAM_ID) printf '%s' "$team_id" ;;
        ASC_API_KEY_P8) printf '%s' "$p8_content" ;;
        ASC_API_KEY_ID) printf '%s' "$key_id" ;;
        ASC_API_ISSUER_ID) printf '%s' "$issuer_id" ;;
    esac
}

secret_is_set() {
    case "$1" in
        MACOS_CERT_P12_PASSWORD) [[ "$p12_password_set" -eq 1 ]] ;;
        *) [[ -n "$(secret_value "$1")" ]] ;;
    esac
}

missing_names=()
present_names=()
for name in "${REQUIRED_SECRETS[@]}"; do
    if secret_is_set "$name"; then
        present_names+=("$name")
    else
        missing_names+=("$name")
    fi
done

if [[ "$dry_run" -eq 1 ]]; then
    printf '[dry-run] Target repository: %s\n' "$repo"
    for name in "${REQUIRED_SECRETS[@]}"; do
        if secret_is_set "$name"; then
            val="$(secret_value "$name")"
            printf '[dry-run] %s: [provided, %d bytes] -> gh secret set %s --repo %q (via stdin)\n' \
                "$name" "${#val}" "$name" "$repo"
        else
            printf '[dry-run] %s: [not provided in flags/env] -> gh secret set %s --repo %q (via stdin)\n' \
                "$name" "$name" "$repo"
        fi
    done
    exit 0
fi

if [[ "$partial" -eq 0 && "${#missing_names[@]}" -gt 0 ]]; then
    fail "missing required secret input(s): ${missing_names[*]}. Pass flags/env (or --partial to update a subset)."
fi
if [[ "${#present_names[@]}" -eq 0 ]]; then
    fail "no secret inputs provided; pass --p12/--p8/etc. or --help"
fi

command -v gh >/dev/null 2>&1 || fail "GitHub CLI (gh) is required"
gh auth status >/dev/null 2>&1 || fail "GitHub CLI is not authenticated (run: gh auth login)"

for name in "${present_names[@]}"; do
    val="$(secret_value "$name")"
    printf '%s' "$val" | gh secret set "$name" --repo "$repo"
    printf 'Updated secret %s on %s\n' "$name" "$repo"
done
