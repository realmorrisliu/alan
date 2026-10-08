#!/usr/bin/env python3
"""Collect native input evidence, retaining unsupported redirected responses.

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


def collect(binary, case, surface, report, baseline="typed"):
    command = [str(binary), surface, str(report)]
    if baseline != "typed":
        command.append("--" + baseline)
    pending = case.get("pending_response", False)
    initial = ":Prepare qualification response" if pending else case["input"]
    if pending:
        command.append("--pending")
    admission_started = None
    admission_elapsed_ms = None
    if surface == "redirected":
        with report.with_suffix(".terminal").open("wb") as terminal:
            process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=terminal,
                                       stderr=subprocess.STDOUT)
            try:
                deadline = time.monotonic() + 60
                while time.monotonic() < deadline and process.poll() is None:
                    if admission_started is None and report.with_suffix(".ready").exists():
                        admission_started = time.monotonic()
                        process.stdin.write(initial.encode())
                        process.stdin.close()
                    if report.exists() and admission_elapsed_ms is None:
                        admission_elapsed_ms = (time.monotonic() - admission_started) * 1000
                        report.with_suffix(".timing-ack").write_bytes(b"recorded")
                    time.sleep(.01)
                if process.poll() is None:
                    raise TimeoutError(f"native client did not settle: {report.name}")
                if process.returncode:
                    raise RuntimeError(f"fixture failed: {report.name}; inspect terminal artifact")
            finally:
                if process.poll() is None:
                    process.kill()
                process.wait()
                if not process.stdin.closed:
                    process.stdin.close()
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
            responded = False
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
                    admission_started = time.monotonic()
                    os.write(master, b"\x1b[200~" + initial.encode() + b"\x1b[201~\r")
                    submitted = True
                if (pending and submitted and not responded
                        and report.with_suffix(".pending.json").exists()
                        and b"QUALIFICATION_RESPONSE" in transcript):
                    admission_started = time.monotonic()
                    os.write(master, b"\x1b[200~" + case["input"].encode() + b"\x1b[201~\r")
                    responded = True
                if report.exists() and not quitting:
                    admission_elapsed_ms = (time.monotonic() - admission_started) * 1000
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
    if admission_elapsed_ms is None:
        raise ValueError("missing native admission timing")
    timing = {"version":1,"elapsed_ms":admission_elapsed_ms,"clock":"time.monotonic",
              "scope":"original input write/EOF through correlated Machine completion report",
              "includes":"client parsing, admission, Machine transition, durability, fixed mock completion and observer polling",
              "excludes":"Host boot, TUI initialization, process exit; not model component latency",
              "baseline":baseline,"pending_response":pending}
    report.with_suffix(".timing.json").write_text(json.dumps(timing, indent=2) + "\n")
    data = json.loads(report.read_bytes())
    source = ROOT / "crates/alan/examples/shadow_client_fixture.rs"
    if data["fixture_source_sha256"] != digest(source.read_bytes()):
        raise ValueError("executable was built from a different fixture source")
    for field, name in [("generation_native_sha256", "native.rs"),
                        ("generation_operation_sha256", "operation.rs")]:
        helper = ROOT / "crates/alan/examples/routing_generation" / name
        if data[field] != digest(helper.read_bytes()):
            raise ValueError("executable was built from different baseline helper source")
    if data.get("unsupported"):
        if not (pending and surface == "redirected"
                and data["client_error"] == "needs interactive input"
                and data["request"]["kind"] == "structured_input"
                and data["request"]["status"] == "pending"):
            raise ValueError("invalid unsupported response evidence")
        return data
    if baseline == "prefix":
        record = data["prefix_record"]
        expected_intent = "command" if case["input"].startswith("!") else "force_agent" if case["input"].startswith(":") else "agent"
        expected_body = case["input"][1:] if case["input"].startswith(("!", ":")) else case["input"]
        if (record["body"] != expected_body or record["intent"] != expected_intent
                or record["submission_id"] not in data["completion"]["submission_ids"]
                or data["observation"] is not None):
            raise ValueError("native prefix baseline changed input or selected an evaluator")
        return data
    observation = data["observation"]
    if observation["outcome"]["state"] == "started":
        raise ValueError("evaluation did not settle")
    completion = data["completion"]
    completion_id = observation["identity"]["submission_id"]
    if pending:
        response = data["response_request"]
        if (completion_id != "response:" + response["request_id"]
                or json.loads(response["body"]) != {"response": case["input"]}
                or response["status"].strip() not in ("answered", "closed")
                or observation["outcome"]["state"] != "bypassed"
                or observation["outcome"]["reason"] != "request_response"
                or observation["outcome"]["evaluator_calls"] != 0):
            raise ValueError("response was not preserved as a literal bypass")
        completion_id = response["prelude_submission_id"]
    if (completion is None or completion["type"] != "input_completed"
            or completion_id not in completion["submission_ids"]):
        raise ValueError("uncorrelated client completion")
    body = case["input"][1:] if case["input"].startswith(("!", ":")) else case["input"]
    if pending:
        body = json.dumps([{"type": "structured", "data": {"response": case["input"]}}],
                          ensure_ascii=False, separators=(",", ":"))
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
    parser.add_argument("--baseline", choices=["typed", "generation", "prefix"], default="typed")
    parser.add_argument("--pending", action="store_true", help="Collect pending-response cases")
    args = parser.parse_args()
    if args.baseline == "typed" and not os.environ.get("TYPESAFE_API_KEY"):
        parser.error("TYPESAFE_API_KEY must be supplied in the environment")
    if args.baseline == "prefix" and args.pending:
        parser.error("prefix baseline has no pending-response path")
    corpus_bytes = (PLAN / "shadow-corpus.v1.json").read_bytes()
    budget_bytes = (PLAN / "shadow-budgets.v1.json").read_bytes()
    corpus, budget = json.loads(corpus_bytes), json.loads(budget_bytes)
    cases = [c for c in corpus["cases"] if (c["id"] == args.case if args.case else c["pending_response"] == args.pending)]
    if not cases:
        parser.error("no corpus case selected")
    args.output.mkdir(parents=True, exist_ok=False)
    manifest = {"version": 1, "kind": "native_client_shadow_matrix", "baseline":args.baseline,
                "corpus_sha256": digest(corpus_bytes), "budgets_sha256": digest(budget_bytes),
                "binary_sha256": digest(args.binary.read_bytes()),
                "runtime_build_source_verified": False,
                "checkout_commit_at_collection": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "checkout_diff_sha256_at_collection": digest(subprocess.check_output(["git", "diff", "HEAD"], cwd=ROOT)),
                "collector_sha256": digest(Path(__file__).read_bytes()),
                "fixture_source_sha256": digest((ROOT / "crates/alan/examples/shadow_client_fixture.rs").read_bytes()),
                "pending_response": "redirected client lacks response admission; unsupported attempts retained",
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
                    data = collect(args.binary.resolve(), case, surface, args.output / name, args.baseline)
                    if data.get("unsupported"):
                        row.update(collection_state="unsupported", outcome={"state": "unavailable"})
                    elif args.baseline == "prefix":
                        row.update(collection_state="collected", outcome={"state":"prefix",
                                   "route":"command" if data["prefix_record"]["intent"] == "command" else "agent"})
                    else:
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
