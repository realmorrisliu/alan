# Proposal

## Why

Some users deliberately prefer uninterrupted execution with their own chosen isolation boundary. This must be explicit and distinguishable from the default governed project workflow.

## What Changes

- **BREAKING** Add a trusted-user-selected fully trusted approval posture alongside autonomous; do not activate it from project files or restored evidence.
- Make bypass invocation-only, independent from the default confined execution boundary; Host-account native execution needs a separate explicit selection.
- Bypass per-action policy/Guardian decisions, including otherwise always-human action red lines, while preserving cancellation, budgets, truthful reporting and protocol/OS checks.
- Document that unconstrained same-account code can modify writable control and audit files; do not claim sandbox guarantees there.
- Adopt the OS-boundary rule: no static shell-text bans or Guardian prompts for effects the active confinement already contains; mounted `AGENTS.md`/`CLAUDE.md` are readable non-authoritative knowledge.
- Qualify confined-trusted and Host-account-trusted operation separately from default daily-use scores.

## Capabilities

### New Capabilities

- `os-boundary-security`: Rely on the OS boundary inside confinement; project instruction files are non-authoritative knowledge.

### Modified Capabilities

- `auto-approve-policy`: Explicit posture selection and precise bypass scope.
- `autonomous-review-mode`: Scope reviewer requirements to governed execution.
- `sandbox-autonomy-invariants`: Scope action gates and isolation guarantees by explicit mode.
- `os-sandbox-enforcement`: Distinguish unavailable sandbox from deliberately selected Host-account execution.
- `host-mount-tool-process-sandbox-projection`: Explicit confined-versus-Host-account native authority without altering aP grants.
- `alan-shell`: Exact command semantics follow the selected posture and isolation.
- `rust-inline-tui`: Visible posture/isolation and trusted-user switching.

## Impact

Milestone M3 (ADR-0061): builds on M2's OS-boundary simplification, proceeds ahead of the delegation qualification corpus and no longer waits for it. Overlapping `auto-approve-policy`, `autonomous-review-mode` and `sandbox-autonomy-invariants` deltas are rebased by whichever change syncs second. Same Runtime, Host adapter and AgentFS owners; no second permission engine. Host-account mode does not synthesize aP mount handles or override operating-system privileges.
