## 1. OpenSpec And Contract Alignment

- [x] 1.1 Audit active app-distribution OpenSpec artifacts, release scripts,
  install scripts, cask metadata, and docs, then remove or supersede any
  requirement to embed, sign, link, install, launch, or validate `alan-tui`.
- [x] 1.2 Confirm long-lived specs affected by this change will archive cleanly:
  `rust-inline-tui`, `daemon-api-contract`, and
  `macos-shell-build-test-contract`.
- [x] 1.3 Add migration notes in documentation for the intentional removal of
  `alan chat`, `alan ask`, `alan-tui`, and `ALAN_TUI_PATH`.

## 2. Rust TUI Crate Setup

- [x] 2.1 Add a Rust workspace crate for the TUI as a library crate linked by
  the existing `alan` binary, without adding a shipped `alan-tui` binary target.
- [x] 2.2 Add TUI dependencies such as crossterm, ratatui, terminal snapshot
  support, and vt100-style test utilities using versions consistent with the
  workspace.
- [x] 2.3 Add a minimal `alan_tui::run` entrypoint that initializes terminal
  mode, restores terminal state on exit, and returns structured errors.
- [x] 2.4 Add initial TUI unit/snapshot test scaffolding that runs through Cargo.

## 3. CLI Entrypoint Migration

- [x] 3.1 Change bare `alan` with no subcommand to launch `alan_tui::run` in an
  interactive terminal.
- [x] 3.2 Preserve explicit management subcommands such as daemon, connection,
  workspace, skills, shell, and init.
- [x] 3.3 Remove `alan chat` and `alan ask` from clap definitions, modules,
  help text, docs, and tests.
- [x] 3.4 Remove `ALAN_TUI_PATH` handling and any production code that launches
  or locates a standalone TUI bundle.
- [x] 3.5 Add noninteractive startup behavior that fails clearly without falling
  back to the deleted TUI.

## 4. Daemon Client Contract

- [x] 4.1 Implement daemon URL resolution for default local configuration,
  host-configured URLs, and explicit remote overrides such as `ALAN_AGENTD_URL`.
- [x] 4.2 Implement local daemon health checks, auto-start, readiness wait,
  existing-daemon reuse, and startup failure reporting before any session API
  call.
- [x] 4.3 Add or expose Rust endpoint helpers for session lifecycle, event
  streaming, reconnect snapshot/history reads, submissions, connection queries,
  and skill catalog reads.
- [x] 4.4 Refactor the Rust TUI daemon client to use endpoint helpers instead of
  raw canonical `/api/v1/...` route strings.
- [x] 4.5 Add contract checks or snapshots that detect drift in protocol event
  names and selected daemon payloads used by the Rust TUI.
- [x] 4.6 Preserve public daemon route paths while replacing the TypeScript TUI
  client.

## 5. TUI Application Core

- [x] 5.1 Implement the internal `AppEvent` bus and main event loop selecting
  over terminal input, daemon events, app events, and background completions.
- [x] 5.2 Implement daemon-backed session create/attach after daemon readiness,
  history/reconnect hydration, event streaming, gap detection, and reconnect
  behavior.
- [x] 5.3 Implement protocol submissions for turns, follow-up input, resume
  data, interrupt, rollback, and compaction.
- [x] 5.4 Implement session reducers that translate `EventEnvelope` streams into
  TUI state without leaking raw daemon JSON into default UI.

## 6. Codex-Like Terminal Interaction

- [x] 6.1 Implement terminal mode ownership for raw mode, paste handling,
  keyboard/mouse events, resize events, and terminal restoration on panic or
  normal exit.
- [x] 6.2 Implement typed history cells for assistant text, thinking, tool calls,
  plans, warnings, errors, compaction/memory events, and pending yields.
- [x] 6.3 Implement inline viewport rendering with committed transcript
  insertion into terminal scrollback.
- [x] 6.4 Implement a bottom composer with multiline editing, submit behavior,
  basic slash-command surface, and stable layout under resize.
- [x] 6.5 Implement frame coalescing/rate limiting so streaming deltas redraw
  smoothly without overwhelming the terminal.
- [x] 6.6 Implement pending confirmation, structured input, recoverable error,
  and interrupt states as first-class TUI surfaces.

## 7. TypeScript TUI Removal

- [x] 7.1 Delete `clients/tui` and its Bun/Ink package metadata, generated files,
  tests, and build scripts.
- [x] 7.2 Remove `alan-tui` build, bundle, sign, install, and validation steps
  from `justfile` and release scripts.
- [x] 7.3 Remove TypeScript-TUI-only generated helper requirements while keeping
  daemon contract checks for remaining shipped clients.
- [x] 7.4 Remove stale documentation, examples, shell snippets, and environment
  variable references that point to the deleted TUI.

## 8. macOS Packaging And Shell Integration

- [x] 8.1 Update release app assembly so `Alan.app` embeds only the `alan`
  command-line executable under the supported Resources/bin layout.
- [x] 8.2 Update signing/notarization validation so it verifies the embedded
  `alan` binary and fails if `alan-tui` is present.
- [x] 8.3 Update direct app install and future cask metadata expectations so
  only `alan` is linked or installed.
- [x] 8.4 Update Apple shell launch paths so the default alan terminal tab runs
  bare `alan` and never `alan chat`, `alan ask`, or `alan-tui`.
- [x] 8.5 Update focused shell contract checks to reject legacy TUI launch paths
  and TypeScript/Bun TUI packaging dependencies.

## 9. Verification

- [x] 9.1 Run `cargo fmt --all`.
- [x] 9.2 Run `cargo test -p alan-tui` or the final TUI crate package name.
- [x] 9.3 Run `cargo test -p alan`.
- [x] 9.4 Run TUI daemon-lifecycle checks covering local auto-start,
  existing-daemon reuse, remote override behavior, and startup failure reporting.
- [x] 9.5 Run daemon API contract checks that cover endpoint helpers and payload
  drift for the Rust TUI.
- [x] 9.6 Run Apple shell/package contract checks that cover single-binary
  packaging and bare-`alan` launch.
- [x] 9.7 Run `openspec validate --all --strict`.
- [x] 9.8 Run `git diff --check`.

## 10. Review And Archive Readiness

- [x] 10.1 Review the final diff for accidental fallback paths to the deleted
  TypeScript TUI.
- [x] 10.2 Verify release artifacts and docs present one command-line binary:
  `alan`.
- [x] 10.3 Prepare PR/review notes that call out the intentional breaking
  changes and validation evidence.
- [ ] 10.4 After implementation is merged, sync accepted delta specs into
  `openspec/specs/` and archive this change.
