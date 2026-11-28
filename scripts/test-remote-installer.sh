#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
INSTALLER="$SCRIPT_DIR/install.sh"
WEBSITE_INSTALLER="$PROJECT_ROOT/website/public/install.sh"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

sha256() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        openssl dgst -sha256 "$1" | awk '{print tolower($NF)}'
    fi
}

sh -n "$INSTALLER"
if command -v dash >/dev/null 2>&1; then
    dash -n "$INSTALLER"
fi
cmp -s "$INSTALLER" "$WEBSITE_INSTALLER" \
    || fail "scripts/install.sh and website/public/install.sh differ"

TEST_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/alan-remote-installer-test.XXXXXX")"
SERVER_PID=""
cleanup() {
    if [[ -n "$SERVER_PID" ]]; then
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    rm -rf "$TEST_ROOT"
}
trap cleanup EXIT

HTTP_ROOT="$TEST_ROOT/http"
mkdir -p "$HTTP_ROOT/releases/download/v0.1.0" "$HTTP_ROOT/releases/download/v0.2.0" "$HTTP_ROOT/releases/download/v0.3.0"

build_stub_tarball() {
    local version="$1"
    local target="$2"
    local out_dir="$3"
    local stage="$TEST_ROOT/build-$version-$target"
    mkdir -p "$stage"
    cat >"$stage/alan" <<EOF
#!/bin/sh
if [ "\${1:-}" = "--version" ]; then
    printf 'alan %s\n' "$version"
    exit 0
fi
printf 'alan stub %s\n' "$version"
EOF
    chmod 0755 "$stage/alan"
    printf '{\n  "product": "alan-cli",\n  "version": "%s",\n  "target": "%s",\n  "binaries": ["alan"]\n}\n' \
        "$version" "$target" >"$stage/manifest.json"

    local asset="alan-$version-$target.tar.gz"
    tar -czf "$out_dir/$asset" -C "$stage" .
    sha256 "$out_dir/$asset" >"$out_dir/$asset.sha256"
    printf '%s  %s\n' "$(sha256 "$out_dir/$asset")" "$asset" >>"$out_dir/checksums.txt"
}

TARGETS=(
    aarch64-apple-darwin
    x86_64-apple-darwin
    x86_64-unknown-linux-gnu
    aarch64-unknown-linux-gnu
)

for target in "${TARGETS[@]}"; do
    build_stub_tarball "0.1.0" "$target" "$HTTP_ROOT/releases/download/v0.1.0"
    build_stub_tarball "0.2.0" "$target" "$HTTP_ROOT/releases/download/v0.2.0"
    build_stub_tarball "0.3.0" "$target" "$HTTP_ROOT/releases/download/v0.3.0"
done

# Pick a free local port and start python3 http.server
PORT="$(python3 - <<'PY'
import socket
s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
)"

BASE_URL="http://127.0.0.1:$PORT"
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$HTTP_ROOT" \
    >"$TEST_ROOT/http.log" 2>&1 &
SERVER_PID="$!"

# Wait until server responds
for _ in $(seq 1 30); do
    if curl -fsS "$BASE_URL/" >/dev/null 2>&1; then
        break
    fi
    sleep 0.1
done

write_manifest() {
    local version="$1"
    local out_path="$2"
    local compact="${3:-0}"
    local override_sha="${4:-}"
    local override_url="${5:-}"
    local channel="${6:-stable}"
    local tag="${7:-}"
    local asset_dir="${8:-$HTTP_ROOT/releases/download/v$version}"
    if [[ -z "$tag" ]]; then
        if [[ "$channel" == "beta" ]]; then
            tag="beta"
        else
            tag="v$version"
        fi
    fi
    python3 - "$version" "$BASE_URL" "$asset_dir" "$out_path" "$compact" "$override_sha" "$override_url" "$channel" "$tag" <<'PY'
import hashlib, json, os, sys

version, base_url, asset_dir, out_path, compact, override_sha, override_url, channel, tag = sys.argv[1:]
targets = {}
for triple in [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
]:
    asset = f"alan-{version}-{triple}.tar.gz"
    path = os.path.join(asset_dir, asset)
    with open(path, "rb") as f:
        data = f.read()
    digest = override_sha if override_sha else hashlib.sha256(data).hexdigest()
    url = override_url if override_url else f"{base_url}/releases/download/{tag}/{asset}"
    targets[triple] = {
        "asset": asset,
        "url": url,
        "sha256": digest,
        "size": len(data),
        "signed": triple.endswith("apple-darwin"),
        "notarized": triple.endswith("apple-darwin"),
    }

payload = {
    "schema_version": 1,
    "product": "alan",
    "channel": channel,
    "version": version,
    "tag": tag,
    "published_at": "2026-11-20T00:00:00Z",
    "release_url": f"https://github.com/thinksmart-systems/alan/releases/tag/{tag}",
    "notes_url": f"https://github.com/thinksmart-systems/alan/releases/tag/{tag}",
    "installer_url": "https://alanworks.app/install.sh",
    "targets": targets,
}
if channel == "beta":
    payload["commit"] = "6c1dfd7"
with open(out_path, "w", encoding="utf-8") as f:
    if compact == "1":
        json.dump(payload, f, separators=(",", ":"))
    else:
        json.dump(payload, f, indent=2)
        f.write("\n")
PY
}

BETA_VERSION="0.2.0-beta.20261123.42"
mkdir -p "$HTTP_ROOT/releases/download/beta"
for target in "${TARGETS[@]}"; do
    build_stub_tarball "$BETA_VERSION" "$target" "$HTTP_ROOT/releases/download/beta"
done

write_manifest "0.1.0" "$HTTP_ROOT/releases/download/v0.1.0/latest.json" 0
write_manifest "0.2.0" "$HTTP_ROOT/releases/download/v0.2.0/latest.json" 0
write_manifest "0.2.0" "$HTTP_ROOT/latest-0.2.0-minified.json" 1
write_manifest "0.1.0" "$HTTP_ROOT/latest.json" 0
write_manifest "$BETA_VERSION" "$HTTP_ROOT/beta.json" 0 "" "" "beta" "beta" "$HTTP_ROOT/releases/download/beta"
write_manifest "$BETA_VERSION" "$HTTP_ROOT/beta-minified.json" 1 "" "" "beta" "beta" "$HTTP_ROOT/releases/download/beta"

# Create corrupt archive & path traversal archive
printf 'not a gzip archive\n' >"$HTTP_ROOT/corrupt.tar.gz"
write_manifest "0.2.0" "$HTTP_ROOT/latest-bad-sha.json" 0 "0000000000000000000000000000000000000000000000000000000000000000"
write_manifest "0.2.0" "$HTTP_ROOT/latest-corrupt.json" 0 "$(sha256 "$HTTP_ROOT/corrupt.tar.gz")" "$BASE_URL/corrupt.tar.gz"

python3 - "$HTTP_ROOT/traversal.tar.gz" <<'PY'
import io, sys, tarfile
with tarfile.open(sys.argv[1], "w:gz") as tf:
    info = tarfile.TarInfo("../escape")
    data = b"pwned\n"
    info.size = len(data)
    info.mode = 0o755
    tf.addfile(info, io.BytesIO(data))
    info2 = tarfile.TarInfo("./alan")
    data2 = b"#!/bin/sh\necho hi\n"
    info2.size = len(data2)
    info2.mode = 0o755
    tf.addfile(info2, io.BytesIO(data2))
PY
write_manifest "0.2.0" "$HTTP_ROOT/latest-traversal.json" 0 "$(sha256 "$HTTP_ROOT/traversal.tar.gz")" "$BASE_URL/traversal.tar.gz"

# 1. --help
"$INSTALLER" --help >/dev/null

# 2. Fresh install + .alan-cli-manifest + --version + --no-modify-path
FRESH_HOME="$TEST_ROOT/fresh-home"
FRESH_BIN="$TEST_ROOT/fresh-bin"
mkdir -p "$FRESH_HOME"
HOME="$FRESH_HOME" SHELL="/bin/zsh" "$INSTALLER" \
    --manifest-url "$BASE_URL/latest.json" \
    --install-dir "$FRESH_BIN" \
    --no-modify-path >/dev/null
[[ -x "$FRESH_BIN/alan" ]] || fail "fresh install did not install executable $FRESH_BIN/alan"
[[ "$("$FRESH_BIN/alan" --version)" == "alan 0.1.0" ]] || fail "unexpected --version output on fresh install"
[[ "$(cat "$FRESH_BIN/.alan-cli-manifest")" == "alan|$(sha256 "$FRESH_BIN/alan")" ]] \
    || fail "fresh install manifest mismatch"
[[ ! -e "$FRESH_BIN/.alan-cli-install.lock" ]] || fail "fresh install left stale lock"
[[ ! -e "$FRESH_HOME/.zshrc" ]] || fail "--no-modify-path touched .zshrc"

# 3. SHA-256 mismatch, corrupt tarball, and path traversal abort cleanly
cp "$FRESH_BIN/alan" "$TEST_ROOT/fresh-alan-before"
cp "$FRESH_BIN/.alan-cli-manifest" "$TEST_ROOT/fresh-manifest-before"
for bad_manifest in latest-bad-sha.json latest-corrupt.json latest-traversal.json; do
    if HOME="$FRESH_HOME" "$INSTALLER" \
        --manifest-url "$BASE_URL/$bad_manifest" \
        --install-dir "$FRESH_BIN" \
        --no-modify-path >"$TEST_ROOT/bad.out" 2>"$TEST_ROOT/bad.err"; then
        fail "installer succeeded unexpectedly for $bad_manifest"
    fi
    cmp -s "$TEST_ROOT/fresh-alan-before" "$FRESH_BIN/alan" \
        || fail "$bad_manifest modified existing binary"
    cmp -s "$TEST_ROOT/fresh-manifest-before" "$FRESH_BIN/.alan-cli-manifest" \
        || fail "$bad_manifest modified existing manifest"
    [[ ! -e "$FRESH_BIN/.alan-cli-install.lock" ]] \
        || fail "$bad_manifest left stale lock"
done

# 4. Owned upgrade (v0.1.0 -> v0.2.0)
HOME="$FRESH_HOME" "$INSTALLER" \
    --manifest-url "$BASE_URL/releases/download/v0.2.0/latest.json" \
    --install-dir "$FRESH_BIN" \
    --no-modify-path >/dev/null
[[ "$("$FRESH_BIN/alan" --version)" == "alan 0.2.0" ]] || fail "upgrade did not install 0.2.0"
[[ "$(cat "$FRESH_BIN/.alan-cli-manifest")" == "alan|$(sha256 "$FRESH_BIN/alan")" ]] \
    || fail "upgrade did not update manifest sha256"

# 5. Legacy upgrade & safe retirement
LEGACY_BIN="$TEST_ROOT/legacy-bin"
mkdir -p "$LEGACY_BIN"
printf 'old stable CLI\n' >"$LEGACY_BIN/alan"
printf 'old stable Host\n' >"$LEGACY_BIN/alan-os-host"
printf 'old dev CLI\n' >"$LEGACY_BIN/alan-dev"
printf 'old dev Host\n' >"$LEGACY_BIN/alan-os-host-dev"
printf 'alan|%s\nalan-os-host|%s\n' \
    "$(sha256 "$LEGACY_BIN/alan")" "$(sha256 "$LEGACY_BIN/alan-os-host")" \
    >"$LEGACY_BIN/.alan-cli-manifest-stable"
printf 'alan-dev|%s\nalan-os-host-dev|%s\n' \
    "$(sha256 "$LEGACY_BIN/alan-dev")" "$(sha256 "$LEGACY_BIN/alan-os-host-dev")" \
    >"$LEGACY_BIN/.alan-cli-manifest-dev"
HOME="$FRESH_HOME" "$INSTALLER" \
    --manifest-url "$BASE_URL/latest.json" \
    --install-dir "$LEGACY_BIN" \
    --no-modify-path >/dev/null
[[ "$("$LEGACY_BIN/alan" --version)" == "alan 0.1.0" ]] || fail "legacy upgrade failed"
for retired in alan-os-host alan-dev alan-os-host-dev .alan-cli-manifest-stable .alan-cli-manifest-dev; do
    [[ ! -e "$LEGACY_BIN/$retired" ]] || fail "legacy upgrade left retired $retired"
done
[[ "$(cat "$LEGACY_BIN/.alan-cli-manifest")" == "alan|$(sha256 "$LEGACY_BIN/alan")" ]] \
    || fail "legacy upgrade did not write canonical manifest"

# 6. Unowned existing alan binary refused unless --force
UNOWNED_BIN="$TEST_ROOT/unowned-bin"
mkdir -p "$UNOWNED_BIN"
printf 'user-owned binary\n' >"$UNOWNED_BIN/alan"
chmod 0755 "$UNOWNED_BIN/alan"
if HOME="$FRESH_HOME" "$INSTALLER" \
    --manifest-url "$BASE_URL/latest.json" \
    --install-dir "$UNOWNED_BIN" \
    --no-modify-path >"$TEST_ROOT/unowned.out" 2>"$TEST_ROOT/unowned.err"; then
    fail "installer overwrote unowned alan binary without --force"
fi
grep -Fq "$UNOWNED_BIN/alan" "$TEST_ROOT/unowned.err" \
    || fail "unowned error did not mention $UNOWNED_BIN/alan"
[[ "$(cat "$UNOWNED_BIN/alan")" == "user-owned binary" ]] \
    || fail "unowned binary was modified on failure"
[[ ! -e "$UNOWNED_BIN/.alan-cli-manifest" ]] || fail "manifest written despite unowned refusal"
[[ ! -e "$UNOWNED_BIN/.alan-cli-install.lock" ]] || fail "stale lock left after unowned refusal"

# 7. --force and ALAN_INSTALL_FORCE=1 overwrite succeed and update .alan-cli-manifest
HOME="$FRESH_HOME" "$INSTALLER" \
    --manifest-url "$BASE_URL/latest.json" \
    --install-dir "$UNOWNED_BIN" \
    --force \
    --no-modify-path >/dev/null
[[ "$("$UNOWNED_BIN/alan" --version)" == "alan 0.1.0" ]] || fail "--force overwrite failed"
[[ "$(cat "$UNOWNED_BIN/.alan-cli-manifest")" == "alan|$(sha256 "$UNOWNED_BIN/alan")" ]] \
    || fail "--force did not write manifest"

printf 'modified locally\n' >>"$UNOWNED_BIN/alan"
HOME="$FRESH_HOME" ALAN_INSTALL_FORCE=1 "$INSTALLER" \
    --manifest-url "$BASE_URL/releases/download/v0.2.0/latest.json" \
    --install-dir "$UNOWNED_BIN" \
    --no-modify-path >/dev/null
[[ "$("$UNOWNED_BIN/alan" --version)" == "alan 0.2.0" ]] || fail "ALAN_INSTALL_FORCE=1 overwrite failed"
[[ "$(cat "$UNOWNED_BIN/.alan-cli-manifest")" == "alan|$(sha256 "$UNOWNED_BIN/alan")" ]] \
    || fail "ALAN_INSTALL_FORCE=1 did not update manifest"

# 8. Legacy ALAN_INSTALL_CHANNEL rejected
for legacy_val in dev stable ""; do
    if ALAN_INSTALL_CHANNEL="$legacy_val" HOME="$FRESH_HOME" "$INSTALLER" \
        --manifest-url "$BASE_URL/latest.json" \
        --install-dir "$TEST_ROOT/channel-bin" \
        --no-modify-path >"$TEST_ROOT/chan.out" 2>"$TEST_ROOT/chan.err"; then
        fail "installer accepted retired ALAN_INSTALL_CHANNEL='$legacy_val'"
    fi
    grep -Fq "ALAN_INSTALL_CHANNEL" "$TEST_ROOT/chan.err" \
        || fail "missing ALAN_INSTALL_CHANNEL retirement message"
done
[[ ! -e "$TEST_ROOT/channel-bin" ]] || fail "channel rejection created install dir"

# 9. --dry-run makes no filesystem changes
DRY_HOME="$TEST_ROOT/dry-home"
DRY_BIN="$TEST_ROOT/dry-bin"
mkdir -p "$DRY_HOME"
HOME="$DRY_HOME" SHELL="/bin/zsh" "$INSTALLER" \
    --manifest-url "$BASE_URL/latest.json" \
    --install-dir "$DRY_BIN" \
    --dry-run >"$TEST_ROOT/dry.out"
[[ ! -e "$DRY_BIN" ]] || fail "--dry-run created install directory"
[[ ! -e "$DRY_HOME/.zshrc" ]] || fail "--dry-run created .zshrc"
grep -Fq "[dry-run]" "$TEST_ROOT/dry.out" || fail "--dry-run did not print dry-run summary"

# 10. Pure-POSIX awk JSON fallback (no python3, no jq on PATH) + minified latest.json
POSIX_TOOLS="$TEST_ROOT/posix-tools"
mkdir -p "$POSIX_TOOLS"
for cmd in sh awk sed tr grep mkdir rmdir rm mv cp chmod mktemp tar uname curl wget shasum sha256sum openssl sleep cat; do
    if resolved="$(command -v "$cmd" 2>/dev/null)"; then
        ln -sf "$resolved" "$POSIX_TOOLS/$cmd"
    fi
done
AWK_BIN="$TEST_ROOT/awk-bin"
PATH="$POSIX_TOOLS" HOME="$FRESH_HOME" "$INSTALLER" \
    --manifest-url "$BASE_URL/latest-0.2.0-minified.json" \
    --install-dir "$AWK_BIN" \
    --no-modify-path >/dev/null
[[ "$("$AWK_BIN/alan" --version)" == "alan 0.2.0" ]] \
    || fail "POSIX awk fallback failed on minified latest.json"
[[ "$(cat "$AWK_BIN/.alan-cli-manifest")" == "alan|$(sha256 "$AWK_BIN/alan")" ]] \
    || fail "POSIX awk fallback wrote invalid manifest"

# 11. --version with ALAN_RELEASE_BASE_URL (both latest.json and direct .tar.gz + .sha256 fallback)
VER_BIN="$TEST_ROOT/ver-bin"
HOME="$FRESH_HOME" ALAN_RELEASE_BASE_URL="$BASE_URL/releases/download" "$INSTALLER" \
    --version 0.1.0 \
    --install-dir "$VER_BIN" \
    --no-modify-path >/dev/null
[[ "$("$VER_BIN/alan" --version)" == "alan 0.1.0" ]] || fail "--version 0.1.0 failed"

# v0.3.0 has NO latest.json, only .tar.gz + .sha256
HOME="$FRESH_HOME" ALAN_RELEASE_BASE_URL="$BASE_URL/releases/download" "$INSTALLER" \
    --version v0.3.0 \
    --install-dir "$VER_BIN" \
    --no-modify-path >/dev/null
[[ "$("$VER_BIN/alan" --version)" == "alan 0.3.0" ]] || fail "--version v0.3.0 sha256 fallback failed"

# 12. Shell RC update when --no-modify-path is omitted (and idempotent second run)
RC_HOME="$TEST_ROOT/rc-home"
RC_BIN="$RC_HOME/.local/bin"
mkdir -p "$RC_HOME"
HOME="$RC_HOME" SHELL="/bin/zsh" "$INSTALLER" \
    --manifest-url "$BASE_URL/latest.json" \
    --install-dir "$RC_BIN" >/dev/null
[[ -f "$RC_HOME/.zshrc" ]] || fail "installer did not update .zshrc when not on PATH"
grep -Fq '$HOME/.local/bin' "$RC_HOME/.zshrc" || fail ".zshrc missing \$HOME/.local/bin PATH export"
HOME="$RC_HOME" SHELL="/bin/zsh" "$INSTALLER" \
    --manifest-url "$BASE_URL/latest.json" \
    --install-dir "$RC_BIN" >/dev/null
[[ "$(grep -c "Added by Alan installer" "$RC_HOME/.zshrc")" -eq 1 ]] \
    || fail "RC PATH modification was not idempotent"

# 13. Unsupported OS rejection & Rosetta 2 detection
MOCK_BIN="$TEST_ROOT/mock-bin"
mkdir -p "$MOCK_BIN"
for cmd in sh awk sed tr grep mkdir rmdir rm mv cp chmod mktemp tar curl wget shasum sha256sum openssl sleep cat python3 jq dirname basename; do
    if resolved="$(command -v "$cmd" 2>/dev/null)"; then
        ln -sf "$resolved" "$MOCK_BIN/$cmd"
    fi
done
cat >"$MOCK_BIN/uname" <<'EOF'
#!/bin/sh
if [ "${1:-}" = "-s" ]; then printf 'MINGW64_NT-10.0\n'; else printf 'x86_64\n'; fi
EOF
chmod +x "$MOCK_BIN/uname"
if PATH="$MOCK_BIN" HOME="$FRESH_HOME" "$INSTALLER" --dry-run >"$TEST_ROOT/os.out" 2>"$TEST_ROOT/os.err"; then
    fail "installer accepted MINGW64_NT-10.0"
fi
grep -Fq "WSL2" "$TEST_ROOT/os.err" || fail "MINGW error did not mention WSL2 clarity"

cat >"$MOCK_BIN/uname" <<'EOF'
#!/bin/sh
if [ "${1:-}" = "-s" ]; then printf 'Darwin\n'; else printf 'x86_64\n'; fi
EOF
cat >"$MOCK_BIN/sysctl" <<'EOF'
#!/bin/sh
if [ "${2:-}" = "sysctl.proc_translated" ]; then printf '1\n'; else printf '0\n'; fi
EOF
chmod +x "$MOCK_BIN/uname" "$MOCK_BIN/sysctl"
PATH="$MOCK_BIN" HOME="$FRESH_HOME" "$INSTALLER" \
    --manifest-url "$BASE_URL/latest.json" \
    --install-dir "$TEST_ROOT/rosetta-dry" \
    --dry-run >"$TEST_ROOT/rosetta.out" 2>"$TEST_ROOT/rosetta.err"
grep -Fq "aarch64-apple-darwin" "$TEST_ROOT/rosetta.out" \
    || fail "Rosetta 2 translation did not select native aarch64-apple-darwin"
grep -Fq "Rosetta 2" "$TEST_ROOT/rosetta.err" \
    || fail "Rosetta 2 note missing"

# 14. --channel / ALAN_CHANNEL validation & retired ALAN_INSTALL_CHANNEL
for bad_chan in nightly dev foo; do
    if HOME="$FRESH_HOME" "$INSTALLER" --channel "$bad_chan" --dry-run >"$TEST_ROOT/bad-chan.out" 2>"$TEST_ROOT/bad-chan.err"; then
        fail "installer accepted unknown --channel '$bad_chan'"
    fi
    grep -Fq "unknown release channel" "$TEST_ROOT/bad-chan.err" \
        || fail "missing unknown release channel error for --channel '$bad_chan'"

    if ALAN_CHANNEL="$bad_chan" HOME="$FRESH_HOME" "$INSTALLER" --dry-run >"$TEST_ROOT/bad-chan.out" 2>"$TEST_ROOT/bad-chan.err"; then
        fail "installer accepted unknown ALAN_CHANNEL='$bad_chan'"
    fi
    grep -Fq "unknown release channel" "$TEST_ROOT/bad-chan.err" \
        || fail "missing unknown release channel error for ALAN_CHANNEL='$bad_chan'"
done

if ALAN_INSTALL_CHANNEL=beta HOME="$FRESH_HOME" "$INSTALLER" --dry-run >"$TEST_ROOT/ret-beta.out" 2>"$TEST_ROOT/ret-beta.err"; then
    fail "installer accepted retired ALAN_INSTALL_CHANNEL=beta"
fi
grep -Fq "ALAN_INSTALL_CHANNEL" "$TEST_ROOT/ret-beta.err" \
    || fail "missing ALAN_INSTALL_CHANNEL retirement error"

# 15. Beta channel default manifest URL mapping, precedence, and POSIX awk beta parsing
BETA_MOCK_BIN="$TEST_ROOT/beta-mock-bin"
mkdir -p "$BETA_MOCK_BIN"
for cmd in sh awk sed tr grep mkdir rmdir rm mv cp chmod mktemp tar uname wget shasum sha256sum openssl sleep cat python3 jq dirname basename; do
    if resolved="$(command -v "$cmd" 2>/dev/null)"; then
        ln -sf "$resolved" "$BETA_MOCK_BIN/$cmd"
    fi
done
REAL_CURL="$(command -v curl)"
cat >"$BETA_MOCK_BIN/curl" <<EOF
#!/bin/sh
out=""
url=""
prev=""
for arg in "\$@"; do
    case "\$prev" in
        -o) out="\$arg"; prev=""; continue ;;
    esac
    case "\$arg" in
        -o) prev="-o" ;;
        https://*|http://*) url="\$arg" ;;
    esac
done
if [ -n "\${ALAN_TEST_CURL_LOG:-}" ]; then
    printf '%s\n' "\$url" >>"\$ALAN_TEST_CURL_LOG"
fi
case "\$url" in
    https://alanworks.app/beta.json)
        cp "\${ALAN_TEST_BETA_MANIFEST:-$HTTP_ROOT/beta.json}" "\$out"
        exit 0
        ;;
    https://alanworks.app/latest.json)
        cp "$HTTP_ROOT/latest.json" "\$out"
        exit 0
        ;;
esac
exec "$REAL_CURL" "\$@"
EOF
chmod +x "$BETA_MOCK_BIN/curl"

BETA_BIN="$TEST_ROOT/beta-bin"
CURL_LOG="$TEST_ROOT/curl.log"
PATH="$BETA_MOCK_BIN" ALAN_TEST_CURL_LOG="$CURL_LOG" HOME="$FRESH_HOME" "$INSTALLER" \
    --channel beta \
    --install-dir "$BETA_BIN" \
    --no-modify-path >/dev/null
[[ "$("$BETA_BIN/alan" --version)" == "alan $BETA_VERSION" ]] \
    || fail "--channel beta did not install $BETA_VERSION"
[[ "$(cat "$BETA_BIN/.alan-cli-manifest")" == "alan|$(sha256 "$BETA_BIN/alan")" ]] \
    || fail "--channel beta did not preserve .alan-cli-manifest ownership format"
grep -Fq "https://alanworks.app/beta.json" "$CURL_LOG" \
    || fail "--channel beta did not request https://alanworks.app/beta.json"

: >"$CURL_LOG"
PATH="$BETA_MOCK_BIN" ALAN_TEST_CURL_LOG="$CURL_LOG" ALAN_CHANNEL=beta HOME="$FRESH_HOME" "$INSTALLER" \
    --install-dir "$BETA_BIN" \
    --no-modify-path >/dev/null
grep -Fq "https://alanworks.app/beta.json" "$CURL_LOG" \
    || fail "ALAN_CHANNEL=beta did not request https://alanworks.app/beta.json"

# --channel stable overrides ALAN_CHANNEL=beta
: >"$CURL_LOG"
PATH="$BETA_MOCK_BIN" ALAN_TEST_CURL_LOG="$CURL_LOG" ALAN_CHANNEL=beta HOME="$FRESH_HOME" "$INSTALLER" \
    --channel stable \
    --install-dir "$BETA_BIN" \
    --dry-run >/dev/null
grep -Fq "https://alanworks.app/latest.json" "$CURL_LOG" \
    || fail "--channel stable did not override ALAN_CHANNEL=beta"

# Channel mismatch on default endpoint is rejected
if PATH="$BETA_MOCK_BIN" ALAN_TEST_BETA_MANIFEST="$HTTP_ROOT/latest.json" HOME="$FRESH_HOME" "$INSTALLER" \
    --channel beta \
    --install-dir "$BETA_BIN" \
    --dry-run >"$TEST_ROOT/mismatch.out" 2>"$TEST_ROOT/mismatch.err"; then
    fail "installer accepted stable manifest when --channel beta was selected"
fi
grep -Fq "does not match requested channel" "$TEST_ROOT/mismatch.err" \
    || fail "missing channel mismatch error"

# --manifest-url and ALAN_MANIFEST_URL override ALAN_CHANNEL=beta
ALAN_CHANNEL=beta HOME="$FRESH_HOME" "$INSTALLER" \
    --manifest-url "$BASE_URL/latest.json" \
    --install-dir "$BETA_BIN" \
    --no-modify-path >/dev/null
[[ "$("$BETA_BIN/alan" --version)" == "alan 0.1.0" ]] \
    || fail "--manifest-url did not override ALAN_CHANNEL=beta"

ALAN_CHANNEL=beta ALAN_MANIFEST_URL="$BASE_URL/beta.json" HOME="$FRESH_HOME" "$INSTALLER" \
    --install-dir "$BETA_BIN" \
    --no-modify-path >/dev/null
[[ "$("$BETA_BIN/alan" --version)" == "alan $BETA_VERSION" ]] \
    || fail "ALAN_MANIFEST_URL did not install beta"

# Pure-POSIX awk JSON fallback on minified beta manifest
AWK_BETA_BIN="$TEST_ROOT/awk-beta-bin"
PATH="$POSIX_TOOLS" HOME="$FRESH_HOME" "$INSTALLER" \
    --channel beta \
    --manifest-url "$BASE_URL/beta-minified.json" \
    --install-dir "$AWK_BETA_BIN" \
    --no-modify-path >/dev/null
[[ "$("$AWK_BETA_BIN/alan" --version)" == "alan $BETA_VERSION" ]] \
    || fail "POSIX awk fallback failed on minified beta.json"

printf 'Remote installer checks passed (15 scenarios).\n'
