## MODIFIED Requirements

### Requirement: Executable text uses explicit control operations

Alan OS SHALL execute text from an editable buffer only through an explicit
caller-spawned `run` Tool Process. The evaluator Process SHALL read the selected
bytes and submit a complete-document `ctl exec` carrying its `/proc/<pid>` Path,
the expected `addr` range, source `body` revision, and address revision; editfs
SHALL act only when that document is clunked and SHALL atomically validate the
snapshot before the Process dispatches the shared Alan Shell command executor.
Editfs SHALL NOT spawn the Process, execute the text, or lend service authority.
Until the ADR-0024 R1 amplification check lands, the mount set is an
architectural discipline rather than a security property; native subprocesses
such as shell commands cannot see the Alan namespace directly, so OS sandbox
projection remains a permanent second enforcement mechanism for them.

#### Scenario: Selected text is executed

- **WHEN** a client spawns `/bin/run` with a buffer Path or bounded descriptors
  and the evaluator commits a matching process/body/address snapshot to `ctl`
- **THEN** editfs records an execution-started event referencing `/proc/<pid>`
- **AND** the evaluator executes the captured text through the shared Alan Shell
  command path under its inherited Namespace

#### Scenario: Partial control writes do not validate execution

- **WHEN** the evaluator writes only part of an `exec` control document to `ctl`
- **THEN** editfs does not validate or record the execution until the evaluator
  completes the document and clunks `ctl`

#### Scenario: Concurrent selection changes do not retarget execution

- **WHEN** a client captures an `addr` range, source `body` revision, and address
  revision, another client changes `addr`, and the evaluator commits the old
  snapshot
- **THEN** the operation fails with a typed aP error and the evaluator exits
  without executing text from the other client's range

#### Scenario: Concurrent body edits do not retarget execution

- **WHEN** a client captures selected bytes and another client edits `body`
  before the evaluator commits the expected body/address snapshot
- **THEN** the operation fails with a typed aP error and the evaluator exits
  without executing the mutated range

#### Scenario: Execution does not bypass authority

- **WHEN** selected text would require an executable, mount, descriptor, or
  action not available to the evaluator Process
- **THEN** execution is denied or fails through normal Namespace, access-right,
  Tool-governance, or policy behavior
- **AND** editfs validation does not grant the missing authority

#### Scenario: Native process execution keeps the OS sandbox boundary

- **WHEN** selected text resolves to a native subprocess such as a shell command
- **THEN** Alan projects the evaluator's allowed authority into the OS sandbox
  layer instead of assuming the subprocess can inspect or enforce the Alan
  Namespace directly

### Requirement: Buffer activity is observable as events

Alan OS SHALL expose edits, address changes, validated execution starts, and
result materializations through the buffer `event` stream using blocking-read
semantics. Execution outcome, exit state, and output SHALL be read from the
evaluator's `/proc/<pid>` files; buffer events link to that Process truth and
SHALL NOT record a parallel accepted, denied, yielded, or failed execution
status.

#### Scenario: Edit event is observed

- **WHEN** a client writes new content to `body`
- **THEN** another client reading `event` at the live edge blocks until an edit
  event is appended and then receives that event

#### Scenario: Execution-start event is observed

- **WHEN** an evaluator Process commits a valid `ctl exec` snapshot
- **THEN** a client reading `event` at the live edge receives an
  execution-started event carrying the source range, command text, and the
  evaluator's `/proc/<pid>` Path
- **AND** execution status and outcome are read from the Process files, not from
  a buffer-event status field

#### Scenario: Materialization event is observed

- **WHEN** an evaluator's finite result append commits to `body`
- **THEN** the `event` stream records the result range, committed body revision,
  and the evaluator's `/proc/<pid>` Path

## ADDED Requirements

### Requirement: Process IO is the execution output authority
An Alan Shell Evaluator Process SHALL publish output incrementally through its
ordinary `/proc/<pid>/io/output` Stream while running. Process status, exit
state, cancellation, and output SHALL remain authoritative in `/proc`; editfs
events and editable result text SHALL be linked projections rather than a
second execution state machine.

#### Scenario: Output arrives before Process exit
- **WHEN** an evaluator command produces output while it is still running
- **THEN** a client reading `/proc/<pid>/io/output` at the live edge receives the
  bytes before Process exit
- **AND** it does not wait for a terminal ProcessOutcome buffer

#### Scenario: A Process is cancelled
- **WHEN** a client writes interrupt or cancel to `/proc/<pid>/ctl`
- **THEN** the evaluator lifecycle and final state are reflected through the
  ordinary Process files
- **AND** editfs does not own a separate cancellation mechanism

### Requirement: Bounded finite UTF-8 results materialize into body
After a finite command completes successfully, the evaluator Process SHALL
retain its complete output in `/proc/<pid>/io/output` and SHALL attempt to append
bounded UTF-8 output to `body` through revision-safe file operations. A
materialization conflict SHALL NOT overwrite concurrent edits or replay the
command. The buffer event stream SHALL link the materialized body range and
revision to `/proc/<pid>`.

#### Scenario: Finite result materializes
- **WHEN** a finite evaluator command succeeds with bounded UTF-8 output and the
  body append commits
- **THEN** subsequent reads of `body` contain the appended result
- **AND** `event` identifies the result range, new body revision, and evaluator
  Process Path

#### Scenario: Body changes during materialization
- **WHEN** another client changes `body` before the evaluator's append commits
- **THEN** editfs rejects the stale commit and the evaluator may retry only a
  safe append against a newly read body end
- **AND** it never overwrites the concurrent edit or reruns the command

#### Scenario: Materialization ultimately fails
- **WHEN** a command succeeds but bounded retries cannot commit its result
- **THEN** the complete result remains readable from `/proc/<pid>/io/output`
- **AND** Process diagnostics distinguish materialization failure from command
  failure

### Requirement: Live stream output stays transient until explicit capture
A long-running `tail` command SHALL keep its source Stream Descriptor in the
evaluator Process and project new bytes through `/proc/<pid>/io/output`. It
SHALL NOT continually append live bytes into editable `body`. A client SHALL
materialize a finite snapshot explicitly, for example by stopping or
snapshotting the evaluator and running `cat` on its retained Process output.

#### Scenario: Tail remains live
- **WHEN** `/bin/run` executes `tail <path>` and the source Stream appends bytes
- **THEN** the evaluator remains running and publishes those bytes through its
  Process output Stream
- **AND** `body` does not change merely because live bytes arrived

#### Scenario: Live output is explicitly captured
- **WHEN** a client executes a finite `cat` of retained evaluator output
- **THEN** the resulting bounded UTF-8 snapshot follows the normal finite
  materialization path into `body`
