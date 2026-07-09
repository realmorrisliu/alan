## Why

`alan-shell` already proves the file-native client model, but `alan-terminal-ui`
still starts by creating or attaching to daemon sessions and reducing
`EventEnvelope` streams. That leaves the primary terminal client on the retired
compatibility boundary even after the namespace-native runtime, `/proc`, `/agent`,
and `/mnt/llm` surfaces are in place.

## What Changes

- Add a first file-backed `alan-terminal-ui` path that renders over
  `alan-shell`/aP instead of starting from daemon session APIs.
- Keep the current daemon-backed TUI path as a compatibility path during
  migration instead of treating it as the durable contract.
- Expose the minimal runtime/bootstrap wiring needed for the `alan` binary to
  launch a local namespace-native agent surface and hand that surface to the
  Ratatui renderer.
- Retire the TUI's private session-first framing as the architectural default:
  the renderer host reads files and writes `ctl`, while daemon/session APIs
  remain an adapter path for transition and remote attachment.

## Capabilities

### New Capabilities
- `alan-renderer-host-contract`: durable contract for renderer hosts that render
  from Alan OS file surfaces and translate user input into file writes and `ctl`.

### Modified Capabilities
- `rust-inline-tui`: the Rust terminal UI no longer treats daemon-backed session
  APIs as its terminal contract; file-backed rendering becomes the target, with
  daemon-backed operation retained only as a compatibility path during migration.

## Impact

- Affected code: `crates/tui`, `crates/shell`, `crates/alan`, and
  `crates/agent-engine` runtime bootstrap/exposure code.
- Affected behavior: bare/local terminal rendering gains a namespace-native path
  over `/proc`, `/agent`, and `/mnt/llm`; existing daemon-backed behavior stays
  available during migration.
- Affected tests/specs: `rust-inline-tui` requirements and new renderer-host
  contract coverage, plus focused tests for the file-backed Ratatui path.
