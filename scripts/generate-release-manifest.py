#!/usr/bin/env python3
"""Generate and verify Alan multi-platform release manifest (latest.json) and checksums.txt."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import sys
import tarfile
from typing import Iterable

SUPPORTED_TARGETS = (
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
)
SUPPORTED_CHANNELS = ("stable", "beta")

DEFAULT_REPO = "thinksmart-systems/alan"
DEFAULT_INSTALLER_URL = "https://alanworks.app/install.sh"
SHA256_LINE_RE = re.compile(r"^([0-9a-fA-F]{64})(?:[ \t]+\*?(.+))?$")
HEX64_RE = re.compile(r"^[0-9a-f]{64}$")
RFC3339_UTC_RE = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z$")
FORBIDDEN_ARCHIVE_NAMES = {"alan-dev", "alan-os-host", "alan-os-host-dev"}


class ManifestError(ValueError):
    """Raised when release assets, metadata, or manifest inputs are invalid."""


def compute_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while True:
            chunk = handle.read(65536)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()


def parse_csv_targets(raw: str | None, allowed: set[str], flag_name: str) -> set[str]:
    if not raw:
        return set()
    targets: set[str] = set()
    for part in raw.split(","):
        item = part.strip()
        if not item:
            continue
        if item not in allowed:
            raise ManifestError(f"Unknown target {item!r} in {flag_name}")
        targets.add(item)
    return targets


def find_unique_file(root: Path, filename: str) -> Path | None:
    direct = root / filename
    if direct.exists():
        if not direct.is_file() or direct.is_symlink():
            raise ManifestError(f"Expected regular file at {direct}")
        return direct
    matches = sorted(p for p in root.rglob(filename) if p.is_file() and not p.is_symlink())
    if not matches:
        return None
    if len(matches) > 1:
        joined = ", ".join(str(p) for p in matches)
        raise ManifestError(f"Ambiguous duplicate files named {filename!r}: {joined}")
    return matches[0]


def verify_sha256_sidecar(sidecar: Path, asset_name: str, actual_digest: str) -> None:
    lines = [line.strip() for line in sidecar.read_text(encoding="utf-8").splitlines() if line.strip()]
    if len(lines) != 1:
        raise ManifestError(f"Expected exactly 1 checksum line in {sidecar}, got {len(lines)}")
    match = SHA256_LINE_RE.match(lines[0])
    if not match:
        raise ManifestError(f"Malformed SHA-256 sidecar {sidecar}: {lines[0]!r}")
    expected_digest = match.group(1).lower()
    recorded_name = match.group(2)
    if recorded_name is not None:
        recorded_basename = Path(recorded_name.strip()).name
        if recorded_basename != asset_name:
            raise ManifestError(
                f"Sidecar {sidecar} references {recorded_name!r}, expected {asset_name!r}"
            )
    if expected_digest != actual_digest:
        raise ManifestError(
            f"SHA-256 mismatch for {asset_name}: sidecar {expected_digest} != actual {actual_digest}"
        )


def verify_archive_contents(archive_path: Path, version: str, target: str) -> None:
    has_alan_binary = False
    manifest_bytes: bytes | None = None
    try:
        with tarfile.open(archive_path, "r:gz") as tar:
            for member in tar.getmembers():
                raw_name = member.name
                if raw_name in ("", "."):
                    continue
                if raw_name.startswith("/") or ".." in Path(raw_name).parts:
                    raise ManifestError(f"Unsafe archive entry path in {archive_path}: {raw_name!r}")
                normalized = raw_name[2:] if raw_name.startswith("./") else raw_name
                if normalized in ("", "."):
                    continue
                basename = Path(normalized).name
                if basename in FORBIDDEN_ARCHIVE_NAMES:
                    raise ManifestError(
                        f"Archive {archive_path} contains forbidden legacy executable {raw_name!r}"
                    )
                if normalized == "alan":
                    if not member.isreg() or member.size <= 0:
                        raise ManifestError(
                            f"Archive {archive_path} entry {raw_name!r} must be a non-empty regular file"
                        )
                    has_alan_binary = True
                elif normalized == "manifest.json":
                    if not member.isreg():
                        raise ManifestError(
                            f"Archive {archive_path} entry {raw_name!r} must be a regular file"
                        )
                    extracted = tar.extractfile(member)
                    if extracted is None:
                        raise ManifestError(f"Unable to read {raw_name!r} from {archive_path}")
                    manifest_bytes = extracted.read()
    except (tarfile.TarError, OSError) as exc:
        raise ManifestError(f"Invalid release archive {archive_path}: {exc}") from exc

    if not has_alan_binary:
        raise ManifestError(f"Archive {archive_path} is missing ./alan")
    if manifest_bytes is None:
        raise ManifestError(f"Archive {archive_path} is missing ./manifest.json")

    try:
        manifest_data = json.loads(manifest_bytes.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ManifestError(f"Invalid manifest.json in {archive_path}: {exc}") from exc

    if manifest_data.get("binaries") != ["alan"]:
        raise ManifestError(
            f"Archive {archive_path} manifest.json must have binaries=['alan'], got {manifest_data.get('binaries')!r}"
        )
    if manifest_data.get("target") != target:
        raise ManifestError(
            f"Archive {archive_path} manifest.json target {manifest_data.get('target')!r} != {target!r}"
        )
    if manifest_data.get("version") != version:
        raise ManifestError(
            f"Archive {archive_path} manifest.json version {manifest_data.get('version')!r} != {version!r}"
        )


def parse_notary_json(notary_path: Path) -> bool:
    raw = notary_path.read_text(encoding="utf-8", errors="replace")
    start = raw.find("{")
    if start < 0:
        raise ManifestError(f"Notary metadata {notary_path} does not contain JSON")
    try:
        data = json.loads(raw[start:])
    except json.JSONDecodeError as exc:
        raise ManifestError(f"Invalid JSON in {notary_path}: {exc}") from exc
    status = data.get("status")
    if status != "Accepted":
        raise ManifestError(f"Notarization status in {notary_path} is {status!r}, expected 'Accepted'")
    return True


def parse_signing_proof(proof_path: Path) -> tuple[bool | None, bool | None]:
    text = proof_path.read_text(encoding="utf-8", errors="replace")
    signed: bool | None = None
    notarized: bool | None = None
    for line in text.splitlines():
        stripped = line.strip()
        if stripped == "signed=true":
            signed = True
        elif stripped == "signed=false":
            signed = False
        elif stripped == "notarized=true" or stripped == "notary_status=Accepted":
            notarized = True
        elif stripped == "notarized=false":
            if notarized is None:
                notarized = False

    if signed is None:
        if "Signature=adhoc" in text:
            signed = False
        elif (
            "satisfies its Designated Requirement" in text
            and "Authority=Developer ID Application:" in text
        ):
            signed = True
    return signed, notarized


def detect_signing_status(
    assets_dir: Path,
    target: str,
    signed_override: bool,
    notarized_override: bool,
) -> tuple[bool, bool]:
    proof_path = find_unique_file(assets_dir, f"signing-proof-{target}.txt")
    notary_path = find_unique_file(assets_dir, f"notary-{target}.json")

    proof_signed: bool | None = None
    proof_notarized: bool | None = None
    if proof_path is not None:
        proof_signed, proof_notarized = parse_signing_proof(proof_path)

    file_notarized: bool | None = None
    if notary_path is not None:
        file_notarized = parse_notary_json(notary_path)
    elif proof_notarized is not None:
        file_notarized = proof_notarized

    if signed_override and proof_signed is False:
        raise ManifestError(
            f"Target {target} listed in --signed-targets, but {proof_path} records unsigned/ad-hoc"
        )
    if notarized_override and file_notarized is False:
        raise ManifestError(
            f"Target {target} listed in --notarized-targets, but proof metadata records unnotarized"
        )

    signed = bool(proof_signed) or signed_override
    notarized = bool(file_notarized) or notarized_override
    return signed, notarized


def validate_version_and_tag(
    version: str,
    tag: str | None,
    channel: str = "stable",
) -> tuple[str, str]:
    if channel not in SUPPORTED_CHANNELS:
        raise ManifestError(
            f"Invalid channel {channel!r} (expected one of {', '.join(SUPPORTED_CHANNELS)})"
        )
    clean_version = version.strip()
    if not clean_version:
        raise ManifestError("version must not be empty")
    if clean_version.startswith(("v", "V")):
        raise ManifestError(f"version must not start with 'v': {clean_version!r}")
    if any(ch.isspace() for ch in clean_version):
        raise ManifestError(f"version contains whitespace: {clean_version!r}")

    if channel == "stable":
        expected_tag = f"v{clean_version}"
        resolved_tag = expected_tag if tag is None else tag.strip()
        if resolved_tag != expected_tag:
            raise ManifestError(
                f"tag {resolved_tag!r} does not match version {clean_version!r} (expected {expected_tag!r})"
            )
        return clean_version, resolved_tag

    if "-" not in clean_version:
        raise ManifestError(
            f"beta version {clean_version!r} must include a pre-release suffix containing '-'"
        )
    _, pre = clean_version.split("-", 1)
    if not pre:
        raise ManifestError(f"beta version {clean_version!r} has an empty pre-release suffix")
    resolved_tag = "beta" if tag is None else tag.strip()
    if resolved_tag not in ("beta", f"v{clean_version}"):
        raise ManifestError(
            f"beta tag {resolved_tag!r} must be 'beta' or 'v{clean_version}'"
        )
    return clean_version, resolved_tag


def validate_commit(commit: str | None) -> str | None:
    if commit is None:
        return None
    clean_commit = commit.strip()
    if not clean_commit:
        return None
    if any(ch.isspace() for ch in clean_commit):
        raise ManifestError(f"commit contains whitespace: {clean_commit!r}")
    return clean_commit


def validate_published_at(published_at: str | None) -> str:
    if published_at is None:
        return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    value = published_at.strip()
    if not RFC3339_UTC_RE.match(value):
        raise ManifestError(f"published_at must be RFC3339 UTC ending in Z: {value!r}")
    try:
        datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ManifestError(f"Invalid published_at timestamp {value!r}: {exc}") from exc
    return value


def build_release_manifest(
    *,
    assets_dir: Path,
    version: str,
    tag: str | None = None,
    channel: str = "stable",
    commit: str | None = None,
    repo: str = DEFAULT_REPO,
    published_at: str | None = None,
    installer_url: str = DEFAULT_INSTALLER_URL,
    release_url: str | None = None,
    notes_url: str | None = None,
    download_base_url: str | None = None,
    targets: Iterable[str] = SUPPORTED_TARGETS,
    signed_targets: Iterable[str] = (),
    notarized_targets: Iterable[str] = (),
    require_apple_notarized: bool = False,
    verify_archives: bool = False,
) -> tuple[dict, str]:
    if not assets_dir.is_dir():
        raise ManifestError(f"Assets directory does not exist: {assets_dir}")

    clean_version, resolved_tag = validate_version_and_tag(version, tag, channel)
    clean_commit = validate_commit(commit)
    resolved_published_at = validate_published_at(published_at)

    target_list = list(targets)
    if not target_list:
        raise ManifestError("At least one target must be specified")
    if len(set(target_list)) != len(target_list):
        raise ManifestError("Duplicate target in targets list")

    target_set = set(target_list)
    signed_set = set(signed_targets)
    notarized_set = set(notarized_targets)
    unknown_signed = sorted(signed_set - target_set)
    if unknown_signed:
        raise ManifestError(f"Unknown target(s) in signed_targets: {', '.join(unknown_signed)}")
    unknown_notarized = sorted(notarized_set - target_set)
    if unknown_notarized:
        raise ManifestError(f"Unknown target(s) in notarized_targets: {', '.join(unknown_notarized)}")

    base_repo_url = f"https://github.com/{repo.strip('/')}"
    resolved_release_url = release_url or f"{base_repo_url}/releases/tag/{resolved_tag}"
    resolved_notes_url = notes_url or resolved_release_url
    resolved_download_base = (download_base_url or f"{base_repo_url}/releases/download/{resolved_tag}").rstrip("/")

    missing_targets: list[str] = []
    target_records: dict[str, dict] = {}
    checksum_rows: list[tuple[str, str]] = []

    for target in target_list:
        asset_name = f"alan-{clean_version}-{target}.tar.gz"
        asset_path = find_unique_file(assets_dir, asset_name)
        if asset_path is None:
            missing_targets.append(target)
            continue

        size = asset_path.stat().st_size
        if size <= 0:
            raise ManifestError(f"Archive {asset_path} is empty")

        sha256 = compute_sha256(asset_path)
        if not HEX64_RE.match(sha256):
            raise ManifestError(f"Unexpected digest format for {asset_path}: {sha256!r}")

        sidecar = find_unique_file(assets_dir, f"{asset_name}.sha256")
        if sidecar is not None:
            verify_sha256_sidecar(sidecar, asset_name, sha256)

        if verify_archives:
            verify_archive_contents(asset_path, clean_version, target)

        signed, notarized = detect_signing_status(
            assets_dir,
            target,
            signed_override=(target in signed_set),
            notarized_override=(target in notarized_set),
        )

        if require_apple_notarized and target.endswith("-apple-darwin"):
            if not signed or not notarized:
                raise ManifestError(
                    f"macOS target {target} must be signed and notarized (signed={signed}, notarized={notarized})"
                )

        target_records[target] = {
            "asset": asset_name,
            "url": f"{resolved_download_base}/{asset_name}",
            "sha256": sha256,
            "size": size,
            "signed": signed,
            "notarized": notarized,
        }
        checksum_rows.append((asset_name, sha256))

    if missing_targets:
        raise ManifestError(f"Missing release archives for target(s): {', '.join(missing_targets)}")

    manifest = {
        "schema_version": 1,
        "product": "alan",
        "channel": channel,
        "version": clean_version,
        "tag": resolved_tag,
        "published_at": resolved_published_at,
        "release_url": resolved_release_url,
        "notes_url": resolved_notes_url,
        "installer_url": installer_url,
    }
    if clean_commit is not None:
        manifest["commit"] = clean_commit
    manifest["targets"] = target_records

    checksum_rows.sort(key=lambda item: item[0])
    checksums_text = "".join(f"{digest}  {asset}\n" for asset, digest in checksum_rows)
    return manifest, checksums_text


def atomic_write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp_path = path.with_name(f".{path.name}.tmp")
    tmp_path.write_text(content, encoding="utf-8")
    tmp_path.replace(path)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assets-dir", type=Path, required=True, help="Directory with built release assets")
    parser.add_argument("--version", required=True, help="Release version without leading 'v' (e.g. 0.1.0)")
    parser.add_argument("--tag", help="Release tag (defaults to v<version> for stable or 'beta' for beta)")
    parser.add_argument(
        "--channel",
        choices=SUPPORTED_CHANNELS,
        default="stable",
        help="Release channel ('stable' or 'beta', default: stable)",
    )
    parser.add_argument("--commit", help="Optional short Git commit SHA for the release")
    parser.add_argument("--repo", default=DEFAULT_REPO, help=f"GitHub owner/repo (default: {DEFAULT_REPO})")
    parser.add_argument("--published-at", help="RFC3339 UTC timestamp (default: current UTC time)")
    parser.add_argument(
        "--installer-url",
        default=DEFAULT_INSTALLER_URL,
        help=f"Canonical installer URL (default: {DEFAULT_INSTALLER_URL})",
    )
    parser.add_argument("--release-url", help="Override GitHub release URL")
    parser.add_argument("--notes-url", help="Override release notes URL")
    parser.add_argument("--download-base-url", help="Override asset download base URL")
    parser.add_argument(
        "--targets",
        help=f"Comma-separated expected targets (default: {','.join(SUPPORTED_TARGETS)})",
    )
    parser.add_argument("--signed-targets", help="Comma-separated targets known to be Developer ID signed")
    parser.add_argument("--notarized-targets", help="Comma-separated targets known to be Apple notarized")
    parser.add_argument(
        "--require-apple-notarized",
        action="store_true",
        help="Fail unless all *-apple-darwin targets are both signed and notarized",
    )
    parser.add_argument(
        "--verify-archives",
        action="store_true",
        help="Inspect .tar.gz contents for ./alan, ./manifest.json, and absence of legacy/unsafe entries",
    )
    parser.add_argument(
        "--output-manifest",
        "--latest-json",
        dest="output_manifest",
        type=Path,
        help="Output path for latest.json (default: <assets-dir>/latest.json)",
    )
    parser.add_argument(
        "--output-checksums",
        "--checksums",
        dest="output_checksums",
        type=Path,
        help="Output path for checksums.txt (default: <assets-dir>/checksums.txt)",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    target_list = (
        [item.strip() for item in args.targets.split(",") if item.strip()]
        if args.targets
        else list(SUPPORTED_TARGETS)
    )
    target_set = set(target_list)
    signed_set = parse_csv_targets(args.signed_targets, target_set, "--signed-targets")
    notarized_set = parse_csv_targets(args.notarized_targets, target_set, "--notarized-targets")

    manifest, checksums_text = build_release_manifest(
        assets_dir=args.assets_dir,
        version=args.version,
        tag=args.tag,
        channel=args.channel,
        commit=args.commit,
        repo=args.repo,
        published_at=args.published_at,
        installer_url=args.installer_url,
        release_url=args.release_url,
        notes_url=args.notes_url,
        download_base_url=args.download_base_url,
        targets=target_list,
        signed_targets=signed_set,
        notarized_targets=notarized_set,
        require_apple_notarized=args.require_apple_notarized,
        verify_archives=args.verify_archives,
    )

    manifest_path = args.output_manifest or (args.assets_dir / "latest.json")
    checksums_path = args.output_checksums or (args.assets_dir / "checksums.txt")
    atomic_write_text(manifest_path, json.dumps(manifest, indent=2) + "\n")
    atomic_write_text(checksums_path, checksums_text)
    print(f"Wrote {manifest_path} and {checksums_path} ({len(manifest['targets'])} targets)")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        sys.exit(1)
