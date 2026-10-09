#!/usr/bin/env python3
"""Conservative cleanup of generated service stores, never legacy PID guesses."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import build_artifacts as artifacts
import service_scratch as scratch


class ServiceScratchTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="alan-scratch-test-")
        self.addCleanup(self.temp.cleanup)
        self.parent = Path(self.temp.name).resolve()
        self.root = self.parent / "alan-service-scratch-fixture"
        self.root.mkdir(mode=0o700)
        self.marker = self.root / ".alan-scratch.json"
        self.receipt = {"version": 1, "kind": "connection", "root": str(self.root),
                        "pid": os.getpid(), "identity": artifacts.identity(self.root)}
        self.marker.write_text(json.dumps(self.receipt))
        (self.root / "connections.toml").write_text("generated metadata")
        (self.root / "connections.toml.lock").touch()

    def test_preview_and_apply_require_full_ownership_and_dead_owner(self):
        with patch.object(scratch.os, "kill", side_effect=ProcessLookupError), \
             patch.object(scratch, "open_paths", return_value=[]):
            self.assertEqual(scratch.inspect_scratch(self.root)["state"], "would-clean")
            self.assertTrue(self.marker.exists())
            self.assertEqual(scratch.inspect_scratch(self.root, True)["state"], "cleaned")
        self.assertFalse(self.root.exists())

    def test_live_or_reused_pid_and_inherited_lease_are_retained(self):
        self.assertEqual(scratch.inspect_scratch(self.root, True)["state"], "retained")
        with artifacts.locked(self.marker, exclusive=False) as fd:
            child = subprocess.Popen(["cat"], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                                     pass_fds=[fd])
        try:
            with patch.object(scratch.os, "kill", side_effect=ProcessLookupError):
                self.assertEqual(scratch.inspect_scratch(self.root, True)["state"], "retained")
        finally:
            child.communicate(timeout=5)
        self.assertTrue(self.root.exists())

    def test_missing_marker_symlink_and_replaced_root_are_retained(self):
        self.marker.unlink()
        self.assertEqual(scratch.inspect_scratch(self.root, True)["state"], "retained")
        original = self.parent / "original"
        self.root.rename(original)
        self.root.symlink_to(original, target_is_directory=True)
        self.assertEqual(scratch.inspect_scratch(self.root, True)["state"], "retained")
        self.root.unlink()
        self.root.mkdir()
        self.marker.write_text(json.dumps(self.receipt))
        self.assertEqual(scratch.inspect_scratch(self.root, True)["state"], "retained")
        self.assertTrue((original / "connections.toml").exists())

    def test_open_consumer_and_unknown_marker_never_authorize_deletion(self):
        with patch.object(scratch.os, "kill", side_effect=ProcessLookupError), \
             patch.object(scratch, "open_paths", return_value=[self.root / "connections.toml"]):
            self.assertEqual(scratch.inspect_scratch(self.root, True)["state"], "retained")
        for value in [None, {}, self.receipt | {"version": 2}, self.receipt | {"pid": 0},
                      self.receipt | {"pid": 2**64}, self.receipt | {"kind": "durable"}]:
            self.marker.write_text(json.dumps(value))
            self.assertEqual(scratch.inspect_scratch(self.root, True)["state"], "retained")
        self.assertTrue(self.root.exists())

    def test_authored_patch_in_marked_root_is_retained(self):
        (self.root / "source.patch").write_text("authored")
        with patch.object(scratch.os, "kill", side_effect=ProcessLookupError), \
             patch.object(scratch, "open_paths", return_value=[]):
            self.assertEqual(scratch.inspect_scratch(self.root, True)["state"], "retained")
        self.assertTrue((self.root / "source.patch").exists())

    def test_abrupt_exit_leaves_a_marked_root_that_can_be_retired(self):
        code = ("import fcntl,json,os,pathlib; p=pathlib.Path(os.environ['SCRATCH_MARKER']); "
                "f=p.open('r+'); fcntl.flock(f,fcntl.LOCK_SH); v=json.load(f); "
                "v['pid']=os.getpid(); f.seek(0); json.dump(v,f); f.truncate(); f.flush(); "
                "os._exit(9)")
        result = subprocess.run(["python3", "-c", code], env=os.environ | {"SCRATCH_MARKER": str(self.marker)})
        self.assertEqual(result.returncode, 9)
        with patch.object(scratch, "open_paths", return_value=[]):
            self.assertEqual(scratch.inspect_scratch(self.root, True)["state"], "cleaned")

    def test_sweep_excludes_unmarked_historical_roots(self):
        legacy = self.parent / "alan/builtin-skill-packages/old/123"
        legacy.mkdir(parents=True)
        (legacy / "source.patch").write_text("authored")
        with patch.object(scratch.tempfile, "gettempdir", return_value=str(self.parent)):
            rows = scratch.scratch_outputs(True)
        self.assertEqual(len(rows), 1)
        self.assertTrue((legacy / "source.patch").exists())


if __name__ == "__main__":
    unittest.main()
