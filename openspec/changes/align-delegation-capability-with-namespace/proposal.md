## Why

Delegation can still launch a child whose world cannot satisfy the task (the
capability-mismatch failure mode observed in the superseded
`harden-agent-operating-system-contracts` change, archived 2026-07-10). Since
the namespace-native refactor, an agent's capability set IS its spawner-assembled
namespace (`agent-namespace-runtime`), so eligibility must be expressed as
spawn-time namespace alignment — not the descriptor-matching engine the
superseded change proposed.

## What Changes

- Classify the material capabilities a delegated task requires (workspace
  read/write scope, shell, network/GitHub, browser, model access, side effects)
  before spawning the child, in a vocabulary that maps directly onto namespace
  mounts and `/bin` bindings.
- Check the namespace the spawner is about to assemble against those
  requirements; a requirement is satisfied if and only if the corresponding
  mount/binding will be present in the child's namespace.
- On mismatch, take a visible recovery path: narrow the task to what the
  namespace supports (stated in the child's task description), satisfy the
  requirement through the parent's own namespace, ask the user for the missing
  input, or return a limitation-focused answer — never silently substitute
  unrelated local context.
- Make decisions auditable through existing namespace surfaces: a launched
  child's capability record is its `/proc/<pid>/namespace`; declined or
  narrowed launches are recorded on the parent's own surfaces (action record /
  tape), not in a parallel decision store.
- Design the check on the spawn model (`delegate` is an Agent Executable spawn
  target per `agent-file-layout-contract`); the current `invoke_delegated_skill`
  virtual tool receives the check at its child-spawn boundary so the contract
  survives the delegation-to-spawn migration.

## Capabilities

### New Capabilities

- `delegation-capability-alignment`: Owns task-requirement classification, the
  spawn-time requirement-vs-namespace check, mismatch recovery paths, and
  decision observability for delegated launches.

### Modified Capabilities

- `child-run-lifecycle`: Child-run launch metadata gains a bounded summary of
  the capability-bearing mounts the child was spawned with, so a mismatch
  investigation does not require the child process to still be alive.

## Impact

- Affected runtime modules: delegated child spawn path
  (`crates/agent-engine/src/runtime/virtual_tools.rs` and the future `delegate`
  Agent Executable), exec-spec/namespace assembly, child-run registration.
- Affected specs: interacts with `agent-namespace-runtime` (spawner-assembled
  namespace) and `agent-file-layout-contract` (Agent Executables, `children/`);
  neither is modified — this change consumes their guarantees.
- Affected skills: delegated-skill instructions describe requirement narrowing
  and recovery semantics once the runtime check exists.
- Affected tests: mismatch decline, narrowed-task delegation, parent-path
  recovery, and namespace-metadata presence on child-run records.
