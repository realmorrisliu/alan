"""Offline regression: failed attempts must remain in the evidence denominator."""
import io
import json
import os
import subprocess
from pathlib import Path
import tempfile
import unittest
from unittest.mock import MagicMock, patch

import collect_routing_shadow_clients as collector


class CollectionEvidenceTest(unittest.TestCase):
    def test_redirected_timeout_retains_terminal_output(self):
        with tempfile.TemporaryDirectory() as temp:
            report = Path(temp) / "case.json"
            process = MagicMock()
            process.poll.return_value = None
            process.stdin = io.BytesIO()
            def start(*args, **kwargs):
                kwargs["stdout"].write(b"failure detail")
                return process
            with patch.object(collector.subprocess, "Popen", side_effect=start), \
                    patch.object(collector.time, "monotonic", side_effect=[0, 1, 61]):
                with self.assertRaises(TimeoutError):
                    collector.collect(Path("unused"), {"input": "pwd"}, "redirected", report)
            process.kill.assert_called_once()
            self.assertEqual(report.with_suffix(".terminal").read_bytes(), b"failure detail")

    def test_unsupported_requires_real_pending_request_evidence(self):
        with tempfile.TemporaryDirectory() as temp:
            report = Path(temp) / "case.json"
            source = collector.ROOT / "crates/alan/examples/shadow_client_fixture.rs"
            data = {"unsupported": True, "client_error": "needs interactive input",
                    "request": {"kind": "structured_input", "status": "pending"},
                    "fixture_source_sha256": collector.digest(source.read_bytes()),
                    "generation_native_sha256":collector.digest((source.parent / "routing_generation/native.rs").read_bytes()),
                    "generation_operation_sha256":collector.digest((source.parent / "routing_generation/operation.rs").read_bytes())}
            report.write_text(json.dumps(data))
            report.with_suffix(".ready").write_bytes(b"ready")
            payloads = []
            def start(*args, **kwargs):
                process = MagicMock()
                process.poll.side_effect = [None, 0, 0, 0]
                process.returncode = 0
                class Input(io.BytesIO):
                    def close(self):
                        if not self.closed:
                            payloads.append(self.getvalue())
                        super().close()
                process.stdin = Input()
                return process
            with patch.object(collector.subprocess, "Popen", side_effect=start):
                case = {"input": "!rm -rf output", "pending_response": True}
                self.assertTrue(collector.collect(Path("unused"), case, "redirected", report)["unsupported"])
                self.assertEqual(payloads[-1], b":Prepare qualification response")
                timing = json.loads(report.with_suffix(".timing.json").read_text())
                self.assertGreaterEqual(timing["elapsed_ms"], 0)
                self.assertEqual(timing["clock"], "time.monotonic")
                self.assertEqual(report.with_suffix(".timing-ack").read_bytes(), b"recorded")
                with self.assertRaises(ValueError):
                    collector.collect(Path("unused"), {"input": "pwd"}, "redirected", report)
                data["request"]["status"] = "answered"
                report.write_text(json.dumps(data))
                with self.assertRaises(ValueError):
                    collector.collect(Path("unused"), case, "redirected", report)

    def test_after_exit_report_cannot_supply_matched_admission_timing(self):
        with tempfile.TemporaryDirectory() as temp:
            report = Path(temp) / "case.json"
            report.write_text("{}")
            report.with_suffix(".ready").write_bytes(b"ready")
            process = MagicMock()
            process.poll.return_value = 0
            process.returncode = 0
            process.stdin = io.BytesIO()
            with patch.object(collector.subprocess, "Popen", return_value=process):
                with self.assertRaisesRegex(ValueError, "missing native admission timing"):
                    collector.collect(Path("unused"), {"input":"pwd"}, "redirected", report)
            self.assertFalse(report.with_suffix(".timing-ack").exists())

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
