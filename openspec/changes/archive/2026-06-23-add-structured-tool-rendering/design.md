## Context

`crates/protocol` defines `Event::ToolCallStarted { id, name, audit }` and `Event::ToolCallCompleted { id, name, success, result_preview, audit }`. `result_preview` is a single human string; `audit` is policy-decision metadata, not tool arguments. The runtime's tool orchestrator produces tool outputs that are flattened into that preview before emission. The TUI (`history.rs`) renders tools as `name status preview`.

The system supports dynamic/client tools (`DynamicToolSpec`) and is expected to integrate MCP, so any model keyed on a closed set of *tool identities* cannot cover tools that appear at runtime.

## Goals / Non-Goals

**Goals:**
- A bounded, open-to-any-tool way to carry rich tool results across protocol → runtime → daemon → TUI.
- Keep tool-argument knowledge out of the TUI.
- Ship incrementally, one primitive at a time, end-to-end.

**Non-Goals:**
- Approval-flow changes (slice C consumes these payloads but is separate).
- Sandbox work (slice D).
- Inventing per-tool typed result structs.

## Decisions

### D1. Presentation-form primitives, not tool-identity types
Define a closed enum of presentation primitives, independent of tool name:
- `Diff { path, hunks }` — edit/write
- `FileContent { path, lines, truncated }` — read/glob
- `Command { cmdline, exit_code, stdout, stderr, truncated }` — bash
- `Listing { rows }` — grep/list_dir
- `PlainText { body }` — universal fallback (dynamic/MCP/unknown)

The TUI implements exactly one renderer per primitive (~5), never per tool (N). Built-in tools map to a primitive in the runtime; dynamic/MCP tools pick one or fall back to `PlainText`.

Alternatives rejected: (a) closed typed-by-tool enum — cannot cover runtime tools, couples protocol to the tool list; (b) fully generic `result: Value` — pushes all modeling debt onto the TUI, which degrades to a JSON viewer.

### D2. Daemon-formatted start title
`ToolCallStarted` gains `title: Option<String>` produced by the runtime/daemon (the layer that understands tool arguments). The TUI displays it verbatim. Raw `args` are deliberately NOT added to the protocol, to avoid forcing the TUI to learn each tool's argument schema.

### D3. Additive, backward-compatible event fields
`ToolCallCompleted` gains an optional `presentation: Option<ToolResultPresentation>` field; `result_preview` stays as the fallback when `presentation` is absent. All new fields use `#[serde(default, skip_serializing_if = "Option::is_none")]` so existing consumers and rollouts are unaffected.

### D4. Vertical-slice rollout
Implement and ship `Diff` across all layers first (highest information value), with the others falling back to `PlainText` until added. Then repeat for `Command`, `FileContent`, `Listing`.

## Risks / Trade-offs

- [Diff payload size for large edits] → carry hunks with context limits and a `truncated` flag; TUI collapses large diffs with an expand affordance.
- [Mapping ambiguity for multiplexed tools] → tools that don't fit a primitive use `PlainText`; mapping lives in the runtime where tool semantics are known.
- [Protocol/runtime/TUI drift during incremental rollout] → optional fields + `PlainText` fallback keep every layer functional at each step; contract tests assert round-trip of each primitive.
- [Truncation hides important output] → always report `truncated` and total counts so the TUI can show "+N more" and the user can expand.

## Migration Plan

Additive protocol fields; no data migration. Roll out per primitive. Rollback is reverting; consumers ignore unknown/absent fields and fall back to `result_preview`.

## Open Questions

- Whether `Diff.hunks` reuses an existing diff representation in the runtime or defines a minimal protocol-level hunk type.
- How MCP tool results advertise a preferred primitive (capability hint vs always `PlainText`).
