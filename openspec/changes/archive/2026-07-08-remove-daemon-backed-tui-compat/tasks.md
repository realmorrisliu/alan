## 1. Spec And Contract Updates

- [x] 1.1 Add OpenSpec artifacts describing the removal of the daemon-backed TUI compatibility path and its superseded migration assumptions
- [x] 1.2 Sync the canonical `rust-inline-tui` spec so file-backed renderer-host behavior is the only terminal UI contract

## 2. Code Removal

- [x] 2.1 Remove the hidden backend selector and daemon-backed TUI launch path from `crates/alan`
- [x] 2.2 Delete daemon-backed-only `crates/tui` modules, payload adapters, and terminal helpers while keeping file-backed code compiling cleanly
- [x] 2.3 Remove or rewrite tests that only exist to keep the daemon-backed TUI compatibility runner alive

## 3. Verification And Cleanup

- [x] 3.1 Run focused Rust tests covering `alan-terminal-ui`, bare `alan` launch wiring, and file-surface runtime control paths
- [x] 3.2 Run `openspec validate remove-daemon-backed-tui-compat --strict`
- [x] 3.3 After merge, archive this change once the canonical spec is synced and no daemon-backed TUI references remain. Done 2026-07-08: `openspec/specs/rust-inline-tui/spec.md` already carries the merged state ("No daemon-backed TUI mode remains"); grep of `crates/tui/src` finds no daemon-backed path (one cosmetic test name only).
