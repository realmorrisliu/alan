#!/usr/bin/env python3
"""Report or retire only marked, stopped Alan temporary service stores."""

import json
import os
from pathlib import Path
import re
import shutil
import stat
import tempfile

from build_artifacts import identity, in_use, locked, open_paths


def inspect_scratch(root, apply=False):
    row = {"path": str(root), "state": "unknown"}
    try:
        root_identity = identity(root)
        if root.stat().st_mode & (stat.S_IWGRP | stat.S_IWOTH):
            raise ValueError("temporary store is writable by another user")
        marker = root / ".alan-scratch.json"
        with locked(marker, create=False) as fd:
            opened = os.fstat(fd)
            receipt = json.loads(os.read(fd, 4097)) if opened.st_size <= 4096 else {}
            if (not isinstance(receipt, dict) or set(receipt) != {"version", "kind", "root", "pid", "identity"}
                    or receipt["version"] != 1 or receipt["kind"] not in ("connection", "package")
                    or receipt["root"] != str(root) or receipt["identity"] != root_identity
                    or type(receipt["pid"]) is not int or not 0 < receipt["pid"] <= 2147483647):
                raise ValueError("missing or invalid temporary-store ownership marker")
            row["kind"] = receipt["kind"]
            allowed = ({"connections.toml", "connections.toml.lock"} if receipt["kind"] == "connection"
                       else {"catalog.json", "store.lock", "revisions", "staging", "leases"})
            allowed.add(".alan-scratch.json")
            for child in root.iterdir():
                staged = (re.fullmatch(r"\.tmp[A-Za-z0-9]{6}", child.name) if receipt["kind"] == "connection"
                          else re.fullmatch(r"catalog-[0-9a-f]{32}\.tmp", child.name))
                if child.is_symlink() or (child.name not in allowed and not staged):
                    raise ValueError("temporary store contains unclassified content")
            try:
                os.kill(receipt["pid"], 0)
            except ProcessLookupError:
                pass
            else:
                raise ValueError("owner PID is live or reused; retain the store")
            if in_use(root, open_paths()):
                raise ValueError("temporary store still has an open consumer")
            current = marker.lstat()
            if (identity(root) != root_identity
                    or (current.st_dev, current.st_ino) != (opened.st_dev, opened.st_ino)):
                raise ValueError("temporary store or marker changed during inspection")
            if apply:
                if not shutil.rmtree.avoids_symlink_attacks:
                    raise ValueError("platform cannot safely remove a generated tree")
                shutil.rmtree(root)
                row["state"] = "retained-or-recreated" if root.exists() else "cleaned"
            else:
                row["state"] = "would-clean"
    except (ValueError, OSError) as error:
        row.update(state="retained", reason=str(error))
    return row


def scratch_outputs(apply=False):
    # Old PID directories have no ownership evidence and are deliberately excluded.
    parent = Path(tempfile.gettempdir()).resolve()
    return [inspect_scratch(root, apply) for root in sorted(parent.glob("alan-service-scratch-*"))]
