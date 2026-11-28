#!/usr/bin/env python3
"""Unit and CLI tests for scripts/generate-release-manifest.py."""

from __future__ import annotations

import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

SCRIPTS_DIR = Path(__file__).resolve().parent
SCRIPT_PATH = SCRIPTS_DIR / "generate-release-manifest.py"

SPEC = importlib.util.spec_from_file_location("generate_release_manifest", SCRIPT_PATH)
assert SPEC is not None and SPEC.loader is not None
manifest_mod = importlib.util.module_from_spec(SPEC)
sys.modules["generate_release_manifest"] = manifest_mod
SPEC.loader.exec_module(manifest_mod)


def make_tarball(
    dest: Path,
    *,
    version: str,
    target: str,
    binary_bytes: bytes = b"#!/bin/sh\necho alan\n",
    include_binary: bool = True,
    include_manifest: bool = True,
    manifest_override: dict | None = None,
    extra_entries: dict[str, bytes] | None = None,
) -> tuple[str, int]:
    dest.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(dest, "w:gz") as tar:
        if include_binary:
            info = tarfile.TarInfo("./alan")
            info.size = len(binary_bytes)
            info.mode = 0o755
            tar.addfile(info, io.BytesIO(binary_bytes))
        if include_manifest:
            payload = manifest_override if manifest_override is not None else {
                "product": "alan-cli",
                "version": version,
                "target": target,
                "binaries": ["alan"],
            }
            raw = (json.dumps(payload, indent=2) + "\n").encode("utf-8")
            info = tarfile.TarInfo("./manifest.json")
            info.size = len(raw)
            info.mode = 0o644
            tar.addfile(info, io.BytesIO(raw))
        for name, data in (extra_entries or {}).items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mode = 0o755
            tar.addfile(info, io.BytesIO(data))

    raw_bytes = dest.read_bytes()
    return hashlib.sha256(raw_bytes).hexdigest(), len(raw_bytes)


class ReleaseManifestTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="alan-release-manifest-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.assets_dir = self.root / "assets"
        self.assets_dir.mkdir()

    def populate_all_targets(
        self,
        version: str = "0.1.0",
        *,
        with_sidecars: bool = True,
        with_apple_proofs: bool = True,
        nested: bool = False,
    ) -> dict[str, tuple[str, int]]:
        records: dict[str, tuple[str, int]] = {}
        for idx, target in enumerate(manifest_mod.SUPPORTED_TARGETS):
            base = self.assets_dir / f"artifact-{target}" if nested else self.assets_dir
            base.mkdir(parents=True, exist_ok=True)
            asset_name = f"alan-{version}-{target}.tar.gz"
            archive_path = base / asset_name
            digest, size = make_tarball(
                archive_path,
                version=version,
                target=target,
                binary_bytes=f"binary-{target}-{idx}\n".encode("utf-8"),
            )
            records[target] = (digest, size)
            if with_sidecars:
                (base / f"{asset_name}.sha256").write_text(
                    f"{digest}  {asset_name}\n", encoding="utf-8"
                )
            if with_apple_proofs and target.endswith("-apple-darwin"):
                (base / f"signing-proof-{target}.txt").write_text(
                    "\n".join(
                        [
                            f"binary=/tmp/{target}/alan",
                            "sha256=" + ("a" * 64),
                            "identity=Developer ID Application: Example Corp (TEAM123456)",
                            "signed=true",
                            "notarized=true",
                            "notary_id=11111111-2222-3333-4444-555555555555",
                            "notary_status=Accepted",
                            "codesign_verify:",
                            "/tmp/alan: valid on disk",
                            "/tmp/alan: satisfies its Designated Requirement",
                            "codesign_details:",
                            "Authority=Developer ID Application: Example Corp (TEAM123456)",
                            "",
                        ]
                    ),
                    encoding="utf-8",
                )
                (base / f"notary-{target}.json").write_text(
                    json.dumps(
                        {
                            "id": "11111111-2222-3333-4444-555555555555",
                            "message": "Successfully uploaded file",
                            "status": "Accepted",
                        },
                        indent=2,
                    )
                    + "\n",
                    encoding="utf-8",
                )
        return records

    def test_generates_exact_latest_json_and_checksums_for_all_targets(self) -> None:
        records = self.populate_all_targets(version="0.1.0", nested=True)
        latest_path = self.root / "out" / "latest.json"
        checksums_path = self.root / "out" / "checksums.txt"

        result = subprocess.run(
            [
                sys.executable,
                str(SCRIPT_PATH),
                "--assets-dir",
                str(self.assets_dir),
                "--version",
                "0.1.0",
                "--tag",
                "v0.1.0",
                "--published-at",
                "2026-11-20T00:00:00Z",
                "--verify-archives",
                "--require-apple-notarized",
                "--output-manifest",
                str(latest_path),
                "--output-checksums",
                str(checksums_path),
            ],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

        data = json.loads(latest_path.read_text(encoding="utf-8"))
        self.assertEqual(
            list(data.keys()),
            [
                "schema_version",
                "product",
                "channel",
                "version",
                "tag",
                "published_at",
                "release_url",
                "notes_url",
                "installer_url",
                "targets",
            ],
        )
        self.assertEqual(data["schema_version"], 1)
        self.assertEqual(data["product"], "alan")
        self.assertEqual(data["channel"], "stable")
        self.assertEqual(data["version"], "0.1.0")
        self.assertEqual(data["tag"], "v0.1.0")
        self.assertEqual(data["published_at"], "2026-11-20T00:00:00Z")
        self.assertEqual(
            data["release_url"], "https://github.com/thinksmart-systems/alan/releases/tag/v0.1.0"
        )
        self.assertEqual(
            data["notes_url"], "https://github.com/thinksmart-systems/alan/releases/tag/v0.1.0"
        )
        self.assertEqual(data["installer_url"], "https://alanworks.app/install.sh")
        self.assertEqual(list(data["targets"].keys()), list(manifest_mod.SUPPORTED_TARGETS))

        for target, (digest, size) in records.items():
            entry = data["targets"][target]
            expected_asset = f"alan-0.1.0-{target}.tar.gz"
            self.assertEqual(entry["asset"], expected_asset)
            self.assertEqual(
                entry["url"],
                f"https://github.com/thinksmart-systems/alan/releases/download/v0.1.0/{expected_asset}",
            )
            self.assertEqual(entry["sha256"], digest)
            self.assertEqual(entry["size"], size)
            is_macos = target.endswith("-apple-darwin")
            self.assertIs(entry["signed"], is_macos)
            self.assertIs(entry["notarized"], is_macos)

        expected_checksums = "".join(
            f"{records[t][0]}  alan-0.1.0-{t}.tar.gz\n"
            for t in sorted(manifest_mod.SUPPORTED_TARGETS)
        )
        self.assertEqual(checksums_path.read_text(encoding="utf-8"), expected_checksums)

    def test_deterministic_formatting_across_repeated_runs(self) -> None:
        self.populate_all_targets(version="0.2.3")
        m1, c1 = manifest_mod.build_release_manifest(
            assets_dir=self.assets_dir,
            version="0.2.3",
            published_at="2026-11-20T12:34:56Z",
        )
        m2, c2 = manifest_mod.build_release_manifest(
            assets_dir=self.assets_dir,
            version="0.2.3",
            published_at="2026-11-20T12:34:56Z",
        )
        self.assertEqual(json.dumps(m1, indent=2), json.dumps(m2, indent=2))
        self.assertEqual(c1, c2)

    def test_missing_target_fails(self) -> None:
        self.populate_all_targets(version="0.1.0", with_apple_proofs=False)
        (self.assets_dir / "alan-0.1.0-aarch64-unknown-linux-gnu.tar.gz").unlink()

        result = subprocess.run(
            [
                sys.executable,
                str(SCRIPT_PATH),
                "--assets-dir",
                str(self.assets_dir),
                "--version",
                "0.1.0",
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("aarch64-unknown-linux-gnu", result.stderr)

    def test_sha256_sidecar_mismatch_fails(self) -> None:
        self.populate_all_targets(version="0.1.0", with_apple_proofs=False)
        sidecar = self.assets_dir / "alan-0.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256"
        sidecar.write_text(
            f"{'0' * 64}  alan-0.1.0-x86_64-unknown-linux-gnu.tar.gz\n",
            encoding="utf-8",
        )

        with self.assertRaisesRegex(manifest_mod.ManifestError, "SHA-256 mismatch"):
            manifest_mod.build_release_manifest(assets_dir=self.assets_dir, version="0.1.0")

    def test_sha256_sidecar_wrong_filename_fails(self) -> None:
        records = self.populate_all_targets(version="0.1.0", with_apple_proofs=False)
        digest = records["x86_64-unknown-linux-gnu"][0]
        sidecar = self.assets_dir / "alan-0.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256"
        sidecar.write_text(f"{digest}  wrong-asset.tar.gz\n", encoding="utf-8")

        with self.assertRaisesRegex(manifest_mod.ManifestError, "wrong-asset.tar.gz"):
            manifest_mod.build_release_manifest(assets_dir=self.assets_dir, version="0.1.0")

    def test_version_and_tag_validation(self) -> None:
        self.populate_all_targets(version="0.1.0", with_apple_proofs=False)
        with self.assertRaisesRegex(manifest_mod.ManifestError, "must not start with 'v'"):
            manifest_mod.build_release_manifest(assets_dir=self.assets_dir, version="v0.1.0")
        with self.assertRaisesRegex(manifest_mod.ManifestError, "does not match version"):
            manifest_mod.build_release_manifest(
                assets_dir=self.assets_dir, version="0.1.0", tag="v0.1.1"
            )
        with self.assertRaisesRegex(manifest_mod.ManifestError, "published_at"):
            manifest_mod.build_release_manifest(
                assets_dir=self.assets_dir,
                version="0.1.0",
                published_at="2026-11-20 00:00:00",
            )

    def test_signed_and_notarized_detection_variants(self) -> None:
        self.populate_all_targets(version="0.1.0", with_apple_proofs=False)
        # Raw codesign proof without explicit signed=true line, plus preamble before notary JSON.
        (self.assets_dir / "signing-proof-aarch64-apple-darwin.txt").write_text(
            "/tmp/alan: valid on disk\n"
            "/tmp/alan: satisfies its Designated Requirement\n"
            "Authority=Developer ID Application: Test Org (TEAM999999)\n",
            encoding="utf-8",
        )
        (self.assets_dir / "notary-aarch64-apple-darwin.json").write_text(
            "Conducting pre-submission checks...\n"
            '{"id":"sub-1","status":"Accepted"}\n',
            encoding="utf-8",
        )
        # Ad-hoc proof for x86_64-apple-darwin -> signed=False.
        (self.assets_dir / "signing-proof-x86_64-apple-darwin.txt").write_text(
            "/tmp/alan: valid on disk\nSignature=adhoc\nsigned=false\n",
            encoding="utf-8",
        )

        manifest, _ = manifest_mod.build_release_manifest(
            assets_dir=self.assets_dir, version="0.1.0"
        )
        self.assertTrue(manifest["targets"]["aarch64-apple-darwin"]["signed"])
        self.assertTrue(manifest["targets"]["aarch64-apple-darwin"]["notarized"])
        self.assertFalse(manifest["targets"]["x86_64-apple-darwin"]["signed"])
        self.assertFalse(manifest["targets"]["x86_64-apple-darwin"]["notarized"])

        with self.assertRaisesRegex(manifest_mod.ManifestError, "must be signed and notarized"):
            manifest_mod.build_release_manifest(
                assets_dir=self.assets_dir,
                version="0.1.0",
                require_apple_notarized=True,
            )

        # Explicit conflict with ad-hoc proof fails.
        with self.assertRaisesRegex(manifest_mod.ManifestError, "records unsigned/ad-hoc"):
            manifest_mod.build_release_manifest(
                assets_dir=self.assets_dir,
                version="0.1.0",
                signed_targets=["x86_64-apple-darwin"],
            )

    def test_rejected_notary_status_fails(self) -> None:
        self.populate_all_targets(version="0.1.0", with_apple_proofs=False)
        (self.assets_dir / "notary-aarch64-apple-darwin.json").write_text(
            json.dumps({"id": "bad-1", "status": "Invalid"}),
            encoding="utf-8",
        )
        with self.assertRaisesRegex(manifest_mod.ManifestError, "expected 'Accepted'"):
            manifest_mod.build_release_manifest(assets_dir=self.assets_dir, version="0.1.0")

    def test_signed_notarized_cli_overrides_when_proofs_absent(self) -> None:
        self.populate_all_targets(version="0.1.0", with_apple_proofs=False)
        manifest, _ = manifest_mod.build_release_manifest(
            assets_dir=self.assets_dir,
            version="0.1.0",
            signed_targets=["aarch64-apple-darwin", "x86_64-apple-darwin"],
            notarized_targets=["aarch64-apple-darwin", "x86_64-apple-darwin"],
            require_apple_notarized=True,
        )
        self.assertTrue(manifest["targets"]["aarch64-apple-darwin"]["signed"])
        self.assertTrue(manifest["targets"]["x86_64-apple-darwin"]["notarized"])
        self.assertFalse(manifest["targets"]["x86_64-unknown-linux-gnu"]["signed"])

    def test_verify_archives_rejects_legacy_binaries_and_path_traversal(self) -> None:
        self.populate_all_targets(version="0.1.0", with_apple_proofs=False)
        target = "x86_64-unknown-linux-gnu"
        bad_archive = self.assets_dir / f"alan-0.1.0-{target}.tar.gz"
        digest, _ = make_tarball(
            bad_archive,
            version="0.1.0",
            target=target,
            extra_entries={"./alan-os-host": b"legacy"},
        )
        (self.assets_dir / f"alan-0.1.0-{target}.tar.gz.sha256").write_text(
            f"{digest}  alan-0.1.0-{target}.tar.gz\n", encoding="utf-8"
        )
        with self.assertRaisesRegex(manifest_mod.ManifestError, "forbidden legacy executable"):
            manifest_mod.build_release_manifest(
                assets_dir=self.assets_dir, version="0.1.0", verify_archives=True
            )

        digest, _ = make_tarball(
            bad_archive,
            version="0.1.0",
            target=target,
            extra_entries={"../outside": b"evil"},
        )
        (self.assets_dir / f"alan-0.1.0-{target}.tar.gz.sha256").write_text(
            f"{digest}  alan-0.1.0-{target}.tar.gz\n", encoding="utf-8"
        )
        with self.assertRaisesRegex(manifest_mod.ManifestError, "Unsafe archive entry path"):
            manifest_mod.build_release_manifest(
                assets_dir=self.assets_dir, version="0.1.0", verify_archives=True
            )

        digest, _ = make_tarball(
            bad_archive,
            version="0.1.0",
            target=target,
            manifest_override={
                "product": "alan-cli",
                "version": "0.1.0",
                "target": target,
                "binaries": ["alan", "alan-dev"],
            },
        )
        (self.assets_dir / f"alan-0.1.0-{target}.tar.gz.sha256").write_text(
            f"{digest}  alan-0.1.0-{target}.tar.gz\n", encoding="utf-8"
        )
        with self.assertRaisesRegex(manifest_mod.ManifestError, "binaries="):
            manifest_mod.build_release_manifest(
                assets_dir=self.assets_dir, version="0.1.0", verify_archives=True
            )

        digest, _ = make_tarball(
            bad_archive,
            version="0.1.0",
            target=target,
            include_binary=False,
        )
        (self.assets_dir / f"alan-0.1.0-{target}.tar.gz.sha256").write_text(
            f"{digest}  alan-0.1.0-{target}.tar.gz\n", encoding="utf-8"
        )
        with self.assertRaisesRegex(manifest_mod.ManifestError, "missing ./alan"):
            manifest_mod.build_release_manifest(
                assets_dir=self.assets_dir, version="0.1.0", verify_archives=True
            )

        digest, _ = make_tarball(
            bad_archive,
            version="0.1.0",
            target=target,
            include_manifest=False,
        )
        (self.assets_dir / f"alan-0.1.0-{target}.tar.gz.sha256").write_text(
            f"{digest}  alan-0.1.0-{target}.tar.gz\n", encoding="utf-8"
        )
        with self.assertRaisesRegex(manifest_mod.ManifestError, "missing ./manifest.json"):
            manifest_mod.build_release_manifest(
                assets_dir=self.assets_dir, version="0.1.0", verify_archives=True
            )

    def test_duplicate_nested_asset_fails(self) -> None:
        self.populate_all_targets(version="0.1.0", with_apple_proofs=False, nested=True)
        dup_dir = self.assets_dir / "duplicate"
        dup_dir.mkdir()
        make_tarball(
            dup_dir / "alan-0.1.0-x86_64-unknown-linux-gnu.tar.gz",
            version="0.1.0",
            target="x86_64-unknown-linux-gnu",
        )
        with self.assertRaisesRegex(manifest_mod.ManifestError, "Ambiguous duplicate"):
            manifest_mod.build_release_manifest(assets_dir=self.assets_dir, version="0.1.0")

    def test_generates_beta_manifest_with_commit_and_rolling_tag(self) -> None:
        beta_version = "0.1.6-beta.20261123.42"
        records = self.populate_all_targets(version=beta_version, nested=True)
        latest_path = self.root / "out" / "latest.json"
        checksums_path = self.root / "out" / "checksums.txt"

        result = subprocess.run(
            [
                sys.executable,
                str(SCRIPT_PATH),
                "--assets-dir",
                str(self.assets_dir),
                "--version",
                beta_version,
                "--tag",
                "beta",
                "--channel",
                "beta",
                "--commit",
                "6c1dfd7",
                "--published-at",
                "2026-11-23T00:00:00Z",
                "--verify-archives",
                "--require-apple-notarized",
                "--output-manifest",
                str(latest_path),
                "--output-checksums",
                str(checksums_path),
            ],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

        data = json.loads(latest_path.read_text(encoding="utf-8"))
        self.assertEqual(data["schema_version"], 1)
        self.assertEqual(data["product"], "alan")
        self.assertEqual(data["channel"], "beta")
        self.assertEqual(data["version"], beta_version)
        self.assertEqual(data["tag"], "beta")
        self.assertEqual(data["commit"], "6c1dfd7")
        self.assertEqual(
            data["release_url"], "https://github.com/thinksmart-systems/alan/releases/tag/beta"
        )
        for target, (digest, size) in records.items():
            entry = data["targets"][target]
            expected_asset = f"alan-{beta_version}-{target}.tar.gz"
            self.assertEqual(
                entry["url"],
                f"https://github.com/thinksmart-systems/alan/releases/download/beta/{expected_asset}",
            )
            self.assertEqual(entry["sha256"], digest)
            self.assertEqual(entry["size"], size)

    def test_beta_channel_validation_rules(self) -> None:
        beta_version = "0.1.6-beta.20261123.42"
        self.populate_all_targets(version=beta_version, with_apple_proofs=False)

        # Explicit SemVer tag v<version> is also allowed on beta.
        tagged_manifest, _ = manifest_mod.build_release_manifest(
            assets_dir=self.assets_dir,
            version=beta_version,
            tag=f"v{beta_version}",
            channel="beta",
        )
        self.assertEqual(tagged_manifest["tag"], f"v{beta_version}")
        self.assertNotIn("commit", tagged_manifest)

        # Default tag for beta is "beta".
        default_beta, _ = manifest_mod.build_release_manifest(
            assets_dir=self.assets_dir,
            version=beta_version,
            channel="beta",
        )
        self.assertEqual(default_beta["tag"], "beta")

        # Unknown channel rejected.
        with self.assertRaisesRegex(manifest_mod.ManifestError, "Invalid channel"):
            manifest_mod.build_release_manifest(
                assets_dir=self.assets_dir,
                version=beta_version,
                channel="nightly",
            )

        # Mismatched beta tag rejected.
        with self.assertRaisesRegex(manifest_mod.ManifestError, "must be 'beta' or"):
            manifest_mod.build_release_manifest(
                assets_dir=self.assets_dir,
                version=beta_version,
                tag="v0.1.5",
                channel="beta",
            )

        # Beta version without '-' pre-release suffix rejected.
        self.populate_all_targets(version="0.1.6", with_apple_proofs=False)
        with self.assertRaisesRegex(manifest_mod.ManifestError, "pre-release suffix"):
            manifest_mod.build_release_manifest(
                assets_dir=self.assets_dir,
                version="0.1.6",
                tag="beta",
                channel="beta",
            )


if __name__ == "__main__":
    unittest.main()
