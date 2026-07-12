## Context

The repository already has the durable substrate this change needs:

- Kernel namespace and `/proc/clone` Process creation;
- LLMFS Generation through mounted files;
- AgentFS `io/`, `requests/`, `actions/`, `machine/`, and `machine/ui/` trees;
- Tool package projections under `/bin`, `/lib/exec/<tool>`, and `/man`;
- a file-backed Rust TUI.

The Agent Execution Engine nevertheless maintains two parallel live authorities.
`RuntimeLoopState` carries an in-process `ToolRegistry`, and
`RuntimeToolProcessRunner` materializes effects by calling registry-backed Tool
implementations after a Process record is created. Separately,
`RuntimeEventEnvelope` is broadcast through `RuntimeHandle.event_sender`; a
forwarding task projects those events into AgentFS, and delegated-child
supervision subscribes to the broadcast for progress and output.

The canonical specs already say the namespace and AgentFS are authoritative.
This change makes the implementation literal. It starts only after the spec,
shim, and macOS cleanup changes are complete so review is not mixed with
unrelated compatibility removal.

## Goals / Non-Goals

**Goals:**

- Make the namespace handle the engine loop's only environment value.
- Discover and execute Tools solely from mounted executable packages.
- Make each runtime state owner write its AgentFS files directly.
- Remove live runtime publish/subscribe APIs and the event-to-file projector.
- Supervise children through `/proc` and AgentFS observation.
- Preserve Event/Op data types only where they are file schemas or
  transition-local values.

**Non-Goals:**

- Design Alan for macOS attachment to Alan OS.
- Implement Service Manager boot or production multi-process isolation.
- Redesign aP, Kernel namespace semantics, Tool package layout, or AgentFS.
- Remove all semantic `Event` or `Op` Rust types by name.
- Change renderer behavior beyond replacing hidden live sources with the
  canonical files it already reads.
- Add a registry-backed or event-broadcast compatibility bridge.

## Decisions

### 1. The engine stores a namespace handle directly

The single-variant `RuntimeEnvironment` enum is deleted. Engine construction,
loop state, turn execution, child launch, and tests receive the concrete
namespace handle. Provider, Tool, policy, skill, memory, and AgentFS reachability
is resolved through that handle or descriptors derived from it.

Alternative considered: keep the enum for future environments. Rejected because
it makes non-namespace execution look supported and weakens the single
environment invariant.

### 2. Tool discovery is a namespace walk over complete packages

Before an Agent Process starts, the composition owner must mount each permitted
Tool executable at `/bin/<tool>` and its manifest at
`/lib/exec/<tool>/manifest`; manual text is available under `/man` as defined by
the package contract. Request assembly walks visible `/bin` entries, joins each
with its manifest, and derives model name, description, parameter schema,
capability class, locality, and execution hints.

An entry without a valid Tool manifest is either an ordinary command/Agent
Executable or an invalid Tool package; it is not model-callable. A manifest
without a visible executable grants nothing. There is no hidden catalog whose
intersection with `/bin` determines the result.

Alternative considered: rebuild a temporary `ToolRegistry` from the namespace.
Rejected because the registry would remain the execution authority under a new
materialization step. A typed immutable description returned by a namespace
walk is allowed; executable effect dispatch remains `/proc/clone`.

### 3. Tool effects execute through Process ownership

The registry-backed `RuntimeToolProcessRunner` is replaced by a namespace Tool
launcher. It resolves `/bin/<tool>`, commits an exec spec through
`/proc/clone`, writes arguments through the executable's defined Process/file
contract, and reads Process output/result files. `actions/<id>` records the
concrete Tool Process and outcome.

The engine cannot call a Rust Tool implementation directly. Built-in Tool
implementations may still be hosted in-process by a file/process server during
the current convention-enforced stage, but invocation crosses the same
namespace and Process boundary as every other Tool.

### 4. Runtime owners write AgentFS directly

Introduce narrow owner-specific AgentFS writers for output/tape, requests,
actions, and renderer-safe `machine/ui` state. The turn driver writes text and
tape; request orchestration owns request trees; Tool orchestration owns action
trees; plan/thinking/activity/notice owners write their corresponding
`machine/ui` snapshot and append a typed record to `machine/ui/events`.

These writers serialize current state directly. They do not accept a generic
runtime event and are not an event projector in disguise. A top-level AgentFS
event record may still be appended as a file-owned audit/change record when the
file contract requires it.

Alternative considered: keep `RuntimeUiProjector` but hide its broadcast
receiver. Rejected because state would still be derived from a parallel event
alphabet rather than written by its owner.

### 5. Live broadcast APIs are deleted after all owners switch

After direct writers and file-based child supervision are active, delete
`RuntimeEventEnvelope`, `RuntimeHandle.event_sender`, broadcast channel setup,
event-forwarding tasks, subscription helpers, host forwarding, and tests that
wait on a live receiver. `Event` variants used only by the broadcast are deleted;
variants used as persisted file records or transition-local control values may
remain under ownership-accurate names.

There is no deprecation window and no host compatibility callback. Tests observe
AgentFS offsets and `/proc` state.

### 6. Child supervision watches Process and file surfaces

The parent registers the child after `/proc/clone` allocation and before initial
input. Liveness comes from `/proc/<pid>/status` and terminal process state.
Progress comes from monotonic offsets and timestamps on child-owned files,
including `io/output`, `requests/`, `actions/`, and `machine/ui/events`;
`machine/ui/activity` carries current quiet-running activity/heartbeat
freshness. The delegation record stores only the observed cursor/timestamp and
delegation metadata.

Timeout classification uses the latest authoritative Process/file freshness.
The parent never needs the child's `RuntimeHandle` or event receiver.

### 7. Verification asserts absence as well as behavior

Focused tests prove:

- an unmounted Tool is neither described to the model nor executable;
- a mounted executable plus valid manifest is both discoverable and spawned;
- parent/child tool differences come entirely from mounted packages;
- UI/output/request/action files update without a live event channel;
- a child can exceed wall-clock timeout while file heartbeat is fresh;
- process exit wins over stale child-run metadata;
- no `ToolRegistry`, `RuntimeEventEnvelope`, `event_sender`, or generic event
  projector remains on the engine live path.

## Risks / Trade-offs

- [Tool package manifests lack metadata currently held only in Rust types] →
  complete and validate manifests before switching request assembly; fail closed
  on incomplete packages.
- [Direct writes become scattered] → Use narrow owner-specific writer modules
  with AgentFS contract tests, not a generic event bus.
- [Child observation misses quiet work] → Require current activity heartbeat
  timestamps in AgentFS and combine them with `/proc` liveness.
- [In-process hosting is mistaken for hard isolation] → Preserve ADR-0024's
  convention-enforced boundary statement until transport/isolation work lands.
- [Removing broadcast breaks hidden host tests] → Inventory every receiver and
  replace it with a file assertion before deleting the type.
- [File IO adds hot-path overhead] → Use AgentFS append/update primitives and
  blocking streams; benchmark turn streaming and child supervision without
  adding a second cache authority.

## Migration Plan

1. Validate and complete built-in Tool package manifests and composition tests.
2. Add namespace Tool discovery and Process-launch helpers; switch model Tool
   definitions and effects away from `ToolRegistry`.
3. Switch child namespace assembly to mounted package selection only.
4. Add owner-specific AgentFS writers and move output, tape, request, action,
   activity, plan, thinking, and notice updates to direct writes.
5. Move child liveness/progress supervision to `/proc` and AgentFS cursors.
6. Convert host and integration tests from event receivers to file observation.
7. Delete the broadcast/event projector, registry materializer, registry fields,
   and single-variant environment wrapper.
8. Run workspace format/lint/tests, focused Kernel/AgentFS/engine/TUI suites,
   dependency checks, and an end-to-end file-only conversation with Tool and
   child execution.

Each step must leave no production fallback to the path it replaces. Rollback
is by reverting the complete change; partial fallback flags are not introduced.

## Open Questions

None.

## Implementation Inventory

The pre-replacement inventory on main at `fa7a5f70` found these live-path owners:

- `ToolRegistry`: 21 files across `agent-engine`, the Alan host, and built-in Tool
  composition. The live authorities are `RuntimeLoopState.tool_catalog`, runtime bootstrap,
  `RuntimeToolProcessRunner`, request/tool orchestration, and child registry rebinding; the
  remaining occurrences are public exports, configuration comments, and tests.
- `RuntimeToolProcessRunner`: one implementation in `runtime/engine.rs`; it owns the current
  direct registry-backed Process materialization seam.
- `RuntimeEventEnvelope`: seven files; the live owner is `runtime/engine.rs`, child supervision
  consumes it in `runtime/child_agents.rs`, and Alan integration/smoke tests subscribe to it.
- `event_sender`: five files; `RuntimeHandle` owns the broadcast sender, child supervision and
  Alan tests consume receivers, and the runtime forwarding task bridges internal events to it.
- `RuntimeUiProjector`: `runtime/ui_surfaces.rs` plus engine initialization/forwarding; it derives
  AgentFS UI files from the generic event stream.
- `RuntimeEnvironment`: 16 files; the single `Namespace` variant is stored by
  `RuntimeLoopState` and propagated through engine, turn, child, memory, submission, and tests.

Replacement order follows the migration plan: complete Tool packages and discovery first,
replace execution and child composition next, move state owners and child observation to files,
then delete the registry, broadcast/projector, and wrapper types. Test-only uses are converted
with the production owner they observe rather than retained as compatibility seams.
