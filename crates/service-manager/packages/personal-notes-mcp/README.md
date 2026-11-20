# Personal Notes MCP Package (`personal-notes-mcp`)

Example Quartermaster (`q`) `mcp` connector package providing read-only Markdown note tools
(`list_notes`, `search_notes`, `read_note`) over a confined stdio JSON-RPC 2.0 subprocess.

## Zero Ambient Authority

Installing this package grants **zero** Host directory access and exposes **no** tools to Agent
Processes until explicitly referenced. Both `NOTES_DIR` (or explicit `notes_dir` argument) and any
corresponding Host Mount grant must be supplied explicitly by the caller.

## Install

```bash
alan q install crates/service-manager/packages/personal-notes-mcp
```

## Tools Exposed (`mcp_personal_notes_*`)

- `list_notes`: List `.md` / `.markdown` notes inside `NOTES_DIR` (optional `subdir`, `limit`).
- `search_notes`: Case-insensitive keyword search across note names and UTF-8 content.
- `read_note`: Read a bounded UTF-8 Markdown note by relative `path`, rejecting traversal or escaping symlinks.
