## 1. Prerequisites

- [ ] 1.1 aP protocol + namespace with `/proc` available
  (`define-plan9-kernel-substrate`).

## 2. Crate skeleton

- [ ] 2.1 Add `alan-shell` depending only on `alan-ap` (no server/backend deps).
- [ ] 2.2 Add a `dependency_boundary` test enforcing the aP-only rule.

## 3. Generic builtins

- [ ] 3.1 Implement list/walk, `cat` (open+read), `echo >`/write, `tail`
  (blocking watch from an offset), and `spawn`.
- [ ] 3.2 Route control to `ctl` writes; add no agent-specific command or
  `attach` sugar.

## 4. Stdio driver and concurrency

- [ ] 4.1 Implement a line-oriented stdio read-eval-print loop.
- [ ] 4.2 Run stream tailing and stdin as independent tasks so streamed output
  prints while input is still accepted.

## 5. Milestone demos

- [ ] 5.1 M1: against an echo file server, type input and see it echoed back
  through files (no LLM).
- [ ] 5.2 M2: against `alan-agentfs` + `alan-llmfs`, talk to a real agent and see
  the streamed response — using only generic builtins.

## 6. Verification

- [ ] 6.1 Tests for builtins against an in-memory aP file server.
- [ ] 6.2 Run `just verify`.
- [ ] 6.3 Run `openspec validate introduce-alan-shell --strict`.
