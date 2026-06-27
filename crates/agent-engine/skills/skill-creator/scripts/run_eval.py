#!/usr/bin/env python3

import argparse
import subprocess
import sys


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run the structured eval manifest for this skill package."
    )
    parser.add_argument(
        "--output-dir",
        help="Optional output directory for eval artifacts",
    )
    args = parser.parse_args()

    command = ["alan", "skills", "eval", "--path", "."]
    if args.output_dir:
        command.extend(["--output-dir", args.output_dir])
    return subprocess.run(command, check=False).returncode


if __name__ == "__main__":
    sys.exit(main())
