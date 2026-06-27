#!/usr/bin/env python3

import json
import os


def main() -> int:
    candidate_artifact = os.environ.get("ALAN_SKILL_EVAL_CANDIDATE_ARTIFACT", "")
    baseline_artifact = os.environ.get("ALAN_SKILL_EVAL_BASELINE_ARTIFACT", "")
    result = {
        "passed": True,
        "score": 1.0,
        "candidate_artifact": candidate_artifact,
        "baseline_artifact": baseline_artifact,
    }
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
