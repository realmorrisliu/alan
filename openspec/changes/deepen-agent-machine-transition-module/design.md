## Context

The archived clean-architecture burn-down established the external file-native
runtime boundary, but the live implementation still distributes transition
state across `AgentMachine`, `RuntimeLoopState`, and
`NamespaceRuntimeEnvironment`. `engine.rs` consequently coordinates both
Process-loop concerns and the details of advancing an accepted submission.

The target ownership is already accepted: Process owns lifecycle and identity;
Agent Machine owns Tape and transition-local state; AgentFS owns observable
agent files; rollout and checkpoint files own durable evidence. This change
deepens that boundary without changing any file or runtime behavior.

## Goals / Non-Goals

**Goals:**

- Give Agent Machine exclusive ownership of Tape and transition-local state.
- Make the accepted-submission transition a cohesive, testable module boundary.
- Keep Process-loop control outside the transition owner.
- Replace broad state/environment reach-through with private state and narrow
  concrete inputs.
- Leave a smaller internal API whose ownership can be completed by the
  subsequent file-native seam change.

**Non-Goals:**

- Change AgentFS, `/proc`, aP, namespace, request, action, Tool, memory,
  compaction, persistence, recovery, child-process, or renderer behavior.
- Move Process assembly or Host adapter composition; that belongs to
  `complete-agent-runtime-file-native-seam`.
- Add an engine abstraction framework, a transition trait, a factory, or a
  second Machine representation.
- Optimize for a new line-count, method-count, or module-count target.

## Decisions

### Decision: Agent Machine owns one private transition state

Agent Machine owns Tape, the current accepted submission, turn state, pending
Yield, Tool replay state, active-task state, and deferred transition action.
These fields become private and are changed through semantic operations. A
private `RuntimeLoopState` may remain if it is the smallest useful
implementation detail, but it cannot remain a broadly shared field bag.

Agent Machine is not a cross-crate integration surface. Its public re-export and
direct field access are removed; private behavior is covered by adjacent
white-box tests, while external contracts are tested through AgentFS, `/proc`,
rollout, and checkpoint files.

Alternative considered: retain public fields and split callers into more
extensions. Rejected because it preserves distributed ownership while only
moving source text.

### Decision: The transition seam starts after submission acceptance

`engine.rs` retains input polling, channel closure, shutdown, cancellation,
heartbeat, and other Process-loop control. Once a `Submission` is accepted, one
concrete transition owner advances Agent Machine through generation, Yield,
Tool replay, completion, and deferred actions, then returns a compact outcome to
the outer loop.

The transition owner is a concrete module and function boundary, not a trait or
pluggable strategy. Its outcome contains only what the outer loop needs to
continue Process control.

Alternative considered: move the entire engine loop into Agent Machine.
Rejected because input transport and Process lifecycle are not Machine state.

### Decision: One namespace handle enters the transition

The transition boundary receives the existing concrete namespace-backed
environment. Only that boundary may see the complete transitional environment;
child modules receive the specific paths, records, handles, or operations they
need. Namespace construction and cross-crate composition remain unchanged until
the next change removes their transitional collaborators.

Alternative considered: define narrow traits for every current environment
operation. Rejected because each would have one implementation and would hide
the same broad dependency behind boilerplate.

### Decision: Characterize behavior at durable boundaries

Before moving each workflow, focused tests pin its AgentFS, rollout/checkpoint,
Yield/resume, Tool replay, compaction, and failure behavior. Internal tests may
exercise private Machine operations, but completion evidence comes from the
durable file boundaries. No temporary compatibility path is added.

Alternative considered: preserve the old state path until the whole refactor is
finished. Rejected because dual state ownership would make parity unprovable.

## Risks / Trade-offs

- [Moving state changes ordering around Yield or replay] → Characterize the
  affected file/evidence sequence first and move one complete workflow at a
  time.
- [A nominal transition module remains coupled to the full environment] → Keep
  full-environment access at one boundary and reject new downstream reach-through
  in review and architecture checks.
- [Privacy makes tests harder] → Use adjacent white-box tests for internals and
  file-boundary tests for supported behavior; do not reopen public fields.
- [A large one-shot move is difficult to review] → Deliver focused stacked PRs,
  each deleting the old path before merge.

## Migration Plan

1. Characterize the current accepted-submission, Yield/resume, Tool replay,
   compaction, persistence, and cancellation boundaries.
2. Privatize Agent Machine state and move current submission and turn-local
   state behind semantic Machine operations.
3. Move accepted-submission advancement into the concrete transition owner and
   narrow its downstream inputs.
4. Delete obsolete field access, exports, and forwarding helpers; update the
   existing architecture gates without adding a new numeric budget.
5. Run focused engine tests, workspace quality checks, and strict OpenSpec
   validation. Merge only after CI and Codex Review remain clean on the current
   HEAD through a follow-up review window.

Rollback is per focused PR. The change has no persisted data migration, so a
revert restores the prior in-memory ownership without converting user data.

## Open Questions

None.
