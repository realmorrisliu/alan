#!/usr/bin/env python3

import argparse
import subprocess
import sys


def main() -> int:
    parser = argparse.ArgumentParser(description="Initialize an alan skill package.")
    parser.add_argument("path", help="Directory where the skill package should be created")
    parser.add_argument(
        "--template",
        default="inline",
        choices=["inline", "delegate"],
        help="Template kind to initialize",
    )
    args = parser.parse_args()

    command = ["alan", "skills", "init", "--path", args.path, "--template", args.template]
    return subprocess.run(command, check=False).returncode


if __name__ == "__main__":
    sys.exit(main())
