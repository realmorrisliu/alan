"""Offline regression: failed attempts must remain in the evidence denominator."""
import json
import os
import subprocess
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import collect_routing_shadow_clients as collector


class CollectionEvidenceTest(unittest.TestCase):
    def test_redirected_timeout_retains_terminal_output(self):
        with tempfile.TemporaryDirectory() as temp:
            report = Path(temp) / "case.json"
            error = subprocess.TimeoutExpired("fixture", 60, output=b"failure detail")
            with patch.object(collector.subprocess, "run", side_effect=error):
                with self.assertRaises(subprocess.TimeoutExpired):
                    collector.collect(Path("unused"), {"input": "pwd"}, "redirected", report)
            self.assertEqual(report.with_suffix(".terminal").read_bytes(), b"failure detail")

    def test_failure_retains_attempt_without_retry(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            binary = root / "unused-binary"
            binary.write_bytes(b"fixture")
            output = root / "evidence"
            argv = ["collector", "--binary", str(binary), "--output", str(output),
                    "--case", "route-001"]
            with patch.dict(os.environ, {"TYPESAFE_API_KEY": "offline-unused"}), \
                    patch("sys.argv", argv), \
                    patch.object(collector, "collect", side_effect=TimeoutError) as collect:
                with self.assertRaises(TimeoutError):
                    collector.main()
            collect.assert_called_once()
            manifest = json.loads((output / "manifest.json").read_text())
            self.assertEqual(len(manifest["records"]), 1)
            row = manifest["records"][0]
            self.assertEqual((row["case_id"], row["surface"], row["repeat"]),
                             ("route-001", "interactive", 0))
            self.assertEqual(row["collection_state"], "failed")
            self.assertEqual(row["error_type"], "TimeoutError")
            self.assertNotIn("outcome", row)
            self.assertEqual(len(manifest["checkout_commit_at_collection"]), 40)


if __name__ == "__main__":
    unittest.main()
