#!/bin/sh
set -eu

# Alan standalone CLI installer (POSIX sh).
# Fetches release metadata (latest.json), verifies SHA-256 before modifying
# anything on disk, enforces .alan-cli-manifest ownership & .alan-cli-install.lock,
# and installs the standalone `alan` executable.

DEFAULT_MANIFEST_URL="https://alanworks.app/latest.json"
DEFAULT_BETA_MANIFEST_URL="https://alanworks.app/beta.json"
DEFAULT_RELEASE_BASE_URL="https://github.com/realmorrisliu/alan/releases/download"

CHANNEL="${ALAN_CHANNEL:-stable}"

MANIFEST_URL="${ALAN_MANIFEST_URL:-${ALAN_RELEASE_MANIFEST_URL:-$DEFAULT_MANIFEST_URL}}"
MANIFEST_URL_EXPLICIT=0
if [ -n "${ALAN_MANIFEST_URL:-}" ] || [ -n "${ALAN_RELEASE_MANIFEST_URL:-}" ]; then
    MANIFEST_URL_EXPLICIT=1
fi

RELEASE_BASE_URL="${ALAN_RELEASE_BASE_URL:-$DEFAULT_RELEASE_BASE_URL}"
RELEASE_BASE_URL_EXPLICIT=0
if [ -n "${ALAN_RELEASE_BASE_URL:-}" ]; then
    RELEASE_BASE_URL_EXPLICIT=1
fi

REQUESTED_VERSION="${ALAN_VERSION:-}"
INSTALL_DIR="${ALAN_INSTALL_DIR:-}"
FORCE=0
if [ "${ALAN_INSTALL_FORCE:-0}" = "1" ]; then
    FORCE=1
fi
DRY_RUN=0
NO_MODIFY_PATH=0
if [ "${ALAN_NO_MODIFY_PATH:-0}" = "1" ]; then
    NO_MODIFY_PATH=1
fi

STAGE_DIR=""
INSTALL_LOCK=""
LOCK_ACQUIRED=0
KEEP_STAGE=0
NEW_DIGEST=""

info() {
    printf '%s\n' "$*"
}

warn() {
    printf 'warning: %s\n' "$*" >&2
}

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Usage: install.sh [OPTIONS]

Install the standalone Alan CLI (`alan`) from verified release artifacts.

Options:
  --version <VER_OR_TAG>   Install a specific version (e.g. 0.1.0 or v0.1.0)
  --channel <CHANNEL>      Select release channel: stable or beta (default: stable)
  --install-dir <DIR>      Target directory (default: ~/.local/bin)
  --manifest-url <URL>     Override release manifest URL
  --force                  Overwrite an unowned or modified existing `alan` binary
  --dry-run                Validate target and manifest without changing the filesystem
  --no-modify-path         Do not add the install directory to shell startup files
  -h, --help               Show this help message

Environment variables:
  ALAN_VERSION             Same as --version
  ALAN_CHANNEL             Same as --channel (stable or beta; beta uses https://alanworks.app/beta.json)
  ALAN_INSTALL_DIR         Same as --install-dir
  ALAN_MANIFEST_URL        Same as --manifest-url (default: https://alanworks.app/latest.json)
  ALAN_RELEASE_BASE_URL    Release download base URL for --version lookups
  ALAN_INSTALL_FORCE=1     Same as --force
  ALAN_NO_MODIFY_PATH=1    Same as --no-modify-path
  ALAN_TARGET              Explicit Rust target triple override
EOF
}

validate_channel() {
    case "$1" in
        stable|beta) ;;
        *) fail "unknown release channel '$1' (expected 'stable' or 'beta')" ;;
    esac
}

parse_args() {
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --version)
                [ "$#" -ge 2 ] || fail "--version requires a value"
                REQUESTED_VERSION="$2"
                shift 2
                ;;
            --version=*)
                REQUESTED_VERSION="${1#--version=}"
                [ -n "$REQUESTED_VERSION" ] || fail "--version requires a non-empty value"
                shift
                ;;
            --channel)
                [ "$#" -ge 2 ] || fail "--channel requires a value"
                CHANNEL="$2"
                shift 2
                ;;
            --channel=*)
                CHANNEL="${1#--channel=}"
                [ -n "$CHANNEL" ] || fail "--channel requires a non-empty value"
                shift
                ;;
            --install-dir)
                [ "$#" -ge 2 ] || fail "--install-dir requires a value"
                INSTALL_DIR="$2"
                shift 2
                ;;
            --install-dir=*)
                INSTALL_DIR="${1#--install-dir=}"
                [ -n "$INSTALL_DIR" ] || fail "--install-dir requires a non-empty value"
                shift
                ;;
            --manifest-url)
                [ "$#" -ge 2 ] || fail "--manifest-url requires a value"
                MANIFEST_URL="$2"
                MANIFEST_URL_EXPLICIT=1
                shift 2
                ;;
            --manifest-url=*)
                MANIFEST_URL="${1#--manifest-url=}"
                [ -n "$MANIFEST_URL" ] || fail "--manifest-url requires a non-empty value"
                MANIFEST_URL_EXPLICIT=1
                shift
                ;;
            --force)
                FORCE=1
                shift
                ;;
            --dry-run)
                DRY_RUN=1
                shift
                ;;
            --no-modify-path)
                NO_MODIFY_PATH=1
                shift
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                fail "unknown option: $1 (run with --help for usage)"
                ;;
        esac
    done

    validate_channel "$CHANNEL"
    if [ "$MANIFEST_URL_EXPLICIT" = "0" ]; then
        if [ "$CHANNEL" = "beta" ]; then
            MANIFEST_URL="$DEFAULT_BETA_MANIFEST_URL"
        else
            MANIFEST_URL="$DEFAULT_MANIFEST_URL"
        fi
    fi

    if [ -z "$INSTALL_DIR" ]; then
        [ -n "${HOME:-}" ] || fail "HOME is unset; pass --install-dir <DIR>"
        INSTALL_DIR="${HOME%/}/.local/bin"
    else
        case "$INSTALL_DIR" in
            /) ;;
            *) INSTALL_DIR="${INSTALL_DIR%/}" ;;
        esac
    fi
}

cleanup() {
    if [ "$KEEP_STAGE" != "1" ] && [ -n "$STAGE_DIR" ] && [ -d "$STAGE_DIR" ]; then
        rm -rf "$STAGE_DIR"
    fi
    if [ "$LOCK_ACQUIRED" = "1" ] && [ -n "$INSTALL_LOCK" ] && [ -d "$INSTALL_LOCK" ]; then
        rmdir "$INSTALL_LOCK" 2>/dev/null || true
    fi
}

rollback() {
    _rb_failed=0
    if [ -n "$STAGE_DIR" ] && [ ! -e "$STAGE_DIR/new-cli" ] && [ -e "$INSTALL_DIR/alan" ]; then
        if [ -f "$INSTALL_DIR/alan" ] && [ ! -L "$INSTALL_DIR/alan" ] \
            && [ -n "$NEW_DIGEST" ] \
            && [ "$(sha256_file "$INSTALL_DIR/alan")" = "$NEW_DIGEST" ]; then
            rm -f "$INSTALL_DIR/alan" || _rb_failed=1
        else
            _rb_failed=1
        fi
    fi

    if [ -n "$STAGE_DIR" ] && [ -d "$STAGE_DIR/old" ]; then
        for _rb_saved in \
            "$STAGE_DIR/old/alan" \
            "$STAGE_DIR/old/alan-os-host" \
            "$STAGE_DIR/old/alan-dev" \
            "$STAGE_DIR/old/alan-os-host-dev" \
            "$STAGE_DIR/old/.alan-cli-manifest" \
            "$STAGE_DIR/old/.alan-cli-manifest-stable" \
            "$STAGE_DIR/old/.alan-cli-manifest-dev"
        do
            [ -e "$_rb_saved" ] || [ -L "$_rb_saved" ] || continue
            _rb_dest="$INSTALL_DIR/${_rb_saved##*/}"
            if [ -e "$_rb_dest" ] || [ -L "$_rb_dest" ]; then
                _rb_failed=1
            elif ! mv "$_rb_saved" "$_rb_dest"; then
                _rb_failed=1
            fi
        done
    fi

    if [ "$_rb_failed" = "1" ]; then
        KEEP_STAGE=1
        printf 'error: rollback incomplete; recovery files retained at %s\n' "$STAGE_DIR" >&2
    else
        printf 'error: installation failed; previous executables and receipts restored\n' >&2
    fi
}

handle_signal() {
    _sig_status="$1"
    if [ -n "$STAGE_DIR" ] && [ -e "$STAGE_DIR/new-manifest" ]; then
        rollback
    fi
    exit "$_sig_status"
}

detect_target() {
    if [ -n "${ALAN_TARGET:-}" ]; then
        case "$ALAN_TARGET" in
            aarch64-apple-darwin|x86_64-apple-darwin|x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu)
                printf '%s' "$ALAN_TARGET"
                return 0
                ;;
            *)
                fail "unsupported ALAN_TARGET '$ALAN_TARGET' (supported: aarch64-apple-darwin, x86_64-apple-darwin, x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu)"
                ;;
        esac
    fi

    _os="$(uname -s 2>/dev/null || printf 'unknown')"
    _arch="$(uname -m 2>/dev/null || printf 'unknown')"

    case "$_os" in
        Darwin)
            case "$_arch" in
                arm64|aarch64)
                    printf 'aarch64-apple-darwin'
                    ;;
                x86_64|amd64)
                    _rosetta="0"
                    if command -v sysctl >/dev/null 2>&1; then
                        _rosetta="$(sysctl -n sysctl.proc_translated 2>/dev/null || printf '0')"
                    fi
                    if [ "$_rosetta" = "1" ]; then
                        info "Detected Rosetta 2 translation on Apple Silicon; selecting native aarch64-apple-darwin." >&2
                        printf 'aarch64-apple-darwin'
                    else
                        printf 'x86_64-apple-darwin'
                    fi
                    ;;
                *)
                    fail "unsupported macOS architecture '$_arch' (supported: aarch64-apple-darwin, x86_64-apple-darwin)"
                    ;;
            esac
            ;;
        Linux)
            if [ -r /proc/version ] && grep -qiE 'microsoft|wsl' /proc/version 2>/dev/null; then
                _krel="$(uname -r 2>/dev/null || printf '')"
                if ! grep -qi 'wsl2' /proc/version 2>/dev/null && [ ! -d /run/WSL ]; then
                    case "$_krel" in
                        *-[Mm]icrosoft*)
                            fail "WSL1 is not supported by Alan; upgrade to WSL2 ('wsl --set-version <distro> 2') and re-run."
                            ;;
                    esac
                fi
                info "Detected WSL2; using Linux release artifacts." >&2
            fi
            if command -v ldd >/dev/null 2>&1; then
                if ldd --version 2>&1 | grep -qi 'musl'; then
                    warn "Detected musl libc; Alan Linux release binaries target glibc (*-unknown-linux-gnu)."
                fi
            fi
            case "$_arch" in
                x86_64|amd64)
                    printf 'x86_64-unknown-linux-gnu'
                    ;;
                aarch64|arm64)
                    printf 'aarch64-unknown-linux-gnu'
                    ;;
                *)
                    fail "unsupported Linux architecture '$_arch' (supported: x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu)"
                    ;;
            esac
            ;;
        MINGW*|MSYS*|CYGWIN*|Windows_NT)
            fail "native Windows shells ($_os) are not supported; run this installer inside WSL2 Linux instead."
            ;;
        *)
            fail "unsupported OS '$_os' ($_arch); supported targets: aarch64-apple-darwin, x86_64-apple-darwin, x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu"
            ;;
    esac
}

require_downloader() {
    if command -v curl >/dev/null 2>&1; then
        printf 'curl'
    elif command -v wget >/dev/null 2>&1; then
        printf 'wget'
    else
        fail "either 'curl' or 'wget' is required to download release artifacts"
    fi
}

download_file() {
    _dl_tool="$1"
    _dl_url="$2"
    _dl_out="$3"
    if [ "$_dl_tool" = "curl" ]; then
        curl -fsSL --retry 2 --connect-timeout 15 -o "$_dl_out" "$_dl_url"
    else
        wget -q -O "$_dl_out" "$_dl_url"
    fi
}

sha256_file() {
    _sha_path="$1"
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$_sha_path" | awk '{print tolower($1)}'
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$_sha_path" | awk '{print tolower($1)}'
    elif command -v openssl >/dev/null 2>&1; then
        openssl dgst -sha256 "$_sha_path" | awk '{print tolower($NF)}'
    else
        fail "SHA-256 verification requires 'shasum', 'sha256sum', or 'openssl'"
    fi
}

is_sha256_hex() {
    case "$1" in
        *[!0-9a-fA-F]*) return 1 ;;
    esac
    [ "${#1}" -eq 64 ]
}

normalize_version() {
    _nv="$1"
    case "$_nv" in
        v?*) printf '%s' "${_nv#v}" ;;
        *) printf '%s' "$_nv" ;;
    esac
}

# Extract fields from latest.json/beta.json using python3, jq, or pure POSIX awk fallback.
# Prints 9 lines: schema_version, product, version, tag, target_url, target_sha256, signed, notarized, channel.
parse_manifest_fields() {
    _pm_file="$1"
    _pm_target="$2"

    if command -v python3 >/dev/null 2>&1; then
        python3 - "$_pm_file" "$_pm_target" <<'PY'
import json
import sys

manifest_path, target = sys.argv[1], sys.argv[2]
with open(manifest_path, "r", encoding="utf-8") as f:
    data = json.load(f)

if not isinstance(data, dict):
    sys.stderr.write("error: release manifest must be a JSON object\n")
    sys.exit(1)

schema = data.get("schema_version", "")
product = data.get("product", "")
version = data.get("version", "")
tag = data.get("tag", "")
channel = data.get("channel") or "stable"
targets = data.get("targets") or {}
entry = targets.get(target) if isinstance(targets, dict) else None
if not isinstance(entry, dict):
    entry = {}

url = entry.get("url", "")
sha256 = entry.get("sha256", "")
signed = "true" if entry.get("signed") is True else "false"
notarized = "true" if entry.get("notarized") is True else "false"

fields = [str(schema), str(product), str(version), str(tag), str(url), str(sha256), signed, notarized, str(channel)]
for val in fields:
    if "\n" in val or "\r" in val:
        sys.stderr.write("error: invalid control character in release manifest\n")
        sys.exit(1)
    print(val)
PY
        return $?
    fi

    if command -v jq >/dev/null 2>&1; then
        jq -r --arg t "$_pm_target" '
          def clean: if . == null then "" else tostring | gsub("[\r\n]"; "") end;
          (.schema_version | clean),
          (.product | clean),
          (.version | clean),
          (.tag | clean),
          (.targets[$t].url | clean),
          (.targets[$t].sha256 | clean),
          ((.targets[$t].signed == true) | tostring),
          ((.targets[$t].notarized == true) | tostring),
          ((.channel // "stable") | clean)
        ' "$_pm_file"
        return $?
    fi

    # Pure POSIX awk recursive-descent JSON extractor (supports pretty and minified JSON).
    awk -v target="$_pm_target" '
    function skip_ws() {
        while (pos <= len) {
            c = substr(buf, pos, 1)
            if (c == " " || c == "\t" || c == "\n" || c == "\r") {
                pos++
            } else {
                break
            }
        }
    }
    function parse_string(    out, ch, esc) {
        if (substr(buf, pos, 1) != "\"") {
            err = "expected string at pos " pos
            return ""
        }
        pos++
        out = ""
        while (pos <= len) {
            ch = substr(buf, pos, 1)
            if (ch == "\"") {
                pos++
                return out
            }
            if (ch == "\\") {
                pos++
                esc = substr(buf, pos, 1)
                if (esc == "\"" || esc == "\\" || esc == "/") out = out esc
                else if (esc == "n") out = out "\n"
                else if (esc == "r") out = out "\r"
                else if (esc == "t") out = out "\t"
                else out = out esc
                pos++
            } else {
                out = out ch
                pos++
            }
        }
        err = "unterminated string"
        return ""
    }
    function parse_bare(    start, ch) {
        start = pos
        while (pos <= len) {
            ch = substr(buf, pos, 1)
            if (ch == "," || ch == "}" || ch == "]" || ch == " " || ch == "\t" || ch == "\n" || ch == "\r") {
                break
            }
            pos++
        }
        return substr(buf, start, pos - start)
    }
    function parse_value(path,    ch, key, subp, idx) {
        if (err != "") return
        skip_ws()
        if (pos > len) {
            err = "unexpected EOF"
            return
        }
        ch = substr(buf, pos, 1)
        if (ch == "{") {
            pos++
            skip_ws()
            if (substr(buf, pos, 1) == "}") {
                pos++
                return
            }
            while (pos <= len) {
                skip_ws()
                key = parse_string()
                if (err != "") return
                skip_ws()
                if (substr(buf, pos, 1) != ":") {
                    err = "expected : after key " key
                    return
                }
                pos++
                subp = (path == "" ? key : path "." key)
                parse_value(subp)
                if (err != "") return
                skip_ws()
                ch = substr(buf, pos, 1)
                if (ch == ",") {
                    pos++
                } else if (ch == "}") {
                    pos++
                    return
                } else {
                    err = "expected , or } in object"
                    return
                }
            }
        } else if (ch == "[") {
            pos++
            skip_ws()
            if (substr(buf, pos, 1) == "]") {
                pos++
                return
            }
            idx = 0
            while (pos <= len) {
                parse_value(path "[" idx "]")
                if (err != "") return
                idx++
                skip_ws()
                ch = substr(buf, pos, 1)
                if (ch == ",") {
                    pos++
                } else if (ch == "]") {
                    pos++
                    return
                } else {
                    err = "expected , or ] in array"
                    return
                }
            }
        } else if (ch == "\"") {
            kv[path] = parse_string()
        } else {
            kv[path] = parse_bare()
        }
    }
    BEGIN { buf = "" }
    { buf = buf $0 "\n" }
    END {
        len = length(buf)
        pos = 1
        err = ""
        parse_value("")
        if (err != "") {
            print "error: malformed JSON manifest (" err ")" > "/dev/stderr"
            exit 1
        }
        print kv["schema_version"]
        print kv["product"]
        print kv["version"]
        print kv["tag"]
        print kv["targets." target ".url"]
        print kv["targets." target ".sha256"]
        print (kv["targets." target ".signed"] == "true" ? "true" : "false")
        print (kv["targets." target ".notarized"] == "true" ? "true" : "false")
        print (kv["channel"] != "" ? kv["channel"] : "stable")
    }
    ' "$_pm_file"
}

load_manifest_vars() {
    _lm_file="$1"
    _lm_target="$2"
    _lm_out="$(parse_manifest_fields "$_lm_file" "$_lm_target")" || return 1

    MANIFEST_SCHEMA="$(printf '%s\n' "$_lm_out" | sed -n '1p')"
    MANIFEST_PRODUCT="$(printf '%s\n' "$_lm_out" | sed -n '2p')"
    MANIFEST_VERSION="$(printf '%s\n' "$_lm_out" | sed -n '3p')"
    MANIFEST_TAG="$(printf '%s\n' "$_lm_out" | sed -n '4p')"
    ARCHIVE_URL="$(printf '%s\n' "$_lm_out" | sed -n '5p')"
    ARCHIVE_SHA256="$(printf '%s\n' "$_lm_out" | sed -n '6p' | tr 'A-F' 'a-f')"
    TARGET_SIGNED="$(printf '%s\n' "$_lm_out" | sed -n '7p')"
    TARGET_NOTARIZED="$(printf '%s\n' "$_lm_out" | sed -n '8p')"
    MANIFEST_CHANNEL="$(printf '%s\n' "$_lm_out" | sed -n '9p')"

    if [ -n "$MANIFEST_SCHEMA" ] && [ "$MANIFEST_SCHEMA" != "1" ]; then
        fail "unsupported release manifest schema_version '$MANIFEST_SCHEMA'"
    fi
    if [ -n "$MANIFEST_PRODUCT" ] && [ "$MANIFEST_PRODUCT" != "alan" ]; then
        fail "unexpected release manifest product '$MANIFEST_PRODUCT'"
    fi
    if [ -n "$MANIFEST_CHANNEL" ] && [ "$MANIFEST_CHANNEL" != "stable" ] && [ "$MANIFEST_CHANNEL" != "beta" ]; then
        fail "unexpected release manifest channel '$MANIFEST_CHANNEL'"
    fi
    if [ "$MANIFEST_URL_EXPLICIT" = "0" ] && [ -z "$REQUESTED_VERSION" ] && [ -n "$MANIFEST_CHANNEL" ] && [ "$MANIFEST_CHANNEL" != "$CHANNEL" ]; then
        fail "release manifest channel '$MANIFEST_CHANNEL' does not match requested channel '$CHANNEL'"
    fi
}

parse_checksum_for_asset() {
    _pc_file="$1"
    _pc_asset="$2"
    awk -v asset="$_pc_asset" '
    {
        digest = tolower($1)
        if (NF == 1 && digest ~ /^[0-9a-f]{64}$/) {
            print digest
            exit 0
        }
        name = $NF
        sub(/^.*\//, "", name)
        if ((name == asset || $2 == asset || $2 == ("*" asset)) && digest ~ /^[0-9a-f]{64}$/) {
            print digest
            exit 0
        }
    }
    ' "$_pc_file"
}

resolve_release_artifact() {
    _rr_dl="$1"
    _rr_work="$2"
    _rr_target="$3"
    _rr_manifest_file="$_rr_work/latest.json"

    if [ -z "$REQUESTED_VERSION" ]; then
        download_file "$_rr_dl" "$MANIFEST_URL" "$_rr_manifest_file" \
            || fail "failed to fetch release manifest from $MANIFEST_URL"
        load_manifest_vars "$_rr_manifest_file" "$_rr_target"
        [ -n "$ARCHIVE_URL" ] || fail "release manifest at $MANIFEST_URL has no artifact for target $_rr_target"
        is_sha256_hex "$ARCHIVE_SHA256" || fail "release manifest has invalid sha256 for target $_rr_target"
        RESOLVED_VERSION="${MANIFEST_VERSION:-$(normalize_version "${MANIFEST_TAG:-unknown}")}"
        return 0
    fi

    _req_ver="$(normalize_version "$REQUESTED_VERSION")"
    _req_tag="v$_req_ver"

    if [ "$MANIFEST_URL_EXPLICIT" = "1" ]; then
        download_file "$_rr_dl" "$MANIFEST_URL" "$_rr_manifest_file" \
            || fail "failed to fetch release manifest from $MANIFEST_URL"
        load_manifest_vars "$_rr_manifest_file" "$_rr_target"
        _m_ver="$(normalize_version "${MANIFEST_VERSION:-$MANIFEST_TAG}")"
        if [ "$_m_ver" = "$_req_ver" ] || [ "$MANIFEST_TAG" = "$_req_tag" ]; then
            [ -n "$ARCHIVE_URL" ] || fail "release manifest at $MANIFEST_URL has no artifact for target $_rr_target"
            is_sha256_hex "$ARCHIVE_SHA256" || fail "release manifest has invalid sha256 for target $_rr_target"
            RESOLVED_VERSION="$_req_ver"
            return 0
        fi
        if [ "$RELEASE_BASE_URL_EXPLICIT" != "1" ]; then
            fail "manifest at $MANIFEST_URL provides version '${_m_ver:-unknown}', not requested '$_req_ver'"
        fi
    fi

    _tagged_base="${RELEASE_BASE_URL%/}/$_req_tag"
    _tagged_manifest="$_tagged_base/latest.json"
    if download_file "$_rr_dl" "$_tagged_manifest" "$_rr_manifest_file" 2>/dev/null; then
        load_manifest_vars "$_rr_manifest_file" "$_rr_target"
        [ -n "$ARCHIVE_URL" ] || fail "release manifest at $_tagged_manifest has no artifact for target $_rr_target"
        is_sha256_hex "$ARCHIVE_SHA256" || fail "release manifest at $_tagged_manifest has invalid sha256 for target $_rr_target"
        RESOLVED_VERSION="${MANIFEST_VERSION:-$_req_ver}"
        return 0
    fi

    _asset="alan-${_req_ver}-${_rr_target}.tar.gz"
    ARCHIVE_URL="$_tagged_base/$_asset"
    _sha_file="$_rr_work/asset.sha256"
    ARCHIVE_SHA256=""
    if download_file "$_rr_dl" "${ARCHIVE_URL}.sha256" "$_sha_file" 2>/dev/null; then
        ARCHIVE_SHA256="$(parse_checksum_for_asset "$_sha_file" "$_asset" || true)"
    elif download_file "$_rr_dl" "$_tagged_base/checksums.txt" "$_sha_file" 2>/dev/null; then
        ARCHIVE_SHA256="$(parse_checksum_for_asset "$_sha_file" "$_asset" || true)"
    fi
    is_sha256_hex "$ARCHIVE_SHA256" \
        || fail "could not resolve verified SHA-256 checksum for $_asset from $_tagged_base"
    RESOLVED_VERSION="$_req_ver"
    TARGET_SIGNED="false"
    TARGET_NOTARIZED="false"
}

# Match scripts/install-ownership.sh verbatim.
manifest_digest() {
    _md_manifest="$1"
    _md_wanted="$2"
    _md_found=""
    _md_seen="|"

    [ -e "$_md_manifest" ] || [ -L "$_md_manifest" ] || { printf ''; return 0; }
    [ -f "$_md_manifest" ] && [ ! -L "$_md_manifest" ] || fail "non-regular manifest at $_md_manifest"

    while IFS='|' read -r _md_name _md_digest _md_extra || [ -n "${_md_name:-}${_md_digest:-}${_md_extra:-}" ]; do
        [ -z "${_md_name:-}${_md_digest:-}${_md_extra:-}" ] && continue
        case "${_md_manifest##*/}:${_md_name}" in
            .alan-cli-manifest:alan | .alan-cli-manifest-stable:alan | \
            .alan-cli-manifest-stable:alan-os-host | .alan-cli-manifest-dev:alan-dev | \
            .alan-cli-manifest-dev:alan-os-host-dev) ;;
            *) fail "invalid ownership entry in $_md_manifest" ;;
        esac
        is_sha256_hex "${_md_digest:-}" && [ -z "${_md_extra:-}" ] \
            || fail "invalid ownership digest in $_md_manifest"
        case "$_md_seen" in
            *"|$_md_name|"*) fail "duplicate ownership entry in $_md_manifest" ;;
        esac
        _md_seen="${_md_seen}${_md_name}|"
        [ "$_md_name" != "$_md_wanted" ] || _md_found="$(printf '%s' "$_md_digest" | tr 'A-F' 'a-f')"
    done <"$_md_manifest"

    printf '%s' "$_md_found"
}

preflight_owned_path() {
    _po_path="$1"
    _po_expected="$2"
    _po_allow_force="${3:-0}"

    [ -e "$_po_path" ] || [ -L "$_po_path" ] || return 0
    if [ -d "$_po_path" ] && [ ! -L "$_po_path" ]; then
        fail "directory exists at $_po_path"
    fi
    if [ "$_po_allow_force" = "1" ]; then
        return 0
    fi
    [ -f "$_po_path" ] && [ ! -L "$_po_path" ] || fail "non-regular file at $_po_path"
    [ -n "$_po_expected" ] || fail "unowned file at $_po_path (re-run with --force to overwrite)"
    [ "$(sha256_file "$_po_path")" = "$_po_expected" ] \
        || fail "modified owned file at $_po_path (re-run with --force to overwrite)"
}

lookup_expected_digest() {
    _le_name="$1"
    _le_expected=""
    for _le_receipt in \
        "$INSTALL_DIR/.alan-cli-manifest" \
        "$INSTALL_DIR/.alan-cli-manifest-stable" \
        "$INSTALL_DIR/.alan-cli-manifest-dev"
    do
        if [ "$FORCE" = "1" ] && [ "$_le_receipt" = "$INSTALL_DIR/.alan-cli-manifest" ]; then
            if [ -e "$_le_receipt" ] && { [ ! -f "$_le_receipt" ] || [ -L "$_le_receipt" ]; }; then
                continue
            fi
            if ! _le_digest="$(manifest_digest "$_le_receipt" "$_le_name" 2>/dev/null)"; then
                continue
            fi
        else
            _le_digest="$(manifest_digest "$_le_receipt" "$_le_name")"
        fi
        if [ -n "$_le_digest" ]; then
            if [ -n "$_le_expected" ]; then
                if [ "$FORCE" = "1" ] && [ "$_le_name" = "alan" ]; then
                    continue
                fi
                fail "conflicting ownership receipts for $INSTALL_DIR/$_le_name"
            fi
            _le_expected="$_le_digest"
        fi
    done
    printf '%s' "$_le_expected"
}

preflight_all_install_paths() {
    _pf_alan_expected="$(lookup_expected_digest alan)"
    preflight_owned_path "$INSTALL_DIR/alan" "$_pf_alan_expected" "$FORCE"

    for _pf_legacy in alan-os-host alan-dev alan-os-host-dev; do
        _pf_legacy_expected="$(lookup_expected_digest "$_pf_legacy")"
        preflight_owned_path "$INSTALL_DIR/$_pf_legacy" "$_pf_legacy_expected" 0
    done

    DIGEST_ALAN="$_pf_alan_expected"
    DIGEST_LEGACY_HOST="$(lookup_expected_digest alan-os-host)"
    DIGEST_LEGACY_DEV="$(lookup_expected_digest alan-dev)"
    DIGEST_LEGACY_DEV_HOST="$(lookup_expected_digest alan-os-host-dev)"
}

acquire_install_lock() {
    mkdir -p "$INSTALL_DIR"
    INSTALL_LOCK="$INSTALL_DIR/.alan-cli-install.lock"
    _lock_attempt=0
    while ! mkdir "$INSTALL_LOCK" 2>/dev/null; do
        _lock_attempt=$((_lock_attempt + 1))
        if [ "$_lock_attempt" -ge 3 ]; then
            fail "installation lock exists at $INSTALL_LOCK; stop any installer before recovering an interrupted installation"
        fi
        sleep 1
    done
    LOCK_ACQUIRED=1
}

validate_and_unpack_archive() {
    _va_archive="$1"
    _va_unpack="$2"

    command -v tar >/dev/null 2>&1 || fail "'tar' is required to unpack release archives"

    _va_list="$(tar -tzf "$_va_archive")" || fail "failed to inspect release archive $_va_archive"
    _va_found_alan=""

    _va_old_ifs="$IFS"
    IFS='
'
    set -f
    for _va_entry in $_va_list; do
        case "$_va_entry" in
            /*)
                set +f
                IFS="$_va_old_ifs"
                fail "refusing unsafe absolute path in archive: $_va_entry"
                ;;
            ..|../*|*/../*|*/..)
                set +f
                IFS="$_va_old_ifs"
                fail "refusing path traversal entry in archive: $_va_entry"
                ;;
            ./alan|alan)
                _va_found_alan="$_va_entry"
                ;;
        esac
    done
    set +f
    IFS="$_va_old_ifs"

    [ -n "$_va_found_alan" ] || fail "release archive does not contain the 'alan' executable"

    mkdir -p "$_va_unpack"
    tar -xzf "$_va_archive" -C "$_va_unpack" "$_va_found_alan" \
        || fail "failed to extract $_va_found_alan from release archive"

    _va_bin="$_va_unpack/alan"
    [ -f "$_va_bin" ] && [ ! -L "$_va_bin" ] \
        || fail "extracted 'alan' entry is not a regular file"

    printf '%s' "$_va_bin"
}

path_contains_dir() {
    _pc_dir="${1%/}"
    [ -n "$_pc_dir" ] || _pc_dir="/"
    _pc_old_ifs="$IFS"
    IFS=':'
    for _pc_item in ${PATH:-}; do
        _pc_norm="${_pc_item%/}"
        [ -n "$_pc_norm" ] || _pc_norm="/"
        if [ "$_pc_norm" = "$_pc_dir" ]; then
            IFS="$_pc_old_ifs"
            return 0
        fi
    done
    IFS="$_pc_old_ifs"
    return 1
}

maybe_update_shell_path() {
    if path_contains_dir "$INSTALL_DIR"; then
        return 0
    fi
    if [ "$NO_MODIFY_PATH" = "1" ] || [ -z "${HOME:-}" ]; then
        return 1
    fi

    _rc_file=""
    _shell_name="${SHELL:-}"
    case "$_shell_name" in
        zsh|*/zsh)
            _rc_file="${ZDOTDIR:-$HOME}/.zshrc"
            ;;
        bash|*/bash)
            if [ "$(uname -s 2>/dev/null || printf '')" = "Darwin" ] && [ -f "$HOME/.bash_profile" ]; then
                _rc_file="$HOME/.bash_profile"
            elif [ -f "$HOME/.bashrc" ]; then
                _rc_file="$HOME/.bashrc"
            elif [ "$(uname -s 2>/dev/null || printf '')" = "Darwin" ]; then
                _rc_file="$HOME/.bash_profile"
            else
                _rc_file="$HOME/.bashrc"
            fi
            ;;
        fish|*/fish)
            _rc_file="$HOME/.config/fish/config.fish"
            ;;
        *)
            _rc_file="$HOME/.profile"
            ;;
    esac

    _rc_dir="${_rc_file%/*}"
    if [ -n "$_rc_dir" ] && [ ! -d "$_rc_dir" ]; then
        mkdir -p "$_rc_dir" 2>/dev/null || return 1
    fi

    _display_dir="$INSTALL_DIR"
    if [ "$INSTALL_DIR" = "${HOME%/}/.local/bin" ]; then
        # shellcheck disable=SC2016
        _display_dir='$HOME/.local/bin'
    fi

    if [ -f "$_rc_file" ]; then
        if grep -Fq "$INSTALL_DIR" "$_rc_file" 2>/dev/null \
            || grep -Fq "$_display_dir" "$_rc_file" 2>/dev/null; then
            return 0
        fi
    fi

    case "$_rc_file" in
        */config.fish)
            {
                printf '\n# Added by Alan installer\n'
                printf 'fish_add_path "%s"\n' "$INSTALL_DIR"
            } >>"$_rc_file" 2>/dev/null || return 1
            ;;
        *)
            {
                printf '\n# Added by Alan installer\n'
                # shellcheck disable=SC2016
                printf 'export PATH="%s:$PATH"\n' "$_display_dir"
            } >>"$_rc_file" 2>/dev/null || return 1
            ;;
    esac

    MODIFIED_RC_FILE="$_rc_file"
    return 0
}

main() {
    [ "${ALAN_INSTALL_CHANNEL+x}" != "x" ] \
        || fail "ALAN_INSTALL_CHANNEL is retired; unset it and install alan"

    parse_args "$@"
    trap cleanup EXIT

    TARGET="$(detect_target)"
    DOWNLOADER="$(require_downloader)"

    if [ "$DRY_RUN" = "1" ]; then
        STAGE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/alan-install-dryrun.XXXXXX")"
        resolve_release_artifact "$DOWNLOADER" "$STAGE_DIR" "$TARGET"
        preflight_all_install_paths
        info "[dry-run] Channel:       $CHANNEL"
        info "[dry-run] Target:        $TARGET"
        info "[dry-run] Version:       $RESOLVED_VERSION"
        info "[dry-run] Manifest URL:  $MANIFEST_URL"
        info "[dry-run] Archive URL:   $ARCHIVE_URL"
        info "[dry-run] SHA-256:       $ARCHIVE_SHA256"
        info "[dry-run] Install dir:   $INSTALL_DIR"
        info "[dry-run] Manifest file: $INSTALL_DIR/.alan-cli-manifest"
        if ! path_contains_dir "$INSTALL_DIR" && [ "$NO_MODIFY_PATH" != "1" ]; then
            info "[dry-run] Note: $INSTALL_DIR is not on PATH (would update shell rc unless --no-modify-path)"
        fi
        return 0
    fi

    acquire_install_lock
    preflight_all_install_paths

    STAGE_DIR="$(mktemp -d "$INSTALL_DIR/.alan-cli-install.XXXXXX")"
    mkdir -p "$STAGE_DIR/old" "$STAGE_DIR/dl" "$STAGE_DIR/unpack"

    trap 'handle_signal 130' INT
    trap 'handle_signal 129' HUP
    trap 'handle_signal 143' TERM

    resolve_release_artifact "$DOWNLOADER" "$STAGE_DIR/dl" "$TARGET"

    info "Downloading Alan $RESOLVED_VERSION ($TARGET)..."
    _archive_path="$STAGE_DIR/dl/alan.tar.gz"
    download_file "$DOWNLOADER" "$ARCHIVE_URL" "$_archive_path" \
        || fail "failed to download release archive from $ARCHIVE_URL"

    _actual_archive_sha="$(sha256_file "$_archive_path")"
    if [ "$_actual_archive_sha" != "$ARCHIVE_SHA256" ]; then
        fail "SHA-256 checksum mismatch for $_archive_path (expected $ARCHIVE_SHA256, got $_actual_archive_sha)"
    fi
    info "Verified SHA-256 checksum ($_actual_archive_sha)."

    _unpacked_bin="$(validate_and_unpack_archive "$_archive_path" "$STAGE_DIR/unpack")"
    cp "$_unpacked_bin" "$STAGE_DIR/new-cli"
    chmod 0755 "$STAGE_DIR/new-cli"
    NEW_DIGEST="$(sha256_file "$STAGE_DIR/new-cli")"
    printf 'alan|%s\n' "$NEW_DIGEST" >"$STAGE_DIR/new-manifest"

    # Re-verify ownership right before retiring existing paths.
    if ! (
        preflight_owned_path "$INSTALL_DIR/alan" "$DIGEST_ALAN" "$FORCE"
        preflight_owned_path "$INSTALL_DIR/alan-os-host" "$DIGEST_LEGACY_HOST" 0
        preflight_owned_path "$INSTALL_DIR/alan-dev" "$DIGEST_LEGACY_DEV" 0
        preflight_owned_path "$INSTALL_DIR/alan-os-host-dev" "$DIGEST_LEGACY_DEV_HOST" 0
    ); then
        rollback
        fail "ownership changed before retiring existing binaries"
    fi

    for _retire_name in alan alan-os-host alan-dev alan-os-host-dev; do
        _retire_path="$INSTALL_DIR/$_retire_name"
        if [ -e "$_retire_path" ] || [ -L "$_retire_path" ]; then
            mv "$_retire_path" "$STAGE_DIR/old/$_retire_name" \
                || { rollback; fail "unable to retire $_retire_path"; }
        fi
    done

    for _retire_manifest in \
        "$INSTALL_DIR/.alan-cli-manifest" \
        "$INSTALL_DIR/.alan-cli-manifest-stable" \
        "$INSTALL_DIR/.alan-cli-manifest-dev"
    do
        if [ -e "$_retire_manifest" ] || [ -L "$_retire_manifest" ]; then
            mv "$_retire_manifest" "$STAGE_DIR/old/${_retire_manifest##*/}" \
                || { rollback; fail "unable to retire $_retire_manifest"; }
        fi
    done

    mv "$STAGE_DIR/new-cli" "$INSTALL_DIR/alan" \
        || { rollback; fail "unable to publish $INSTALL_DIR/alan"; }
    mv "$STAGE_DIR/new-manifest" "$INSTALL_DIR/.alan-cli-manifest" \
        || { rollback; fail "unable to publish ownership receipt"; }

    MODIFIED_RC_FILE=""
    _path_ok=1
    if ! path_contains_dir "$INSTALL_DIR"; then
        if ! maybe_update_shell_path; then
            _path_ok=0
        fi
    fi

    info ""
    info "Installed Alan $RESOLVED_VERSION to $INSTALL_DIR/alan."
    if [ "${TARGET_SIGNED:-false}" = "true" ] && [ "${TARGET_NOTARIZED:-false}" = "true" ]; then
        info "Release artifact is Apple Developer ID signed and notarized."
    fi
    if [ -n "$MODIFIED_RC_FILE" ]; then
        info "Added $INSTALL_DIR to PATH in $MODIFIED_RC_FILE (restart your shell or run: export PATH=\"$INSTALL_DIR:\$PATH\")."
    elif [ "$_path_ok" = "0" ]; then
        info "Add $INSTALL_DIR to your PATH:"
        info "  export PATH=\"$INSTALL_DIR:\$PATH\""
    fi
    info ""
    info "Next steps:"
    info "  alan login    # authenticate ChatGPT, OpenRouter, Vertex AI, Magpie, or OpenAI"
    info "  alan          # start a foreground alan9 instance & terminal UI"
    info "  alan update   # check for and install future Alan releases"
}

main "$@"
