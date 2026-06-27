#!/usr/bin/env python3

import json
import sys


def main() -> int:
    result = {
        "passed": True,
        "variant": "with_skill",
        "summary": "Candidate output uses the richer skill-aware workflow.",
        "prompt": sys.argv[1] if len(sys.argv) > 1 else "",
    }
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
