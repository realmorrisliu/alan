# delegation-capability-alignment Specification

## Purpose
Defines namespace-vocabulary classification for delegated task requirements,
pre-spawn capability satisfaction, visible mismatch recovery, and observable
capability decisions.
## Requirements
### Requirement: Task Requirements Are Classified In Namespace Vocabulary
The runtime SHALL classify the material capabilities a delegated task requires
before spawning the child, using a vocabulary whose terms map directly onto
namespace mounts and `/bin` bindings (Host Mount read/write scope, shell,
network/GitHub-capable tools, browser, LLM connection, side effects).

#### Scenario: GitHub issue review requires GitHub-capable bindings
- **WHEN** a user asks alan to inspect and review a GitHub issue in another
  repository via delegated work
- **THEN** the classified requirements include read access to the target
  repository through a Host Mount and
  a GitHub- or network-capable tool binding, before any child is spawned

#### Scenario: Local inspection classifies as Host-Mount-read only
- **WHEN** a delegated task only inspects local files in another repository tree
- **THEN** the classified requirements name a read-scoped Host Mount and no
  network capability

### Requirement: Delegated Spawn Requires Namespace Satisfaction
The runtime SHALL spawn a delegated child only when the namespace its exec spec
assembles satisfies the classified task requirements, or when the task has been
explicitly narrowed to what that namespace supports. A requirement SHALL be
considered satisfied only by a corresponding mount or binding in the child's
namespace, not by a separately maintained capability descriptor.

#### Scenario: Child namespace lacks a required capability
- **WHEN** the assembled child namespace would contain no binding satisfying a
  classified requirement (for example, no GitHub-capable tool in `/bin`)
- **THEN** the runtime does not spawn the child for the original task and records
  a capability-mismatch decision naming the unsatisfied requirements

#### Scenario: Narrowed task is spawned with explicit scope
- **WHEN** the parent narrows the task to fit the available namespace
- **THEN** the child's task description states the narrowed scope and the
  withheld capability, and the parent remains responsible for the withheld part

### Requirement: Capability Mismatch Has Visible Recovery
The runtime SHALL take a visible recovery path when no assemblable child
namespace satisfies the task: satisfy the requirement through the parent's own
namespace, narrow the task, ask the user for the missing input, or return a
limitation-focused answer. The runtime SHALL NOT silently substitute unrelated
local context for the unsatisfiable part of the task.

#### Scenario: Parent namespace can satisfy the missing capability
- **WHEN** the parent's own namespace contains a binding that satisfies the
  requirement the child world lacks
- **THEN** alan may perform that part through the parent path and records that
  the parent recovered from a delegated capability mismatch

#### Scenario: No available namespace can satisfy the task
- **WHEN** neither the parent namespace nor any assemblable child namespace
  satisfies a required capability
- **THEN** alan asks for the missing input or answers with the limitation stated,
  instead of substituting unrelated local context

### Requirement: Capability Decisions Are Observable On Existing Surfaces
The runtime SHALL make capability decisions auditable through existing
namespace surfaces: a launched child's capability record is its
`/proc/<pid>/namespace` plus bounded launch metadata, and declined or narrowed
launches are recorded on the parent's action record or tape. The runtime SHALL
NOT introduce a parallel capability-decision registry.

#### Scenario: Launched child is audited
- **WHEN** an auditor inspects a delegated launch decision for a running child
- **THEN** `/proc/<pid>/namespace` shows the child's actual capability set and
  the launch record carries the classified requirements

#### Scenario: Declined launch is audited
- **WHEN** an auditor inspects a delegation that was declined or narrowed
- **THEN** the parent's action record or tape contains the classified
  requirements, the unsatisfied capabilities, and the chosen recovery path
