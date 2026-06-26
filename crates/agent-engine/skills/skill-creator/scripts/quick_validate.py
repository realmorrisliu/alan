#!/usr/bin/env python3

import argparse
import subprocess
import sys


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate an alan skill package.")
    parser.add_argument("path", help="Skill package root to validate")
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Treat warnings as failures",
    )
    args = parser.parse_args()

    command = ["alan", "skills", "validate", "--path", args.path]
    if args.strict:
        command.append("--strict")
    return subprocess.run(command, check=False).returncode


if __name__ == "__main__":
    sys.exit(main())
