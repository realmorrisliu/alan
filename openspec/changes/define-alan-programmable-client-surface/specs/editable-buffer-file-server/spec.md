## REMOVED Requirements

### Requirement: Explicit exec records accepted and denied outcomes

**Reason**: The headless `ExecutionPolicy::{AcceptAll,DenyAll}` decision can
neither grant nor enforce execution authority once selected text runs as a
caller-spawned Process. Keeping it would misrepresent editfs as a Tool-policy
boundary and invite a confused-deputy implementation.

**Migration**: Replace accept/deny policy with atomic selection validation for
the caller-spawned `run` Process. Spawn, Namespace access, Tool governance, and
sandbox projection own execution authority; editfs records Process-linked
interaction events.

## ADDED Requirements

### Requirement: editfs validates evaluator Process selections
Editfs SHALL accept a complete-document `ctl exec` from a caller-spawned `run`
Process containing the evaluator's `/proc/<pid>` Path, expected body revision,
expected address revision, and selected range. On clunk, editfs SHALL atomically
validate the snapshot and append an execution-started event only when it still
matches. Editfs SHALL NOT spawn the evaluator, execute the selected text, or
apply an editfs-owned side-effect policy. The evaluator-supplied `/proc/<pid>`
Path SHALL be recorded as caller-asserted correlation metadata, not verified
identity, until aP request provenance provides authentication.

#### Scenario: Evaluator selection is current
- **WHEN** a `run` Process clunks a complete `ctl exec` document whose body and
  address snapshot matches the current buffer
- **THEN** editfs commits validation and appends an event containing the selected
  command text and `/proc/<pid>` Path

#### Scenario: Evaluator selection is stale
- **WHEN** the expected body revision, address revision, or range does not match
  the current buffer at clunk
- **THEN** editfs returns a typed aP error and appends no execution-started event

### Requirement: body supports revision-safe result append
Editfs SHALL allow an evaluator Process to append bounded UTF-8 result bytes to
the current end of `body` through a read-write fid bound to the body revision
observed at open. Clunk SHALL commit the append only if that revision and append
position are still current. A successful result append SHALL advance the body
revision and remain an ordinary body edit.

#### Scenario: Result append commits
- **WHEN** an evaluator opens `body` read-write, writes bounded UTF-8 bytes at the
  observed end, and clunks before another body edit
- **THEN** editfs appends the bytes, advances the body revision, and emits an edit
  event

#### Scenario: Concurrent edit rejects result append
- **WHEN** another client changes `body` after the evaluator opens its
  read-write fid but before clunk
- **THEN** editfs rejects the stale append and preserves the concurrent edit

### Requirement: editfs links materialized ranges to Process truth
After a successful result append, editfs SHALL accept a complete-document
materialization record from the evaluator that names `/proc/<pid>`, the committed
body revision, and result range. It SHALL validate that the named range exists
in that revision and append a Process-linked materialization event. Editfs
SHALL NOT copy Process status, exit state, or complete output into a parallel
execution record.

#### Scenario: Materialization is recorded
- **WHEN** a `run` Process reports the body revision and range committed for its
  finite result
- **THEN** editfs appends an event linking that range to `/proc/<pid>`
- **AND** clients continue to read execution status and complete output from the
  Process files

#### Scenario: Materialization record is inconsistent
- **WHEN** the reported body revision or range does not describe the committed
  result bytes
- **THEN** editfs rejects the record and does not publish a false Process/result
  association
