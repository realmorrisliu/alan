## MODIFIED Requirements

### Requirement: Address ranges are revision-bound

Alan SHALL store the active body range in `addr` using `rev:<body-revision>
<start>..<end>` write syntax. Reads SHALL return the selected body revision,
address revision, and range as `rev:<body-revision> addr:<addr-revision>
<start>..<end>`. The only `exec` form `ctl` accepts SHALL be the evaluator
`ctl exec` document carrying the `/proc/<pid>` Path and the expected
body/address revision snapshot; the legacy path-less
`exec rev:<body-revision> addr:<addr-revision> <start>..<end>` syntax SHALL be
rejected rather than validated.

#### Scenario: Address range is selected

- **WHEN** a client writes a valid current-revision range to `addr` and clunks
  the fid
- **THEN** subsequent reads of `addr` return that range
- **AND** an address event is appended to `event`

#### Scenario: Stale address fails at execution

- **WHEN** the body revision changes after an address range is selected
- **AND** an evaluator Process clunks a complete `ctl exec` document carrying
  its `/proc/<pid>` Path and the stale body/address revision snapshot
- **THEN** the clunk fails with a typed aP error
- **AND** no command text from the stale range is executed

#### Scenario: Legacy path-less exec is rejected

- **WHEN** a client writes the legacy `exec rev:<body-revision>
  addr:<addr-revision> <start>..<end>` form to `ctl` without an evaluator
  `/proc/<pid>` Path and clunks the fid
- **THEN** the clunk fails with a typed aP error and no execution event is
  recorded
- **AND** stale-selection validation cannot be satisfied through the legacy
  syntax

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

### Requirement: Result materialization is one atomic append-and-link commit
Editfs SHALL accept an evaluator's complete-document `materialize` control write
carrying the evaluator's `/proc/<pid>` Path, the expected body revision and
append position, and the bounded UTF-8 result bytes. On clunk, editfs SHALL
atomically validate the expected revision and append position, append the bytes
at the current end of `body` as an ordinary body edit that advances the body
revision, and emit the edit event plus a Process-linked materialization event
naming the committed range, the new body revision, and the supplied
`/proc/<pid>` Path. As with `ctl exec`, the supplied `/proc/<pid>` Path SHALL be
recorded as caller-asserted correlation metadata, not verified identity, until
aP request provenance provides authentication: the atomic commit binds the
bytes to this commit, not the pid to the writer, so consumers MUST NOT treat
the Process link as verified provenance. A stale revision or append position
SHALL fail the commit with a typed aP error and no side effects. Because the result bytes and their
Process link commit together, a materialization event SHALL never attribute
bytes committed by another writer, and editfs SHALL NOT accept a post-hoc
record that links an existing `body` range to a Process. This uses only
ordinary aP writes and commit-on-clunk; no clunk response payload or protocol
change is required. Editfs SHALL NOT copy Process status, exit state, or
complete output into a parallel execution record.

#### Scenario: Materialization commits atomically
- **WHEN** an evaluator clunks a complete `materialize` document whose expected
  revision and append position match the current buffer
- **THEN** `body` gains the appended bytes as an ordinary edit with an advanced
  revision and an edit event
- **AND** the same commit emits a materialization event linking exactly that
  committed range and revision to the supplied `/proc/<pid>` Path, recorded as
  caller-asserted correlation

#### Scenario: Concurrent edit rejects materialization
- **WHEN** another client changes `body` after the evaluator captures the append
  position but before its `materialize` document is clunked
- **THEN** editfs fails the commit with a typed aP error, appends no bytes,
  emits no events, and preserves the concurrent edit
- **AND** the evaluator may retry a safe append against the newly read body end

#### Scenario: Post-hoc range attribution is rejected
- **WHEN** a client submits a record that names an existing `body` range and a
  `/proc/<pid>` Path without carrying the result bytes in a `materialize` commit
- **THEN** editfs rejects it, because Process/result links exist only as
  products of atomic materialize commits
