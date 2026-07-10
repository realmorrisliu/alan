## Context

The superseded `harden-agent-operating-system-contracts` change (2026-05-14)
observed real failures: delegation launched children that could not reach
GitHub, the network, or the target workspace, and the failure surfaced as an
opaque bad answer instead of a routing decision. Its remedy — a capability
descriptor per delegated target plus a runtime matching engine — predates the
namespace-native substrate. Today:

- An agent's capability set is exactly what its spawner mounted
  (`agent-namespace-runtime`: "anything not present in that namespace is
  unreachable").
- `/proc/<pid>/namespace` already renders a spawned process's world; there is
  no need for a second capability record for launched children.
- `delegate` is designated an Agent Executable spawn target
  (`agent-file-layout-contract`), while the shipping code still routes
  delegation through the `invoke_delegated_skill` virtual tool.

## Goals / Non-Goals

**Goals:**

- Requirement classification and eligibility checking at the child-spawn
  boundary, expressed in mount/binding vocabulary.
- Visible, recorded recovery when requirements cannot be satisfied.
- Zero new registries: launched-child observability rides `/proc`, declined
  launches ride the parent's action/tape records.

**Non-Goals:**

- Migrating `invoke_delegated_skill` to the `delegate` Agent Executable (a
  separate change; this contract is written to survive it).
- Hard security isolation claims (ADR-0024 R1 boundary still applies:
  convention-enforced until the kernel amplification check lands).
- A general planner that infers requirements from arbitrary prose; the first
  vocabulary is small and mechanical.
- Cleaning up the legacy child-run registry vs `children/` duplication (tracked
  separately; this change only adds launch metadata to whichever record exists).

## Decisions

### 1. Capability vocabulary = namespace vocabulary

A requirement is named by the mount or binding that would satisfy it (e.g.
"workspace write mount for path X", "`gh`/GitHub-capable tool in `/bin`",
"network-capable shell", "LLM connection"). Satisfaction is a lookup against
the exec spec's namespace assembly, not a comparison of two hand-maintained
descriptor lists.

Alternative considered: per-target capability descriptors (the superseded
design). Rejected: descriptors drift from actual mounts; the namespace is
already the authoritative record and cannot drift from itself.

### 2. The check lives at the spawn boundary

The eligibility check runs where the exec spec is assembled, immediately before
`/proc/clone`. That single seam covers the current virtual tool (which
ultimately spawns a child runtime) and the future `delegate` executable (which
assembles a namespace by construction). Nothing upstream of the spawn boundary
needs to know the vocabulary.

Alternative considered: checking inside the delegated skill's prompt guidance.
Rejected by the superseded change already: the failure mode is a routing error,
so the model that made the error cannot be its only guard.

### 3. Mismatch produces a decision record on existing surfaces

Declined or narrowed launches append a bounded decision record to the parent's
action record (or tape, pre-migration): required capabilities, what was
missing, and the chosen recovery path. Launched children need no extra record
beyond a bounded namespace summary in child-run launch metadata, because
`/proc/<pid>/namespace` is live truth while the child exists and the launch
metadata preserves it after exit.

Alternative considered: a dedicated capability-decision log. Rejected: second
source of truth, violates the no-parallel-registries principle.

### 4. Narrowing rewrites the task, not just the mounts

When the parent narrows a task to fit an available namespace (e.g. "local
inspection only, GitHub content will be supplied by the parent"), the child's
task description states the narrowed scope explicitly, and the parent remains
responsible for the withheld part. A child must never discover mid-run that its
task assumed a capability its world lacks.

## Risks / Trade-offs

- [Risk] Requirement classification under-detects (task needs network, classifier
  misses it) → the child fails as today, but the decision record shows the
  classifier's view, making the gap diagnosable; vocabulary grows incrementally.
- [Risk] Over-detection blocks legitimate delegation → recovery paths include
  proceeding with an explicitly narrowed task; the check narrows rather than
  hard-fails wherever a narrowed task is coherent.
- [Risk] Dual delegation paths (virtual tool now, `delegate` executable later)
  drift → the check is a single function at the spawn boundary both paths share;
  tests pin both entry points.

## Open Questions

- Exact first vocabulary: is browser access modeled as a `/bin` tool binding or
  a dedicated mount at first cut?
- Should the user-facing mismatch surface be a `requests/` entry (ask for the
  missing input) by default, or only when no narrowing is possible?
