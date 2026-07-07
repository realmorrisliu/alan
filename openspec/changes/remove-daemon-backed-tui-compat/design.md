## Context

`complete-file-backed-tui-parity` finished the local file-backed terminal path:
runtime UI state now projects through `machine/ui/*`, the local renderer host
hydrates directly from mounted agent files, and bare `alan` defaults to that
path. The remaining daemon-backed TUI code is no longer part of the local
contract; it exists only as a hidden compatibility path over daemon sessions.

That leftover path keeps a second terminal event model (`app.rs`, `ui.rs`),
daemon payload decoding logic (`daemon_client.rs`), a hidden backend selector in
`crates/alan`, and tests that only prove the migration layer still compiles.
Keeping those pieces raises maintenance cost and leaves stale OpenSpec language
that suggests the terminal UI still has two durable launch surfaces.

## Goals / Non-Goals

**Goals:**

- Remove the daemon-backed terminal UI launch path and its hidden backend
  selector.
- Delete daemon-backed-only TUI modules and tests so `crates/tui` reflects the
  file-backed renderer-host contract directly.
- Update `rust-inline-tui` so the only TUI contract is mounted file surfaces
  plus `ctl` writes.

**Non-Goals:**

- Do not remove the daemon server, daemon CLI management commands, or daemon API
  contracts used by non-TUI clients.
- Do not redesign remote renderer attachment in this slice; this change only
  removes the old daemon-backed TUI implementation.

## Decisions

### Decision: remove the hidden backend selector instead of keeping a dormant switch

- Why: a hidden switch still preserves an unsupported code path and invites
  accidental drift between the documented contract and runtime behavior.
- Alternative considered: keep `TuiBackend::Daemon` behind an internal flag.
  Rejected because the user explicitly wants the path gone, and the canonical
  terminal contract no longer depends on it.

### Decision: delete daemon-backed-only modules wholesale

- Why: `app.rs`, `ui.rs`, and `daemon_client.rs` implement a separate terminal
  loop whose only caller is the compatibility runner. Removing them is clearer
  than freezing them as dead weight.
- Alternative considered: keep the code but stop exporting it. Rejected because
  the modules would still require maintenance and tests for a path the product
  no longer supports.

### Decision: preserve daemon management surfaces outside the terminal UI

- Why: `alan daemon ...` commands, host config daemon URL resolution, and daemon
  API payloads still belong to the daemon subsystem even after the TUI path is
  removed.
- Alternative considered: remove all `ALAN_AGENTD_URL` or daemon references
  touched by the old TUI path. Rejected because that would broaden the change
  beyond terminal UI cleanup and risk unrelated regressions.

## Risks / Trade-offs

- `[Future remote TUI experiments lose the old implementation]` → If a remote
  terminal client is needed later, rebuild it explicitly on the then-current
  renderer-host contract rather than preserving stale migration code.
- `[Tests or docs still assume daemon-backed TUI structs exist]` → Remove or
  rewrite only the tests that decode TUI-specific daemon payload adapters, and
  update the canonical spec in the same change.
- `[Open in-progress migration artifacts still mention compatibility mode]` →
  Call out the superseded migration assumption in this change and update the
  canonical `rust-inline-tui` contract immediately.

## Migration Plan

1. Update OpenSpec artifacts to remove daemon-backed TUI compatibility from the
   `rust-inline-tui` contract.
2. Remove the backend selector and daemon-backed TUI runner from `crates/alan`
   and `crates/tui`.
3. Delete daemon-backed-only modules and rewrite or remove tests that exist only
   for that runner.
4. Run focused `alan-terminal-ui`, `alan`, and runtime/file-surface tests to
   confirm the file-backed path remains complete.

## Open Questions

- None.
