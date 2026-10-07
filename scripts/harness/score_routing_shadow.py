#!/usr/bin/env python3
"""Score exported shadow observations; never dispatch a case or contact a provider."""
import argparse
import hashlib
import json
import math
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PLAN = ROOT / "openspec/changes/qualify-agent-input-routing"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def decode(raw):
    def object_pairs(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate JSON key: {key}")
            result[key] = value
        return result

    def invalid_constant(value):
        raise ValueError(f"invalid JSON number: {value}")

    return json.loads(raw, object_pairs_hook=object_pairs, parse_constant=invalid_constant)


def number(value):
    return type(value) in (int, float) and math.isfinite(value) and value >= 0


def percentile(values, fraction):
    return sorted(values)[max(0, math.ceil(len(values) * fraction) - 1)]


def score(document, corpus_bytes, budget_bytes):
    corpus = decode(corpus_bytes)
    budgets = decode(budget_bytes)
    if document.get("version") != 1 or corpus["version"] != 1 or budgets["version"] != 1:
        raise ValueError("unsupported schema version")
    for key, raw in [("corpus_sha256", corpus_bytes), ("budgets_sha256", budget_bytes)]:
        if document.get(key) != digest(raw):
            raise ValueError(f"{key} does not match the pre-measurement artifact")
    cases = {case["id"]: case for case in corpus["cases"]}
    if len(cases) != len(corpus["cases"]) or not cases:
        raise ValueError("duplicate or empty corpus")
    expected = {(case, surface, repeat) for case in cases
                for surface in budgets["surfaces"] for repeat in range(budgets["min_repeats"])}
    records = document["records"]
    seen = set()
    for row in records:
        key = (row["case_id"], row["surface"], row["repeat"])
        if type(row["repeat"]) is not int or key not in expected or key in seen:
            raise ValueError(f"duplicate or unexpected observation: {key}")
        seen.add(key)
        if row["input_sha256"] != digest(cases[key[0]]["input"].encode()):
            raise ValueError(f"input changed: {key}")
        if row["route"] not in {"command", "agent", "ambiguous", "response", "cancelled"}:
            raise ValueError(f"invalid route: {key}")
        if row["outcome"] not in {"success", "bypass", "unavailable", "malformed", "timeout", "cancelled"}:
            raise ValueError(f"invalid outcome: {key}")
        if not number(row["elapsed_ms"]):
            raise ValueError(f"missing or invalid elapsed time: {key}")
        for field in ["effects", "evaluator_calls", "fallback_calls"]:
            if type(row[field]) is not int or row[field] < 0:
                raise ValueError(f"missing or invalid {field}: {key}")
        cost = row.get("cost_microusd")
        if cost is not None and not number(cost):
            raise ValueError(f"invalid cost: {key}")
    if seen != expected:
        raise ValueError(f"missing {len(expected - seen)} observations")

    evaluated = [r for r in records if not cases[r["case_id"]]["bypass"]]
    by_class = Counter(cases[r["case_id"]]["expected"] for r in evaluated)
    correct_class = Counter(cases[r["case_id"]]["expected"] for r in evaluated
                            if r["route"] == cases[r["case_id"]]["expected"])
    accuracy = sum(r["route"] == cases[r["case_id"]]["expected"] for r in records) / len(records)
    recall = {label: correct_class[label] / count for label, count in by_class.items()}
    false_execution = sum(r["route"] == "command" and cases[r["case_id"]]["expected"] != "command"
                          for r in records)
    valid_cost = all(number(r.get("cost_microusd")) and r.get("cost_source") in
                     {"provider_billed", "verified_token_pricing"} for r in evaluated)
    mean_cost = sum(r["cost_microusd"] for r in evaluated) / len(evaluated) if valid_cost else None
    timings = [r["elapsed_ms"] for r in evaluated]
    p50, p95 = percentile(timings, .5), percentile(timings, .95)
    success_rate = sum(r["outcome"] == "success" for r in evaluated) / len(evaluated)
    checks = {
        "no_false_execution": false_execution <= budgets["max_false_execution"],
        "accuracy": accuracy >= budgets["min_accuracy"],
        "class_recall": set(recall) == {"command", "agent", "ambiguous"} and
                        all(v >= budgets["min_class_recall"] for v in recall.values()),
        "bypass": all(r["route"] == cases[r["case_id"]]["expected"] and
                      r["outcome"] == "bypass" and r["evaluator_calls"] == 0
                      for r in records if cases[r["case_id"]]["bypass"]),
        "bounded_calls": all(r["evaluator_calls"] <= 1 and
                             (r["outcome"] != "success" or r["evaluator_calls"] == 1)
                             for r in records),
        "failure_never_executes": all(r["route"] != "command" for r in evaluated
                                      if r["outcome"] != "success"),
        "cancelled_stops": all(r["route"] == "cancelled" and r["fallback_calls"] == 0
                               for r in evaluated if r["outcome"] == "cancelled"),
        "shadow_no_fallback_dispatch": all(r["fallback_calls"] == 0 for r in records),
        "success_rate": success_rate >= budgets["min_success_rate"],
        "latency": p50 <= budgets["max_p50_ms"] and p95 <= budgets["max_p95_ms"],
        "cost": mean_cost is not None and mean_cost <= budgets["max_mean_cost_microusd"],
        "no_effects": all(r["effects"] == budgets["max_effects"] for r in records),
    }
    return {"version": 1, "observations": len(records), "false_execution": false_execution,
            "accuracy": accuracy, "class_recall": recall, "success_rate": success_rate,
            "p50_ms": p50, "p95_ms": p95, "mean_cost_microusd": mean_cost,
            "checks": checks, "numeric_pass": all(checks.values()),
            "activation_authorized": False,
            "remaining_gates": ["trusted real-provider and no-effect evidence",
                                "deterministic and generation-only baseline comparison",
                                "review, CI and explicit activation decision"]}


def self_test():
    import copy
    corpus_bytes = (PLAN / "shadow-corpus.v1.json").read_bytes()
    budget_bytes = (PLAN / "shadow-budgets.v1.json").read_bytes()
    corpus, budgets = decode(corpus_bytes), decode(budget_bytes)
    doc = {"version": 1, "corpus_sha256": digest(corpus_bytes),
           "budgets_sha256": digest(budget_bytes), "records": []}
    for case in corpus["cases"]:
        for surface in budgets["surfaces"]:
            for repeat in range(budgets["min_repeats"]):
                doc["records"].append({"case_id": case["id"], "surface": surface, "repeat": repeat,
                    "input_sha256": digest(case["input"].encode()), "route": case["expected"],
                    "outcome": "bypass" if case["bypass"] else "success", "elapsed_ms": 100,
                    "cost_microusd": 10, "cost_source": "verified_token_pricing",
                    "evaluator_calls": int(not case["bypass"]), "fallback_calls": 0, "effects": 0})
    result = score(doc, corpus_bytes, budget_bytes)
    assert result["numeric_pass"] and not result["activation_authorized"]
    bad = copy.deepcopy(doc)
    bad["records"][0]["cost_microusd"] = None
    assert not score(bad, corpus_bytes, budget_bytes)["checks"]["cost"]
    bad = copy.deepcopy(doc)
    discussion = next(r for r in bad["records"] if r["route"] == "agent")
    discussion["route"] = "command"
    assert score(bad, corpus_bytes, budget_bytes)["false_execution"] == 1
    bad = copy.deepcopy(doc)
    bad["records"][0]["effects"] = 1
    assert not score(bad, corpus_bytes, budget_bytes)["numeric_pass"]
    bad = copy.deepcopy(doc)
    cancelled = next(r for r in bad["records"] if r["route"] == "agent")
    cancelled["outcome"] = "cancelled"
    assert not score(bad, corpus_bytes, budget_bytes)["checks"]["cancelled_stops"]
    cancelled["route"] = "cancelled"
    cancelled["fallback_calls"] = 1
    assert not score(bad, corpus_bytes, budget_bytes)["checks"]["cancelled_stops"]
    for raw in ['{"effects":1,"effects":0}', '{"records":[],"records":[]}', '{"x":NaN}']:
        try:
            decode(raw)
        except ValueError:
            pass
        else:
            raise AssertionError("ambiguous JSON evidence accepted")
    for mutate in [lambda d: d["records"].pop(), lambda d: d["records"].append(d["records"][0]),
                   lambda d: d.update(corpus_sha256="wrong")]:
        bad = copy.deepcopy(doc)
        mutate(bad)
        try:
            score(bad, corpus_bytes, budget_bytes)
        except ValueError:
            pass
        else:
            raise AssertionError("invalid evidence accepted")
    print("PASS: scorer fixtures only; no provider or activation qualification claimed")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("observations", nargs="?", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    elif args.observations:
        try:
            report = score(decode(args.observations.read_bytes()),
                           (PLAN / "shadow-corpus.v1.json").read_bytes(),
                           (PLAN / "shadow-budgets.v1.json").read_bytes())
        except (KeyError, TypeError, ValueError) as error:
            parser.error(str(error))
        print(json.dumps(report, indent=2))
        raise SystemExit(0 if report["numeric_pass"] else 1)
    else:
        parser.error("provide exported observations or --self-test")
