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
revision and remain an ordinary body edit, and editfs SHALL return an append
commit token to the committing fid identifying that commit, its resulting body
revision, and the committed range.

#### Scenario: Result append commits
- **WHEN** an evaluator opens `body` read-write, writes bounded UTF-8 bytes at the
  observed end, and clunks before another body edit
- **THEN** editfs appends the bytes, advances the body revision, and emits an edit
  event
- **AND** the committing evaluator receives an append commit token naming the
  committed range and new body revision

#### Scenario: Concurrent edit rejects result append
- **WHEN** another client changes `body` after the evaluator opens its
  read-write fid but before clunk
- **THEN** editfs rejects the stale append and preserves the concurrent edit

### Requirement: editfs links materialized ranges to Process truth
After a successful result append, editfs SHALL accept a complete-document
materialization record from the evaluator that names `/proc/<pid>` and presents
the append commit token editfs issued for that result append. Editfs SHALL bind
the materialization event to the specific append commit the token identifies —
its committed range and body revision — and SHALL NOT accept range existence in
a revision as a substitute for the token; a record naming bytes committed by a
different writer or a different commit SHALL be rejected. Editfs SHALL NOT copy
Process status, exit state, or complete output into a parallel execution record.

#### Scenario: Materialization is recorded
- **WHEN** a `run` Process presents the append commit token for its finite
  result together with its `/proc/<pid>` Path
- **THEN** editfs appends an event linking that commit's range and revision to
  `/proc/<pid>`
- **AND** clients continue to read execution status and complete output from the
  Process files

#### Scenario: Materialization record is inconsistent
- **WHEN** the record's token does not match an append commit editfs performed,
  or names a commit other than the one that wrote the result bytes
- **THEN** editfs rejects the record and does not publish a false Process/result
  association

#### Scenario: Record names another writer's bytes
- **WHEN** an evaluator reports a range that exists in the named revision but was
  committed by a different writer or commit
- **THEN** editfs rejects the record because the presented token does not
  identify that commit
