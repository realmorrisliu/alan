#!/usr/bin/env python3
"""Safety tests for repository build output leases and ownership receipts."""

import json
from concurrent.futures import ThreadPoolExecutor
import threading
import os
import signal
import time
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import build_artifacts as artifacts


class BuildArtifactTests(unittest.TestCase):
    def setUp(self):
        environment = patch.dict(os.environ, {key: value for key, value in os.environ.items()
                                             if not key.startswith(("CARGO_", "ALAN_", "GIT_"))}, clear=True)
        environment.start()
        self.addCleanup(environment.stop)
        self.temp = tempfile.TemporaryDirectory(prefix="alan-artifacts-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.owner = self.root / "checkout"
        subprocess.run(["git", "init", "-q", str(self.owner)], check=True)
        (self.owner / "Cargo.toml").write_text(
            '[package]\nname="artifact-fixture"\nversion="0.1.0"\n'
            '[workspace]\n[[bin]]\nname="fixture"\npath="main.rs"\n')
        (self.owner / "main.rs").write_text("fn main() {}\n")
        (self.owner / "rust-toolchain.toml").write_text(
            (Path(artifacts.__file__).resolve().parent.parent / "rust-toolchain.toml").read_text())
        self.output = self.root / "output"

    def test_new_output_is_registered_and_reusable_by_its_owner(self):
        with artifacts.build_lease(self.owner, [self.output], "task"):
            (self.output / "result").write_text("compiled")
        with artifacts.build_lease(self.owner, [self.output], "task"):
            self.assertEqual((self.output / "result").read_text(), "compiled")
        receipt = artifacts.read_receipt(artifacts.sidecar(self.output, "json"))
        self.assertEqual(receipt["owner"], str(self.owner))
        self.assertEqual(receipt["identity"], artifacts.identity(self.output))

    def test_nonempty_external_output_is_not_claimed(self):
        self.output.mkdir()
        (self.output / "source.patch").write_text("authored")
        with self.assertRaisesRegex(ValueError, "unowned"):
            with artifacts.build_lease(self.owner, [self.output], "task"):
                self.fail("unowned output was granted")
        self.assertFalse(artifacts.sidecar(self.output, "json").exists())
        self.assertEqual((self.output / "source.patch").read_text(), "authored")

    def test_invalid_xdg_override_still_protects_default_product_stores(self):
        home = self.root / "home"
        for override in ("", "relative-data"):
            with patch.dict(os.environ, HOME=str(home), XDG_DATA_HOME=override):
                for path in (home / ".local/share/Alan", home / ".local/share/Alan/Host Store"):
                    with self.assertRaises(ValueError):
                        with artifacts.build_lease(self.owner, [path], "task"):
                            self.fail("product store was granted as compiler output")
        self.assertFalse(home.exists())

    def test_platform_data_alias_protects_both_actual_and_default_stores(self):
        home = self.root / "home"
        data = self.root / "data"
        data.mkdir()
        alias = self.root / "data-alias"
        alias.symlink_to(data, target_is_directory=True)
        with patch.dict(os.environ, HOME=str(home), XDG_DATA_HOME=str(alias)):
            for path in (data / "Alan", home / ".local/share/Alan"):
                with self.assertRaises(ValueError):
                    artifacts.validate_location(self.owner, path)
        self.assertFalse((data / "Alan").exists())

    def test_different_owner_cannot_take_registered_output(self):
        with artifacts.build_lease(self.owner, [self.output], "task"):
            pass
        other = self.root / "other"
        subprocess.run(["git", "init", "-q", str(other)], check=True)
        with self.assertRaisesRegex(ValueError, "another owner"):
            with artifacts.build_lease(other, [self.output], "task"):
                self.fail("ownership changed")

    def test_inherited_ancestor_guard_does_not_grant_another_owners_output(self):
        other = self.root / "other"
        subprocess.run(["git", "init", "-q", str(other)], check=True)
        parent = self.owner / "target/shared"
        with artifacts.build_lease(other, [parent], "task"):
            pass
        with artifacts.build_lease(self.owner, [parent / "nested"], "task") as descriptors:
            inherited = json.dumps({"owner": str(self.owner), "locks": descriptors})
            with patch.dict(os.environ, ALAN_BUILD_LEASE=inherited):
                with self.assertRaisesRegex(ValueError, "ownership receipt"):
                    with artifacts.build_lease(self.owner, [parent], "checkout"):
                        self.fail("ancestor guard was mistaken for ownership")
        receipt = artifacts.read_receipt(artifacts.sidecar(parent, "json"))
        self.assertEqual(receipt["owner"], str(other))

    def test_replaced_directory_requires_reinspection(self):
        with artifacts.build_lease(self.owner, [self.output], "task"):
            pass
        self.output.rename(self.root / "original")
        self.output.mkdir()
        with self.assertRaisesRegex(ValueError, "replaced"):
            with artifacts.build_lease(self.owner, [self.output], "task"):
                self.fail("replacement was implicitly claimed")

    def test_symlinks_and_source_ancestors_are_rejected(self):
        self.output.symlink_to(self.owner, target_is_directory=True)
        for path in [self.output, self.root, self.owner / ".git" / "output"]:
            with self.subTest(path=path), self.assertRaises(ValueError):
                with artifacts.build_lease(self.owner, [path], "task"):
                    self.fail("unsafe output was granted")

    def test_cleaner_cannot_get_exclusive_access_during_build(self):
        with artifacts.build_lease(self.owner, [self.output], "task"):
            with self.assertRaises(BlockingIOError):
                with artifacts.locked(artifacts.sidecar(self.output, "lock")):
                    self.fail("cleaner acquired an active output")
        with artifacts.locked(artifacts.sidecar(self.output, "lock")):
            pass

    def test_registered_producers_share_access_but_exclude_cleaner(self):
        with artifacts.build_lease(self.owner, [self.output], "task"):
            with artifacts.build_lease(self.owner, [self.output], "task"):
                with self.assertRaises(BlockingIOError):
                    with artifacts.locked(artifacts.sidecar(self.output, "lock")):
                        self.fail("cleaner acquired a shared output")

    def test_first_producers_wait_for_registration_then_share(self):
        registering, release, joined = threading.Event(), threading.Event(), threading.Event()
        original = artifacts.register_output

        def register(*args):
            registering.set()
            self.assertTrue(release.wait(5))
            return original(*args)

        def producer(second=False):
            with artifacts.build_lease(self.owner, [self.output], "task"):
                if second:
                    joined.set()
                else:
                    self.assertTrue(joined.wait(5))

        with patch.object(artifacts, "register_output", side_effect=register):
            with ThreadPoolExecutor(max_workers=2) as pool:
                first = pool.submit(producer)
                try:
                    self.assertTrue(registering.wait(5))
                    second = pool.submit(producer, True)
                    self.assertFalse(joined.wait(0.1))
                finally:
                    release.set()
                second.result(timeout=5)
                first.result(timeout=5)

    def test_nested_admission_waits_until_parent_cleanup_finishes(self):
        self.task_source()
        other = self.root / "linked"
        subprocess.run(["git", "-C", str(self.owner), "worktree", "add", "--detach", "-q",
                        str(other)], check=True)
        record_path, record = self.record()
        scanning, release, entered, finish = (threading.Event() for _ in range(4))
        nested = self.output / "debug/nested"

        def scan():
            scanning.set()
            self.assertTrue(release.wait(5))
            return []

        def producer():
            with artifacts.build_lease(other, [nested], "task"):
                (nested / "active").write_text("keep")
                entered.set()
                self.assertTrue(finish.wait(5))

        with patch.object(artifacts, "open_paths", side_effect=scan):
            with ThreadPoolExecutor(max_workers=2) as pool:
                cleaner = pool.submit(artifacts.clean_output, self.owner, self.output,
                                      record_path, record, True)
                try:
                    self.assertTrue(scanning.wait(5))
                    builder = pool.submit(producer)
                    self.assertFalse(entered.wait(0.1))
                    release.set()
                    self.assertEqual(cleaner.result(timeout=5), "cleaned")
                    self.assertTrue(entered.wait(5))
                    self.assertEqual((nested / "active").read_text(), "keep")
                finally:
                    release.set()
                    finish.set()
                builder.result(timeout=5)

    def test_nested_child_retains_ancestor_lease_after_wrapper_exits(self):
        self.record()
        nested = self.output / "debug/nested"
        with artifacts.build_lease(self.owner, [nested], "task") as descriptors:
            self.assertIn(str(self.output), descriptors)
            child = subprocess.Popen(["cat"], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                                     pass_fds=descriptors.values())
        try:
            with self.assertRaises(BlockingIOError):
                with artifacts.locked(artifacts.sidecar(self.output, "lock")):
                    self.fail("parent cleanup entered while a nested child was alive")
        finally:
            child.communicate(timeout=5)
        with artifacts.locked(artifacts.sidecar(self.output, "lock")):
            pass

    def test_child_holds_lease_after_parent_releases_its_handles(self):
        with artifacts.build_lease(self.owner, [self.output], "task") as descriptors:
            child = subprocess.Popen(["cat"], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                                     pass_fds=descriptors.values())
        try:
            with self.assertRaises(BlockingIOError):
                with artifacts.locked(artifacts.sidecar(self.output, "lock")):
                    self.fail("live child lost its lease")
        finally:
            child.communicate(timeout=5)
        with artifacts.locked(artifacts.sidecar(self.output, "lock")):
            pass

    def test_nested_commands_keep_the_outer_quality_lease(self):
        self.record()
        output = self.output / "debug/quality"
        script = str(Path(artifacts.__file__).resolve())
        runner = ["python3", script, "--owner", str(self.owner), "run",
                  "--target-dir", str(output)]
        check = ("import json, os; value=json.loads(os.environ['ALAN_BUILD_LEASE']); "
                 "[os.fstat(fd) for fd in value['locks'].values()]; "
                 f"assert {str(self.output)!r} in value['locks']")
        subprocess.run(runner + ["--purpose", "quality", "--"] + runner +
                       ["--", "python3", "-c", check], cwd=self.owner, check=True)
        receipt = artifacts.read_receipt(artifacts.sidecar(output, "json"))
        self.assertEqual(receipt["purpose"], "quality")
        with artifacts.locked(artifacts.sidecar(self.output, "lock")):
            pass

    def test_existing_checkout_output_remains_unmanaged(self):
        output = self.owner / "target"
        output.mkdir()
        (output / "debug").mkdir()
        with artifacts.build_lease(self.owner, [output], "checkout"):
            self.assertFalse(artifacts.sidecar(output, "json").exists())
        self.assertEqual(artifacts.registered_outputs(self.owner), {})

    def task_source(self):
        artifacts.git(self.owner, "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                      "commit", "--no-gpg-sign", "--allow-empty", "-m", "fixture")

    def test_failed_disposable_build_retains_evidence_and_retires_only_output(self):
        self.task_source()
        evidence = self.owner / "evidence"
        command = ["python3", "-c",
                   "import os, pathlib; assert os.environ['CARGO_INCREMENTAL'] == '0'; "
                   "p=pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'debug'; p.mkdir(); "
                   "(p/'result').write_text('build'); print('failure details'); raise SystemExit(7)"]
        with patch.object(artifacts, "open_paths", return_value=[]):
            result = artifacts.run_command(self.owner, self.owner, [self.output], "task", command,
                                           self.output, evidence)
        self.assertEqual(result, 7)
        self.assertFalse(self.output.exists())
        self.assertTrue((self.owner / "main.rs").exists())
        self.assertEqual((evidence / "build.log").read_text(), "failure details\n")
        report = json.loads((evidence / "build.json").read_text())
        self.assertEqual(report["exit_code"], 7)
        self.assertEqual(report["source_sha"], artifacts.git(self.owner, "rev-parse", "HEAD"))
        self.assertEqual(report["cleanup"][0]["state"], "cleaned")

    def test_interrupted_disposable_task_keeps_diagnostics_and_output(self):
        self.task_source()
        evidence = self.owner / "evidence"
        with patch.object(artifacts.subprocess, "call", side_effect=KeyboardInterrupt):
            with self.assertRaises(KeyboardInterrupt):
                artifacts.run_command(self.owner, self.owner, [self.output], "task", ["false"],
                                      self.output, evidence)
        self.assertTrue(self.output.exists())
        self.assertTrue((evidence / "build.log").exists())
        self.assertEqual(json.loads((evidence / "build.json").read_text())["state"], "interrupted")
        with self.assertRaisesRegex(ValueError, "outside compiler"):
            artifacts.run_command(self.owner, self.owner, [self.output], "task", ["false"],
                                  self.output, self.output / "evidence")

    def test_abrupt_parent_exit_leaves_child_lease_and_diagnostics(self):
        self.task_source()
        evidence = self.owner / "evidence"
        runner = subprocess.Popen([
            "python3", str(Path(artifacts.__file__).resolve()), "--owner", str(self.owner), "run",
            "--purpose", "task", "--target-dir", str(self.output), "--evidence-dir", str(evidence),
            "--", "python3", "-c", "import os,time; print(os.getpid(), flush=True); time.sleep(60)",
        ], stdout=subprocess.DEVNULL)
        child_pid = None
        try:
            deadline = time.monotonic() + 10
            log = evidence / "build.log"
            while time.monotonic() < deadline:
                if log.exists() and log.read_text().strip():
                    child_pid = int(log.read_text().strip())
                    break
                time.sleep(0.02)
            self.assertIsNotNone(child_pid, "task did not start")
            runner.kill()
            runner.wait(timeout=5)
            path, record = artifacts.registered_outputs(self.owner)[self.output]
            with self.assertRaises(BlockingIOError):
                artifacts.clean_output(self.owner, self.output, path, record, True)
            self.assertTrue(self.output.exists())
            self.assertEqual(json.loads((evidence / "build.json").read_text())["state"], "running")
        finally:
            if runner.poll() is None:
                runner.kill()
                runner.wait(timeout=5)
            if child_pid is not None:
                os.kill(child_pid, signal.SIGTERM)
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    try:
                        with artifacts.locked(artifacts.sidecar(self.output, "lock")):
                            break
                    except BlockingIOError:
                        time.sleep(0.02)

    def record(self):
        with artifacts.build_lease(self.owner, [self.output], "task"):
            (self.output / "debug").mkdir()
            (self.output / "debug/result").write_text("compiled")
        return artifacts.registered_outputs(self.owner)[self.output]

    def test_dry_run_then_scoped_clean_preserves_source_and_other_output(self):
        path, receipt = self.record()
        unrelated = self.root / "other-build"
        unrelated.mkdir()
        (unrelated / "keep").write_text("other task")
        with patch.object(artifacts, "open_paths", return_value=[]):
            self.assertEqual(artifacts.clean_output(self.owner, self.output, path, receipt),
                             "would-clean")
            self.assertTrue((self.output / "debug/result").exists())
            with patch.dict(os.environ, {"CARGO_BUILD_BUILD_DIR": str(unrelated)}):
                self.assertEqual(artifacts.clean_output(self.owner, self.output, path, receipt, True),
                                 "cleaned")
        self.assertEqual((unrelated / "keep").read_text(), "other task")
        self.assertTrue((self.owner / "main.rs").exists())
        self.assertTrue((self.owner / ".git").exists())

    def test_open_consumer_and_unclassified_source_prevent_clean(self):
        path, receipt = self.record()
        with patch.object(artifacts, "open_paths", return_value=[self.output / "debug/result"]):
            with self.assertRaisesRegex(ValueError, "active process"):
                artifacts.clean_output(self.owner, self.output, path, receipt, True)
        (self.output / "source.patch").write_text("uncommitted")
        with self.assertRaisesRegex(ValueError, "unclassified"):
            artifacts.clean_output(self.owner, self.output, path, receipt, True)
        self.assertEqual((self.output / "source.patch").read_text(), "uncommitted")

    def test_native_cargo_lock_and_missing_cache_tag_prevent_cleanup(self):
        path, receipt = self.record()
        with artifacts.locked(self.output / "debug/.cargo-lock"):
            with self.assertRaises(BlockingIOError):
                artifacts.clean_output(self.owner, self.output, path, receipt, True)
        (self.output / "CACHEDIR.TAG").unlink()
        with self.assertRaises(FileNotFoundError):
            artifacts.clean_output(self.owner, self.output, path, receipt, True)
        self.assertTrue((self.output / "debug/result").exists())

    def test_changed_receipt_and_recreated_output_are_not_deleted(self):
        path, receipt = self.record()
        artifacts.write_receipt(artifacts.sidecar(self.output, "json"), receipt | {"purpose": "quality"})
        with self.assertRaisesRegex(ValueError, "receipt changed"):
            artifacts.clean_output(self.owner, self.output, path, receipt, True)
        artifacts.write_receipt(artifacts.sidecar(self.output, "json"), receipt)
        self.output.rename(self.root / "prior-output")
        self.output.mkdir()
        with self.assertRaisesRegex(ValueError, "replaced"):
            artifacts.clean_output(self.owner, self.output, path, receipt, True)
        self.assertTrue(self.output.is_dir())

    def test_cleanup_child_retains_exclusion_after_wrapper_releases_handles(self):
        record_path, record = self.record()
        original = subprocess.run
        children = []

        def run(command, **kwargs):
            if command[:2] != ["cargo", "clean"]:
                return original(command, **kwargs)
            children.append(subprocess.Popen(
                ["cat"], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                pass_fds=kwargs.get("pass_fds", ())))
            return subprocess.CompletedProcess(command, 0)

        try:
            with patch.object(artifacts, "open_paths", return_value=[]), \
                    patch.object(artifacts.subprocess, "run", side_effect=run):
                self.assertEqual(artifacts.clean_output(
                    self.owner, self.output, record_path, record, True), "retained-or-recreated")
            for lock in [artifacts.admission_lock(self.owner),
                         artifacts.sidecar(self.output, "lock")]:
                with self.assertRaises(BlockingIOError):
                    with artifacts.locked(lock):
                        self.fail("live cleanup child lost exclusion")
        finally:
            for child in children:
                child.communicate(timeout=5)

    def test_distinct_configured_intermediate_output_is_reported(self):
        with patch.dict(os.environ, {"CARGO_TARGET_DIR": str(self.output),
                                    "CARGO_BUILD_BUILD_DIR": str(self.root / "intermediate")}):
            self.assertEqual(set(artifacts.output_roots(self.owner)),
                             {self.output, self.root / "intermediate"})

    def test_inventory_does_not_claim_unmanaged_output(self):
        output = self.owner / "target"
        output.mkdir()
        (output / "authored.patch").write_text("keep")
        with patch.object(artifacts, "open_paths", return_value=[]):
            report = artifacts.inventory(self.owner)
        self.assertEqual(next(row["state"] for row in report if row["path"] == str(output)),
                         "unknown")
        self.assertFalse(artifacts.sidecar(output, "json").exists())
        self.assertFalse(artifacts.registry(self.owner).exists())


if __name__ == "__main__":
    unittest.main()
