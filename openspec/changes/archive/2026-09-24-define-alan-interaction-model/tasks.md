# Tasks: shell-like terminal interaction

## 1. Replan the retained change

- [x] 1.1 Trace the current `alan` → Root Agent → file-backed renderer flow and
  inspect the live terminal UX.
- [x] 1.2 Replace the superseded desktop/background-dispatch proposal, design,
  and deltas with the inline-REPL scope; do not sync unimplemented behavior.
- [x] 1.3 Record model discovery/selection as a follow-up to the owning
  Connection/Machine boundary; a display-only picker is not acceptable.

## 2. Implement the inline REPL

- [x] 2.1 Replace the full-height layout with an inline transcript followed by
  an `alan >` prompt; preserve committed terminal scrollback and recent output.
- [x] 2.2 Keep slash, `$`, and `@` candidate lists temporary and adjacent to the
  prompt; preserve keyboard selection and multiline input.
- [x] 2.3 Ensure task errors and interruption outcomes are visible and terminal
  state is restored on quit/EOF. Do not alter Process or Host ownership.

## 3. Verify and deliver

- [x] 3.1 Add focused tests for inline layout, transcript/scrollback behavior,
  completion, physical-row accounting in narrow panes, long-composer cursor
  visibility, inline viewport reflow/bounds, Unicode/paste, errors, Ctrl-C and
  Ctrl-D detach.
- [x] 3.2 Run the current build in an ordinary PTY and dedicated Herdr pane;
  verify a successful response, transient candidates, long-output scrollback,
  input preservation across live resize, exit/reattach, shell restoration and
  no replay. Error and interrupt semantics remain covered by focused
  renderer/file-backed tests.
- [x] 3.3 Run focused tests, `just quality`, and strict OpenSpec validation.
- [x] 3.4 Complete current-head review and CI: resolve all PR #931 Codex
  threads, merge PR #931, fix the CI synchronization race in PR #932 and
  verify its full required suite; sync only delivered deltas, archive, and hand
  off model discovery/selection to the cognition owner.
