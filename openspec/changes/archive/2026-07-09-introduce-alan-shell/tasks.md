## 1. Prerequisites

- [x] 1.1 aP protocol + namespace with `/proc` available
  (`define-plan9-kernel-substrate`).

## 2. Crate skeleton

- [x] 2.1 Add `alan-shell` depending only on `alan-ap` (no server/backend deps).
- [x] 2.2 Add a `dependency_boundary` test enforcing the aP-only rule.

## 3. Generic builtins

- [x] 3.1 Implement list/walk, `cat` (open+read), `echo >`/write, `tail`
  (blocking watch from an offset), and `spawn`.
- [x] 3.2 Route control to `ctl` writes; add no agent-specific command or
  `attach` sugar.

## 4. Stdio driver and concurrency

- [x] 4.1 Implement a line-oriented stdio read-eval-print loop.
  Done 2026-07-02: `alan_shell::StdioDriver` parses line-oriented `ls`, `cat`,
  `echo ... >`, `write`, `tail`, `spawn`, and `exit` commands into generic
  `Shell` builtins over a caller-provided async stdin/stdout pair. Protocol
  errors are printed and the loop keeps accepting input.
- [x] 4.2 Run stream tailing and stdin as independent tasks so streamed output
  prints while input is still accepted.
  Done 2026-07-02: `tail` commands start independent tail tasks that forward
  stream bytes through the driver output channel while the main loop continues
  reading stdin. Regression coverage writes to a tailed stream after tailing has
  started and observes the streamed bytes without blocking further input.

## 5. Milestone demos

- [x] 5.1 M1: against an echo file server, type input and see it echoed back
  through files (no LLM).
- [x] 5.2 M2: against `alan-agentfs` + `alan-llmfs`, talk to a real agent and see
  the streamed response — using only generic builtins.
  Done 2026-07-02: the shell integration test assembles `/proc`, `/agent`, and
  `/mnt/llm`, binds a real `AgentFs` and `LlmFs` mock-backed Connection, then
  runs the stdio driver with only `tail /agent/<pid>/io/output` plus
  `echo ... > /agent/<pid>/io/input`. A file-driven agent loop reads the
  committed `io/input` frame, generates through llmfs `clone`/`data`/`events`,
  writes `io/output`, and the driver prints the streamed response.

## 6. Verification

- [x] 6.1 Tests for builtins against an in-memory aP file server.
- [x] 6.2 Run `just verify`.
  Done 2026-07-02: `just verify` passed after the stdio driver and M2
  integration coverage were added.
- [x] 6.3 Run `openspec validate introduce-alan-shell --strict`.
