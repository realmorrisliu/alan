#!/usr/bin/env bash
# Sign and notarize a standalone macOS Mach-O executable (e.g. ./alan).
# Never runs xcrun stapler staple (bare Mach-O executables cannot be stapled).
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: scripts/sign-notarize-macos.sh --binary <path> [options]

Options:
  --binary <path>           Path to standalone Mach-O binary (env: ALAN_SIGN_BINARY)
  --identity <identity>     Signing identity, or "-" with --skip-notarize (env: ALAN_SIGN_IDENTITY)
  --keychain <path>         Optional keychain path for codesign / notarytool profile (env: ALAN_SIGN_KEYCHAIN)
  --notary-key <path>       App Store Connect API .p8 key file (env: ALAN_NOTARY_KEY)
  --notary-key-id <id>      App Store Connect API Key ID (env: ALAN_NOTARY_KEY_ID, ASC_API_KEY_ID)
  --notary-issuer <uuid>    App Store Connect Issuer ID (env: ALAN_NOTARY_ISSUER, ASC_API_ISSUER_ID)
  --notary-profile <name>   notarytool keychain profile name (env: ALAN_NOTARY_PROFILE)
  --identifier <id>         Optional codesign identifier override (env: ALAN_SIGN_IDENTIFIER)
  --proof-out <path>        Write signing/notarization proof text file (env: ALAN_SIGN_PROOF_OUT)
  --notary-json-out <path>  Write notarytool JSON response file (env: ALAN_NOTARY_JSON_OUT)
  --skip-notarize           Sign and verify only; skip zip + notarytool submission (env: ALAN_SKIP_NOTARIZE=1)
  --dry-run                 Validate arguments and print commands without signing or submitting
  -h, --help                Show this help message
EOF
}

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

compute_sha256() {
    local target="$1"
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$target" | awk '{print $1}'
    else
        sha256sum "$target" | awk '{print $1}'
    fi
}

binary="${ALAN_SIGN_BINARY:-}"
identity="${ALAN_SIGN_IDENTITY:-}"
keychain="${ALAN_SIGN_KEYCHAIN:-}"
notary_key="${ALAN_NOTARY_KEY:-}"
notary_key_id="${ALAN_NOTARY_KEY_ID:-${ASC_API_KEY_ID:-}}"
notary_issuer="${ALAN_NOTARY_ISSUER:-${ASC_API_ISSUER_ID:-}}"
notary_profile="${ALAN_NOTARY_PROFILE:-}"
identifier="${ALAN_SIGN_IDENTIFIER:-}"
proof_out="${ALAN_SIGN_PROOF_OUT:-}"
notary_json_out="${ALAN_NOTARY_JSON_OUT:-}"
skip_notarize=0
if [[ "${ALAN_SKIP_NOTARIZE:-0}" == "1" ]]; then
    skip_notarize=1
fi
dry_run=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --binary)
            [[ $# -ge 2 ]] || fail "--binary requires a value"
            binary="$2"
            shift 2
            ;;
        --identity)
            [[ $# -ge 2 ]] || fail "--identity requires a value"
            identity="$2"
            shift 2
            ;;
        --keychain)
            [[ $# -ge 2 ]] || fail "--keychain requires a value"
            keychain="$2"
            shift 2
            ;;
        --notary-key)
            [[ $# -ge 2 ]] || fail "--notary-key requires a value"
            notary_key="$2"
            shift 2
            ;;
        --notary-key-id)
            [[ $# -ge 2 ]] || fail "--notary-key-id requires a value"
            notary_key_id="$2"
            shift 2
            ;;
        --notary-issuer)
            [[ $# -ge 2 ]] || fail "--notary-issuer requires a value"
            notary_issuer="$2"
            shift 2
            ;;
        --notary-profile)
            [[ $# -ge 2 ]] || fail "--notary-profile requires a value"
            notary_profile="$2"
            shift 2
            ;;
        --identifier)
            [[ $# -ge 2 ]] || fail "--identifier requires a value"
            identifier="$2"
            shift 2
            ;;
        --proof-out)
            [[ $# -ge 2 ]] || fail "--proof-out requires a value"
            proof_out="$2"
            shift 2
            ;;
        --notary-json-out)
            [[ $# -ge 2 ]] || fail "--notary-json-out requires a value"
            notary_json_out="$2"
            shift 2
            ;;
        --skip-notarize)
            skip_notarize=1
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
        -*)
            fail "unknown option: $1"
            ;;
        *)
            if [[ -z "$binary" ]]; then
                binary="$1"
                shift
            else
                fail "unexpected positional argument: $1"
            fi
            ;;
    esac
done

[[ -n "$binary" ]] || fail "--binary is required"
[[ -n "$identity" ]] || fail "--identity (or ALAN_SIGN_IDENTITY) is required"

if [[ "$identity" == "-" && "$skip_notarize" -ne 1 ]]; then
    fail "ad-hoc identity '-' cannot be notarized; pass --skip-notarize or a Developer ID Application identity"
fi

notary_mode=""
if [[ "$skip_notarize" -ne 1 ]]; then
    if [[ -n "$notary_profile" ]]; then
        notary_mode="profile"
    elif [[ -n "$notary_key" || -n "$notary_key_id" || -n "$notary_issuer" ]]; then
        [[ -n "$notary_key" ]] || fail "--notary-key is required with API key notarization"
        [[ -n "$notary_key_id" ]] || fail "--notary-key-id is required with --notary-key"
        [[ -n "$notary_issuer" ]] || fail "--notary-issuer is required with --notary-key"
        notary_mode="key"
    else
        fail "notarization requires --notary-profile or (--notary-key, --notary-key-id, --notary-issuer), or pass --skip-notarize"
    fi
fi

if [[ "$dry_run" -eq 1 ]]; then
    if [[ "$identity" == "-" ]]; then
        printf '[dry-run] codesign --force --sign - %q\n' "$binary"
    else
        printf '[dry-run] codesign --force --sign %q --options runtime --timestamp' "$identity"
        if [[ -n "$keychain" ]]; then
            printf ' --keychain %q' "$keychain"
        fi
        if [[ -n "$identifier" ]]; then
            printf ' --identifier %q' "$identifier"
        fi
        printf ' %q\n' "$binary"
    fi
    printf '[dry-run] codesign --verify --strict --verbose=2 %q\n' "$binary"
    printf '[dry-run] codesign -dv --verbose=4 %q\n' "$binary"
    if [[ "$skip_notarize" -ne 1 ]]; then
        zip_preview="<temp>/$(basename "$binary").zip"
        printf '[dry-run] ditto -c -k --keepParent %q %q\n' "$binary" "$zip_preview"
        if [[ "$notary_mode" == "profile" ]]; then
            printf '[dry-run] xcrun notarytool submit %q --keychain-profile %q' "$zip_preview" "$notary_profile"
            if [[ -n "$keychain" ]]; then
                printf ' --keychain %q' "$keychain"
            fi
            printf ' --wait --output-format json\n'
        else
            printf '[dry-run] xcrun notarytool submit %q --key %q --key-id %q --issuer %q --wait --output-format json\n' \
                "$zip_preview" "$notary_key" "$notary_key_id" "$notary_issuer"
        fi
    else
        printf '[dry-run] skipping notarization (--skip-notarize)\n'
    fi
    if [[ -n "$proof_out" ]]; then
        printf '[dry-run] would write signing proof to %q\n' "$proof_out"
    fi
    if [[ -n "$notary_json_out" && "$skip_notarize" -ne 1 ]]; then
        printf '[dry-run] would write notary JSON to %q\n' "$notary_json_out"
    fi
    exit 0
fi

[[ "$(uname -s)" == "Darwin" ]] || fail "macOS signing/notarization requires Darwin (got $(uname -s))"
[[ -f "$binary" && ! -L "$binary" ]] || fail "binary must be an existing regular file: $binary"
if [[ "$notary_mode" == "key" && ! -f "$notary_key" ]]; then
    fail "notary key file not found: $notary_key"
fi
if [[ -n "$keychain" && ! -e "$keychain" ]]; then
    fail "keychain not found: $keychain"
fi

work_dir="$(mktemp -d)"
cleanup() {
    rm -rf "$work_dir"
}
trap cleanup EXIT

sign_binary() {
    if [[ "$identity" == "-" ]]; then
        codesign --force --sign - "$binary"
        return 0
    fi

    local -a sign_args=(--force --sign "$identity" --options runtime --timestamp)
    if [[ -n "$keychain" ]]; then
        sign_args+=(--keychain "$keychain")
    fi
    if [[ -n "$identifier" ]]; then
        sign_args+=(--identifier "$identifier")
    fi

    # Apple's timestamp service occasionally fails transiently; retry only timestamp failures.
    local attempt=1
    while true; do
        local err_file
        err_file="$work_dir/codesign-$attempt.err"
        if codesign "${sign_args[@]}" "$binary" 2>"$err_file"; then
            rm -f "$err_file"
            return 0
        fi
        if [[ "$attempt" -ge 3 ]] || ! grep -qi "timestamp" "$err_file"; then
            cat "$err_file" >&2
            rm -f "$err_file"
            return 1
        fi
        printf 'codesign timestamp failed for %s, retry %d\n' "$binary" "$attempt" >&2
        rm -f "$err_file"
        attempt=$((attempt + 1))
        sleep $((attempt * 2))
    done
}

sign_binary

verify_out="$(codesign --verify --strict --verbose=2 "$binary" 2>&1)"
printf '%s\n' "$verify_out"
details_out="$(codesign -dv --verbose=4 "$binary" 2>&1)"

is_signed=true
if [[ "$identity" == "-" ]]; then
    is_signed=false
fi

is_notarized=false
notary_id=""
notary_status=""

if [[ "$skip_notarize" -ne 1 ]]; then
    zip_path="$work_dir/$(basename "$binary").zip"
    ditto -c -k --keepParent "$binary" "$zip_path"

    notary_raw="$work_dir/notary.raw"
    notary_err="$work_dir/notary.err"
    notary_clean="$work_dir/notary.json"

    declare -a notary_cmd=(xcrun notarytool submit "$zip_path")
    if [[ "$notary_mode" == "profile" ]]; then
        notary_cmd+=(--keychain-profile "$notary_profile")
        if [[ -n "$keychain" ]]; then
            notary_cmd+=(--keychain "$keychain")
        fi
    else
        notary_cmd+=(--key "$notary_key" --key-id "$notary_key_id" --issuer "$notary_issuer")
    fi
    notary_cmd+=(--wait --output-format json)

    set +e
    "${notary_cmd[@]}" >"$notary_raw" 2>"$notary_err"
    submit_rc=$?
    set -e

    rm -f "$zip_path"

    if [[ -s "$notary_err" ]] && grep -qi "agreement" "$notary_err"; then
        printf 'Apple notarization refused: the Developer Program agreement is missing or expired.\n' >&2
        printf 'Sign the current agreement at https://developer.apple.com/account and rerun.\n' >&2
    fi

    parsed_info=""
    set +e
    parsed_info="$(python3 - "$notary_raw" "$notary_clean" <<'PY'
import json
import pathlib
import sys

raw_path = pathlib.Path(sys.argv[1])
clean_path = pathlib.Path(sys.argv[2])
raw = raw_path.read_text(encoding="utf-8", errors="replace") if raw_path.exists() else ""
start = raw.find("{")
if start < 0:
    sys.exit(2)
data = json.loads(raw[start:])
clean_path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
print(data.get("id", ""))
print(data.get("status", ""))
PY
)"
    parse_rc=$?
    set -e

    if [[ "$parse_rc" -eq 0 && -f "$notary_clean" && -n "$notary_json_out" ]]; then
        mkdir -p "$(dirname "$notary_json_out")"
        cp "$notary_clean" "$notary_json_out"
    fi

    if [[ "$parse_rc" -eq 0 ]]; then
        notary_id="$(printf '%s\n' "$parsed_info" | sed -n '1p')"
        notary_status="$(printf '%s\n' "$parsed_info" | sed -n '2p')"
    fi

    if [[ "$submit_rc" -ne 0 || "$parse_rc" -ne 0 || "$notary_status" != "Accepted" ]]; then
        if [[ -s "$notary_err" ]]; then
            grep -v -e "BEGIN " -e "PRIVATE KEY" "$notary_err" >&2 || true
        fi
        fail "notarization failed (exit=${submit_rc}, id=${notary_id:-unknown}, status=${notary_status:-unknown})"
    fi

    is_notarized=true
    printf 'Notarization accepted (id=%s)\n' "$notary_id"
fi

if [[ -n "$proof_out" ]]; then
    mkdir -p "$(dirname "$proof_out")"
    binary_sha256="$(compute_sha256 "$binary")"
    {
        printf 'binary=%s\n' "$binary"
        printf 'sha256=%s\n' "$binary_sha256"
        printf 'identity=%s\n' "$identity"
        printf 'signed=%s\n' "$is_signed"
        printf 'notarized=%s\n' "$is_notarized"
        if [[ -n "$notary_id" ]]; then
            printf 'notary_id=%s\n' "$notary_id"
            printf 'notary_status=%s\n' "$notary_status"
        fi
        printf 'codesign_verify:\n%s\n' "$verify_out"
        printf 'codesign_details:\n%s\n' "$details_out"
    } >"$proof_out"
fi
