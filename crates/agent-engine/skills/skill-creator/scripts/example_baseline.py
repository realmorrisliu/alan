#!/usr/bin/env python3

import json
import sys


def main() -> int:
    result = {
        "passed": False,
        "variant": "without_skill",
        "summary": "Baseline output omits the package-specific workflow.",
        "prompt": sys.argv[1] if len(sys.argv) > 1 else "",
    }
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
