## Why

The TUI's tool-rendering ceiling is set by what the daemon emits, not by the renderer. Today `ToolCallStarted` carries only `id`/`name` and `ToolCallCompleted` carries a single flat `result_preview` string, so the TUI cannot show the signature agent-TUI views — a file path for a read, a colored diff for an edit, a command with exit code for bash. Reaching Claude Code / Codex parity therefore requires a cross-layer change: the protocol must carry richer, structured tool data, the runtime/daemon must emit it, and the TUI must render it.

This is **slice B of the four-slice TUI parity program**; it builds on slice A (`refine-rust-tui-experience`) and is in turn used by slice C (`introduce-auto-approve-policy`), whose approval prompts display these structured results.

## What Changes

- **Model tool results by presentation form, not tool identity.** Introduce a small, closed set of presentation primitives decoupled from which tool ran: `Diff`, `FileContent`, `Command`, `Listing`, `PlainText`. Built-in tools map to the right primitive; dynamic/MCP tools choose one or fall back to `PlainText`.
- **Tool title at start.** `ToolCallStarted` gains a daemon-formatted human `title` (e.g. `Read src/foo.rs`, `Bash cargo test`) so the TUI never has to understand any tool's argument schema.
- **Structured completion payload.** `ToolCallCompleted` carries a typed presentation payload (one of the primitives) instead of only a flat preview string; the flat preview remains as a compatibility/fallback.
- **Runtime/daemon emission.** The runtime maps built-in tool outputs to the appropriate primitive and the daemon streams it over the event API.
- **TUI rendering.** The TUI renders each primitive distinctly (diffs with +/- coloring, file content with path + line counts + truncation, commands with cmdline/exit-code/streams, listings as rows), with collapse/expand for large outputs.
- Vertical-slice delivery: land `Diff` end-to-end first, then add the remaining primitives.

## Capabilities

### New Capabilities
- `tool-result-presentation`: the cross-layer contract for presentation-form tool data — the primitive set, the start-title, daemon emission, and TUI rendering of each primitive.

### Modified Capabilities
- (none — additive event fields are specified within the new capability; existing event scenarios are unaffected)

## Impact

- Code: `crates/protocol` (event fields + presentation primitive types), `crates/runtime` (map tool outputs to primitives), `crates/tools` (surface structured outputs), `crates/alan` daemon (stream new fields), `crates/tui` (render primitives).
- Compatibility: new event fields are optional/additive; the existing `result_preview` is retained as fallback so older clients keep working.
- Depends on slice A's typed-cell rendering and live region.
