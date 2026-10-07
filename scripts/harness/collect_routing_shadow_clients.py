#!/usr/bin/env python3
"""Collect ordinary-input evidence through native clients; pending parity stays open.

Build `cargo build -p alan --example shadow_client_fixture`. Supply the evaluator
key only in the child environment, then pass --binary and a new --output directory.
This collector never interprets corpus input as a host shell command.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import select
import struct
import subprocess
import termios
import time

ROOT = Path(__file__).resolve().parents[2]
PLAN = ROOT / "openspec/changes/qualify-agent-input-routing"


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def collect(binary, case, surface, report):
    command = [str(binary), surface, str(report)]
    if surface == "redirected":
        try:
            result = subprocess.run(command, input=case["input"].encode(),
                                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=60)
        except subprocess.TimeoutExpired as error:
            report.with_suffix(".terminal").write_bytes(error.stdout or b"")
            raise
        report.with_suffix(".terminal").write_bytes(result.stdout)
        if result.returncode:
            raise RuntimeError(f"fixture failed: {report.name}; inspect terminal artifact")
    else:
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 32, 100, 0, 0))
        process = subprocess.Popen(command, stdin=slave, stdout=slave, stderr=slave,
                                   env={**os.environ, "TERM": "xterm-256color"})
        os.close(slave)
        transcript = bytearray()
        try:
            deadline = time.monotonic() + 60
            submitted = False
            quitting = False
            cursor_queries = 0
            while time.monotonic() < deadline:
                if select.select([master], [], [], .02)[0]:
                    try:
                        transcript.extend(os.read(master, 65536))
                    except OSError:
                        process.wait(timeout=5)
                        break
                queries = transcript.count(b"\x1b[6n")
                if queries > cursor_queries:
                    # Fresh empty PTY starts at row/column 1; this is admission
                    # evidence, not a visual layout or emulator qualification.
                    os.write(master, b"\x1b[1;1R" * (queries - cursor_queries))
                    cursor_queries = queries
                # The first rendered cursor-show marks an initialized native TUI.
                if not submitted and b"\x1b[?25h" in transcript:
                    os.write(master, b"\x1b[200~" + case["input"].encode() + b"\x1b[201~\r")
                    submitted = True
                if report.exists() and not quitting:
                    os.write(master, b"\x04")
                    quitting = True
                if process.poll() is not None:
                    break
            if process.poll() is None:
                raise TimeoutError(f"native client did not settle: {report.name}")
            if process.returncode:
                raise RuntimeError(f"native fixture failed: {report.name}")
        finally:
            if process.poll() is None:
                process.kill()
            process.wait()
            os.close(master)
            report.with_suffix(".terminal").write_bytes(transcript)
    data = json.loads(report.read_bytes())
    source = ROOT / "crates/alan/examples/shadow_client_fixture.rs"
    if data["fixture_source_sha256"] != digest(source.read_bytes()):
        raise ValueError("executable was built from a different fixture source")
    observation = data["observation"]
    if observation["outcome"]["state"] == "started":
        raise ValueError("evaluation did not settle")
    completion = data["completion"]
    if (completion is None or completion["type"] != "input_completed"
            or observation["identity"]["submission_id"] not in completion["submission_ids"]):
        raise ValueError("uncorrelated client completion")
    body = case["input"][1:] if case["input"].startswith(("!", ":")) else case["input"]
    if observation["identity"]["input_sha256"] != digest(body.encode()):
        raise ValueError("Machine input differs from original client bytes")
    if observation["identity"]["surface"] != surface:
        raise ValueError("wrong admission surface")
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--case", help="Optional single-case capability smoke; not qualification")
    args = parser.parse_args()
    if not os.environ.get("TYPESAFE_API_KEY"):
        parser.error("TYPESAFE_API_KEY must be supplied in the environment")
    corpus_bytes = (PLAN / "shadow-corpus.v1.json").read_bytes()
    budget_bytes = (PLAN / "shadow-budgets.v1.json").read_bytes()
    corpus, budget = json.loads(corpus_bytes), json.loads(budget_bytes)
    cases = [c for c in corpus["cases"] if not c["pending_response"]
             and (args.case is None or c["id"] == args.case)]
    if not cases:
        parser.error("no ordinary-input case selected")
    args.output.mkdir(parents=True, exist_ok=False)
    manifest = {"version": 1, "kind": "ordinary_client_evidence_not_full_qualification",
                "corpus_sha256": digest(corpus_bytes), "budgets_sha256": digest(budget_bytes),
                "binary_sha256": digest(args.binary.read_bytes()),
                "runtime_build_source_verified": False,
                "checkout_commit_at_collection": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "checkout_diff_sha256_at_collection": digest(subprocess.check_output(["git", "diff", "HEAD"], cwd=ROOT)),
                "collector_sha256": digest(Path(__file__).read_bytes()),
                "fixture_source_sha256": digest((ROOT / "crates/alan/examples/shadow_client_fixture.rs").read_bytes()),
                "pending_response": "not collected; redirected client lacks response admission",
                "records": []}
    for repeat in range(1 if args.case else budget["min_repeats"]):
        for case in cases:
            for surface in budget["surfaces"]:
                name = f"{case['id']}-{surface}-{repeat}.json"
                row = {"case_id": case["id"], "surface": surface, "repeat": repeat,
                       "original_input_sha256": digest(case["input"].encode()),
                       "artifact": name, "collection_state": "attempted"}
                manifest["records"].append(row)
                manifest_path = args.output / "manifest.json"
                manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
                try:
                    data = collect(args.binary.resolve(), case, surface, args.output / name)
                    row.update(collection_state="collected", outcome=data["observation"]["outcome"])
                except Exception as error:
                    # Keep the attempted denominator and stop; never silently retry.
                    row.update(collection_state="failed", error_type=type(error).__name__)
                    raise
                finally:
                    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
                print(f"collected {name}", flush=True)


if __name__ == "__main__":
    main()
