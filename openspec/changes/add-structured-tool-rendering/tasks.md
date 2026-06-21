## 1. Protocol primitives

- [x] 1.1 Define `ToolResultPresentation` enum (`Diff`, `FileContent`, `Command`, `Listing`, `PlainText`) and supporting types (`DiffHunk`, `DiffLine`) in `crates/protocol`
- [x] 1.2 Add optional `title` to `ToolCallStarted` and optional `presentation` to `ToolCallCompleted` (additive, `skip_serializing_if`); update all construction/pattern sites
- [x] 1.3 Serialization round-trip tests for each primitive; absence-by-default tests for old consumers

## 2. Diff vertical slice (end-to-end first)

- [ ] 2.1 Runtime: map edit/write tool outputs to `Diff` and format the start `title`
- [ ] 2.2 Daemon: stream the new fields over the event API (covered by additive serialization; verify pass-through)
- [x] 2.1 Runtime: map edit/write tool outputs to `Diff` and format the start `title` (`tool_presentation.rs`, wired in `tool_orchestrator.rs`)
- [x] 2.2 Daemon: streams the new fields over the event API (additive serialization passes through generically)
- [x] 2.3 TUI: render `Diff` with path header and +/- markers, collapse large output
- [x] 2.4 TUI test: a `Diff` payload renders; preview is ignored when presentation present

## 3. Remaining primitives

- [x] 3.1 Runtime + TUI: `Command` for bash (cmdline, exit_code, stdout/stderr, truncation)
- [x] 3.2 Runtime + TUI: `FileContent` for read (path, lines, truncated)
- [x] 3.3 Runtime + TUI: `Listing` for grep/glob/list_dir (rows)
- [x] 3.4 `PlainText` fallback in TUI; dynamic/MCP/unknown runtime tools return no presentation (preview fallback)

## 4. Title formatting

- [x] 4.1 Runtime title formatters for each built-in tool (`tool_presentation::tool_title`); no tool-arg parsing in the TUI
- [x] 4.2 TUI shows title verbatim; degrades to tool name when absent

## 5. Verification

- [x] 5.1 `just verify` (fmt + lint + test + mock smoke) green across protocol + runtime + tui
- [ ] 5.2 Manual smoke: edit/read/bash/grep each render with the correct primitive in the TUI; dynamic tool falls back to `PlainText`
