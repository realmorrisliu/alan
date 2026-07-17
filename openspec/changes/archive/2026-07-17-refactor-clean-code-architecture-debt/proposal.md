## Why

The repository quality gate makes 64 oversized Rust sources, 15 Apple
architecture warnings, and two important Alan OS ownership leaks visible but
does not remove them. This change begins immediately after that gate merges so
the accepted ceilings become a burn-down queue instead of permanent debt.

## What Changes

- Split oversized Rust owners along behavioral and architectural seams until
  every Rust source under `crates/` is at or below the 1,000-line target.
- Remove all 15 recorded Apple architecture warnings through narrow,
  behavior-preserving ownership slices.
- Move Process namespace assembly and AgentFS lifecycle composition out of the
  Agent Execution Engine and behind the Agent Runtime Service boundary.
- Make Connection Service the owner of connection profile metadata and
  selection; narrow Agent Execution Engine to consuming mounted connection
  handles.
- Tighten the quality baselines after every reduction so removed debt cannot
  return.
- Start the first focused implementation PR immediately after
  `enforce-clean-code-architecture-gates`, before unrelated feature work.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `repository-quality-gate`: Require staged debt burn-down to tighten the Rust
  source and architecture dependency ceilings in every refactor slice.
- `macos-app-architecture-maintainability`: Replace the 15-warning transition
  ledger with zero-warning strict enforcement.
- `agent-namespace-runtime`: Move Process namespace and AgentFS lifecycle
  composition from the Agent Execution Engine to Agent Runtime Service.
- `connection-service`: Make profile metadata, defaults, selection, and
  publication exclusive Connection Service ownership.

## Impact

This is a behavior-preserving refactor program across `crates/agent-engine`,
Agent Runtime Service composition, Connection Service composition, large Rust
owners elsewhere in `crates/`, and `clients/apple/alan-macos`. It follows the
quality-gate PR as a sequence of focused PRs; it does not intentionally change
public Alan OS, AgentFS, aP, shell, or macOS product behavior.
