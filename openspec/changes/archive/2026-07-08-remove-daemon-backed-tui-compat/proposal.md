## Why

The local file-backed terminal path is now complete enough that keeping the
daemon-backed TUI compatibility path only preserves a second terminal model,
extra payload decoders, and stale migration-only tests. That compatibility
layer now obscures the durable Alan Shell direction instead of reducing risk.

## What Changes

- Remove the hidden TUI backend selector and the daemon-backed TUI launch path
  from bare `alan`.
- Delete daemon-backed-only terminal UI modules, daemon payload decoders, and
  tests that exist only to keep the compatibility runner alive.
- Update the Rust terminal UI contract so mounted agent files are the only TUI
  interaction path, not just the default local mode.
- Call out that this change supersedes the remaining migration-only daemon
  compatibility assumptions from `render-alan-shell-in-terminal-ui`.

## Capabilities

### New Capabilities

### Modified Capabilities
- `rust-inline-tui`: remove the daemon-backed compatibility or remote TUI path
  and make the file-backed renderer-host contract the only terminal UI path.

## Impact

- Affected code: `crates/tui`, `crates/alan`, and daemon-backed TUI-specific
  tests.
- Affected behavior: bare `alan` always launches the file-backed terminal UI;
  the daemon-backed TUI runner and its hidden selector are removed.
- Affected contracts: `rust-inline-tui` no longer preserves a migration-only
  daemon-backed terminal path.
