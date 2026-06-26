#!/usr/bin/env python3

import json
import os


def main() -> int:
    result = {
        "passed": True,
        "comparison_mode": os.environ.get("ALAN_SKILL_EVAL_COMPARISON_MODE", ""),
        "delta": "candidate preserved more explicit authoring guidance than the baseline",
    }
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
