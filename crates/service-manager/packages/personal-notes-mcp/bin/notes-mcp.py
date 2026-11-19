#!/usr/bin/env python3
"""Confined read-only stdio MCP server for personal Markdown notes.

Implements JSON-RPC 2.0 MCP lifecycle (`initialize`, `notifications/initialized`,
`tools/list`, `tools/call`) with zero ambient host access: every tool requires
an explicitly supplied `NOTES_DIR` environment variable or `notes_dir` argument.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path
from typing import Any

PROTOCOL_VERSION = "2025-03-26"
SERVER_INFO = {"name": "personal-notes-mcp", "version": "0.1.0"}
NOTE_EXTENSIONS = {".md", ".markdown"}
MAX_READ_BYTES = 262_144
DEFAULT_READ_BYTES = 65_536
MAX_SEARCH_FILE_BYTES = 524_288


class ToolInputError(Exception):
    """Domain or path-boundary error returned as MCP `isError: true`."""


def write_message(payload: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(payload, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def jsonrpc_result(req_id: Any, result: Any) -> None:
    write_message({"jsonrpc": "2.0", "id": req_id, "result": result})


def jsonrpc_error(req_id: Any, code: int, message: str) -> None:
    write_message(
        {"jsonrpc": "2.0", "id": req_id, "error": {"code": code, "message": message}}
    )


def tool_success(data: dict[str, Any]) -> dict[str, Any]:
    return {
        "content": [{"type": "text", "text": json.dumps(data, ensure_ascii=False)}],
        "structuredContent": data,
        "isError": False,
    }


def tool_failure(message: str) -> dict[str, Any]:
    return {
        "content": [{"type": "text", "text": message}],
        "isError": True,
    }


def resolve_notes_root(args: dict[str, Any]) -> Path:
    env_dir = os.environ.get("NOTES_DIR", "").strip()
    arg_dir = args.get("notes_dir")
    if arg_dir is not None and not isinstance(arg_dir, str):
        raise ToolInputError("`notes_dir` must be a string path")
    arg_str = arg_dir.strip() if isinstance(arg_dir, str) else ""

    if not env_dir and not arg_str:
        raise ToolInputError(
            "explicit NOTES_DIR env var or notes_dir argument is required (zero ambient host access)"
        )

    env_real: Path | None = None
    if env_dir:
        env_path = Path(env_dir)
        if env_path.is_symlink():
            raise ToolInputError("NOTES_DIR must not be a symlink")
        if not env_path.is_dir():
            raise ToolInputError(f"NOTES_DIR does not exist or is not a directory: {env_dir}")
        env_real = Path(os.path.realpath(env_path))

    if arg_str:
        candidate = Path(arg_str)
        if not candidate.is_absolute() and env_real is not None:
            candidate = env_real / candidate
        elif not candidate.is_absolute():
            candidate = Path(os.getcwd()) / candidate
        if candidate.is_symlink():
            raise ToolInputError("notes_dir must not be a symlink")
        if not candidate.is_dir():
            raise ToolInputError(f"notes_dir does not exist or is not a directory: {arg_str}")
        arg_real = Path(os.path.realpath(candidate))
        if env_real is not None:
            try:
                arg_real.relative_to(env_real)
            except ValueError as exc:
                raise ToolInputError("notes_dir must stay within NOTES_DIR") from exc
        return arg_real

    assert env_real is not None
    return env_real


def is_within_root(root_real: Path, candidate_real: Path) -> bool:
    try:
        candidate_real.relative_to(root_real)
        return True
    except ValueError:
        return False


def is_markdown_file(path: Path) -> bool:
    return path.suffix.lower() in NOTE_EXTENSIONS


def iter_markdown_notes(root_real: Path, start_dir: Path) -> list[tuple[str, Path]]:
    found: list[tuple[str, Path]] = []
    for dirpath, dirnames, filenames in os.walk(start_dir, followlinks=False):
        # Skip hidden directories (.git, .obsidian, etc.) and symlinked dirs.
        dirnames[:] = sorted(
            d
            for d in dirnames
            if not d.startswith(".") and not (Path(dirpath) / d).is_symlink()
        )
        for fname in sorted(filenames):
            if fname.startswith("."):
                continue
            full_path = Path(dirpath) / fname
            if full_path.is_symlink() or not full_path.is_file():
                continue
            if not is_markdown_file(full_path):
                continue
            real_file = Path(os.path.realpath(full_path))
            if not is_within_root(root_real, real_file):
                continue
            rel_posix = real_file.relative_to(root_real).as_posix()
            found.append((rel_posix, real_file))
    found.sort(key=lambda item: item[0])
    return found


def extract_title(path: Path) -> str | None:
    try:
        with path.open("r", encoding="utf-8", errors="replace") as handle:
            for _ in range(16):
                line = handle.readline()
                if not line:
                    break
                stripped = line.strip()
                if stripped.startswith("#"):
                    heading = stripped.lstrip("#").strip()
                    if heading:
                        return heading[:160]
    except OSError:
        return None
    return None


def resolve_relative_subpath(root_real: Path, raw_rel: str, expect_dir: bool) -> Path:
    rel = raw_rel.strip()
    if not rel:
        raise ToolInputError("relative path must not be empty")
    rel_path = Path(rel)
    if rel_path.is_absolute():
        raise ToolInputError("path must be relative to notes_dir")
    if ".." in rel_path.parts:
        raise ToolInputError("path traversal (`..`) is not permitted")

    cursor = root_real
    for part in rel_path.parts:
        if part in ("", "."):
            continue
        cursor = cursor / part
        if cursor.is_symlink():
            raise ToolInputError(f"symlinks are not permitted inside notes path: {rel}")

    if not cursor.exists():
        raise ToolInputError(f"note path not found: {rel}")
    real_target = Path(os.path.realpath(cursor))
    if not is_within_root(root_real, real_target):
        raise ToolInputError("resolved path escapes notes_dir")
    if expect_dir and not real_target.is_dir():
        raise ToolInputError(f"subdir is not a directory: {rel}")
    if not expect_dir and not real_target.is_file():
        raise ToolInputError(f"note path is not a regular file: {rel}")
    return real_target


def handle_list_notes(args: dict[str, Any]) -> dict[str, Any]:
    root_real = resolve_notes_root(args)
    subdir = args.get("subdir")
    start_dir = root_real
    if subdir is not None:
        if not isinstance(subdir, str):
            raise ToolInputError("`subdir` must be a string")
        if subdir.strip():
            start_dir = resolve_relative_subpath(root_real, subdir, expect_dir=True)

    limit_raw = args.get("limit", 100)
    if not isinstance(limit_raw, int) or isinstance(limit_raw, bool) or not (1 <= limit_raw <= 500):
        raise ToolInputError("`limit` must be an integer between 1 and 500")

    all_notes = iter_markdown_notes(root_real, start_dir)
    truncated = len(all_notes) > limit_raw
    selected = all_notes[:limit_raw]
    items = []
    for rel_path, real_file in selected:
        stat = real_file.stat()
        items.append(
            {
                "path": rel_path,
                "size_bytes": stat.st_size,
                "title": extract_title(real_file),
            }
        )
    return tool_success(
        {
            "notes": items,
            "count": len(items),
            "truncated": truncated,
        }
    )


def handle_search_notes(args: dict[str, Any]) -> dict[str, Any]:
    root_real = resolve_notes_root(args)
    query = args.get("query")
    if not isinstance(query, str) or not query.strip():
        raise ToolInputError("`query` must be a non-empty string")
    needle = query.strip().lower()

    limit_raw = args.get("limit", 20)
    if not isinstance(limit_raw, int) or isinstance(limit_raw, bool) or not (1 <= limit_raw <= 100):
        raise ToolInputError("`limit` must be an integer between 1 and 100")

    matches: list[dict[str, Any]] = []
    for rel_path, real_file in iter_markdown_notes(root_real, root_real):
        if len(matches) >= limit_raw:
            break
        if needle in rel_path.lower():
            matches.append({"path": rel_path, "line": 0, "snippet": rel_path})
            continue
        try:
            if real_file.stat().st_size > MAX_SEARCH_FILE_BYTES:
                continue
            text = real_file.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for line_no, line in enumerate(text.splitlines(), start=1):
            if needle in line.lower():
                snippet = line.strip()
                if len(snippet) > 240:
                    snippet = snippet[:239] + "…"
                matches.append({"path": rel_path, "line": line_no, "snippet": snippet})
                break

    return tool_success(
        {
            "query": query.strip(),
            "matches": matches,
            "count": len(matches),
        }
    )


def handle_read_note(args: dict[str, Any]) -> dict[str, Any]:
    root_real = resolve_notes_root(args)
    rel = args.get("path")
    if not isinstance(rel, str):
        raise ToolInputError("`path` must be a relative string path")
    target = resolve_relative_subpath(root_real, rel, expect_dir=False)
    if not is_markdown_file(target):
        raise ToolInputError("only Markdown notes (.md, .markdown) can be read")

    max_bytes = args.get("max_bytes", DEFAULT_READ_BYTES)
    if (
        not isinstance(max_bytes, int)
        or isinstance(max_bytes, bool)
        or not (1 <= max_bytes <= MAX_READ_BYTES)
    ):
        raise ToolInputError(f"`max_bytes` must be an integer between 1 and {MAX_READ_BYTES}")

    total_size = target.stat().st_size
    with target.open("rb") as handle:
        raw = handle.read(max_bytes)
    truncated = total_size > len(raw)
    content = raw.decode("utf-8", errors="replace")
    rel_posix = target.relative_to(root_real).as_posix()
    return tool_success(
        {
            "path": rel_posix,
            "content": content,
            "bytes": len(raw),
            "truncated": truncated,
        }
    )


TOOLS = [
    {
        "name": "list_notes",
        "description": "List Markdown (.md) notes under the explicitly mounted notes directory.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "notes_dir": {
                    "type": "string",
                    "description": "Optional explicit directory path (defaults to NOTES_DIR env var when set).",
                },
                "subdir": {
                    "type": "string",
                    "description": "Optional relative subdirectory inside notes_dir.",
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 500,
                    "description": "Maximum number of notes to return (default 100).",
                },
            },
            "additionalProperties": False,
        },
    },
    {
        "name": "search_notes",
        "description": "Search Markdown notes by case-insensitive keyword across filenames and contents.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query text.",
                },
                "notes_dir": {
                    "type": "string",
                    "description": "Optional explicit directory path (defaults to NOTES_DIR env var when set).",
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 100,
                    "description": "Maximum matching notes to return (default 20).",
                },
            },
            "required": ["query"],
            "additionalProperties": False,
        },
    },
    {
        "name": "read_note",
        "description": "Read a single Markdown note by relative path inside notes_dir.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path to the Markdown note inside notes_dir.",
                },
                "notes_dir": {
                    "type": "string",
                    "description": "Optional explicit directory path (defaults to NOTES_DIR env var when set).",
                },
                "max_bytes": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": MAX_READ_BYTES,
                    "description": "Maximum bytes to read (default 65536).",
                },
            },
            "required": ["path"],
            "additionalProperties": False,
        },
    },
]


def dispatch_request(msg: dict[str, Any]) -> None:
    req_id = msg.get("id")
    method = msg.get("method")
    params = msg.get("params") or {}
    is_notification = "id" not in msg or req_id is None

    if not isinstance(method, str):
        if not is_notification:
            jsonrpc_error(req_id, -32600, "Invalid Request: missing method")
        return

    if is_notification:
        # Handle `notifications/initialized` and any other notifications with no reply.
        return

    if method == "initialize":
        jsonrpc_result(
            req_id,
            {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {}},
                "serverInfo": SERVER_INFO,
            },
        )
        return

    if method == "ping":
        jsonrpc_result(req_id, {})
        return

    if method == "tools/list":
        jsonrpc_result(req_id, {"tools": TOOLS})
        return

    if method == "tools/call":
        if not isinstance(params, dict):
            jsonrpc_error(req_id, -32602, "Invalid params for tools/call")
            return
        name = params.get("name")
        args = params.get("arguments") or {}
        if not isinstance(name, str) or not isinstance(args, dict):
            jsonrpc_error(req_id, -32602, "tools/call requires string name and object arguments")
            return
        try:
            if name == "list_notes":
                jsonrpc_result(req_id, handle_list_notes(args))
            elif name == "search_notes":
                jsonrpc_result(req_id, handle_search_notes(args))
            elif name == "read_note":
                jsonrpc_result(req_id, handle_read_note(args))
            else:
                jsonrpc_result(req_id, tool_failure(f"unknown tool: {name}"))
        except ToolInputError as err:
            jsonrpc_result(req_id, tool_failure(str(err)))
        return

    jsonrpc_error(req_id, -32601, f"Method not found: {method}")


def main() -> int:
    for raw_line in sys.stdin:
        line = raw_line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError as err:
            jsonrpc_error(None, -32700, f"Parse error: {err}")
            continue
        if not isinstance(msg, dict):
            jsonrpc_error(None, -32600, "Invalid Request: expected JSON object")
            continue
        dispatch_request(msg)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
