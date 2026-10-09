#!/usr/bin/env python3
"""Exercise Cargo path resolution and installer artifact selection without a build."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import build_artifacts as artifacts


SCRIPTS = Path(__file__).resolve().parent


class CargoCliOutputTests(unittest.TestCase):
    def setUp(self):
        environment = patch.dict(os.environ, {key: value for key, value in os.environ.items()
                                             if not key.startswith(("CARGO_", "ALAN_", "GIT_"))}, clear=True)
        environment.start()
        self.addCleanup(environment.stop)
        self.temp = tempfile.TemporaryDirectory(prefix="alan-build-output-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.repo = self.root / "checkout"
        (self.repo / "scripts").mkdir(parents=True)
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True)
        for name in ("cargo-cli-output.sh", "install-cli.sh", "install-channel.sh",
                     "assemble-cli-release.sh", "test-standalone-cli-distribution.sh", "build_artifacts.py"):
            shutil.copy2(SCRIPTS / name, self.repo / "scripts" / name)
        (self.repo / "Cargo.toml").write_text(
            '[package]\nname = "output-fixture"\nversion = "0.1.0"\n'
            '[workspace]\n[[bin]]\nname = "alan"\npath = "main.rs"\n'
        )
        (self.repo / "main.rs").write_text("fn main() {}\n")
        shutil.copy2(SCRIPTS.parent / "rust-toolchain.toml", self.repo / "rust-toolchain.toml")
        self.env = {key: value for key, value in os.environ.items()
                    if not key.startswith(("CARGO_", "ALAN_", "GIT_"))}
        self.env["ALAN_CLI_INSTALL_DIR"] = str(self.root / "bin")

    def run_shell(self, script, **env):
        return subprocess.run(["bash", "-euo", "pipefail", "-c", script],
                              cwd=self.repo, env=self.env | env,
                              capture_output=True, text=True)

    def test_cargo_config_and_explicit_output_are_resolved(self):
        (self.repo / ".cargo").mkdir()
        configured = self.root / "configured output"
        (self.repo / ".cargo/config.toml").write_text(
            "[build]\ntarget-dir = " + json.dumps(str(configured)) + "\n")
        command = 'source scripts/cargo-cli-output.sh; alan_cli_target_dir "$PWD"'
        result = self.run_shell(command)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), str(configured))
        result = self.run_shell(command, ALAN_STANDALONE_TARGET_DIR="selected output")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), str(self.repo / "selected output"))

    def mock_cargo(self):
        tools = self.root / "tools"
        tools.mkdir()
        cargo = tools / "cargo"
        cargo.write_text('''#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
root = pathlib.Path(os.environ["TEST_CARGO_FIXTURE_ROOT"])
if args[0] == "metadata":
    print(json.dumps({"target_directory": os.environ.get("CARGO_TARGET_DIR", str(root / "shared-output"))}))
elif args[0] == "build":
    (root / "build-args.json").write_text(json.dumps(args))
    if os.environ.get("TEST_BUILD_FAIL"):
        sys.exit(1)
    target_dir = pathlib.Path(args[args.index("--target-dir") + 1])
    executable = target_dir / "configured-triple" / "debug" / "alan"
    executable.parent.mkdir(parents=True, exist_ok=True)
    executable.write_text("#!/bin/sh\\necho current-build\\n")
    executable.chmod(0o755)
    print(json.dumps({"reason": "compiler-artifact", "target": {
        "name": "alan", "kind": ["bin"]}, "executable": str(executable)}))
else:
    sys.exit(2)
''')
        cargo.chmod(0o755)
        self.env.update(PATH=str(tools) + os.pathsep + self.env["PATH"],
                        TEST_CARGO_FIXTURE_ROOT=str(self.root))

    def test_installer_uses_reported_artifact_not_a_guessed_path(self):
        self.mock_cargo()
        with artifacts.build_lease(self.repo, [self.root / "shared-output"], "checkout"):
            pass
        stale = self.root / "shared-output/release/alan"
        stale.parent.mkdir(parents=True)
        stale.write_text("#!/bin/sh\necho stale-build\n")
        stale.chmod(0o755)
        result = self.run_shell("bash scripts/install-cli.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        installed = self.root / "bin/alan"
        self.assertIn("current-build", installed.read_text())
        args = json.loads((self.root / "build-args.json").read_text())
        self.assertEqual(args[args.index("--target-dir") + 1], str(self.root / "shared-output"))
        self.assertEqual(args[args.index("--manifest-path") + 1], str(self.repo / "Cargo.toml"))
        self.assertFalse((self.repo / "target/standalone-release").exists())

    def test_failed_build_cannot_install_a_stale_artifact(self):
        self.mock_cargo()
        result = self.run_shell("bash scripts/install-cli.sh", TEST_BUILD_FAIL="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / "bin/alan").exists())

    def test_skip_build_requires_explicit_verified_source(self):
        self.mock_cargo()
        result = self.run_shell("bash scripts/install-cli.sh", ALAN_SKIP_BUILD="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("ALAN_CLI_SOURCE", result.stderr)
        self.assertFalse((self.root / "bin/alan").exists())

    def test_distribution_contract_with_reported_build_artifacts(self):
        self.mock_cargo()
        result = self.run_shell("bash scripts/test-standalone-cli-distribution.sh",
                                ALAN_RELEASE_VERSION="fixture", ALAN_TARGET="configured-triple")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("Standalone CLI distribution checks passed", result.stdout)
        args = json.loads((self.root / "build-args.json").read_text())
        self.assertIn("--release", args)
        self.assertEqual(args[args.index("--target") + 1], "configured-triple")


if __name__ == "__main__":
    unittest.main()
