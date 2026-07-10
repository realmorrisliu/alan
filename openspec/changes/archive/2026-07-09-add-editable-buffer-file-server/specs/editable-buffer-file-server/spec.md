## ADDED Requirements

### Requirement: editfs exposes the editable buffer files

Alan SHALL provide a headless `editfs` aP file server whose root directory
contains `body`, `tag`, `addr`, `ctl`, and `event`.

#### Scenario: Root lists buffer files

- **WHEN** a client reads the `editfs` root directory
- **THEN** the listing includes `body`, `tag`, `addr`, `ctl`, and `event`

### Requirement: Body and tag edits commit on clunk

Alan SHALL treat `body` and `tag` writes as UTF-8 document edits that become
visible only when the writing fid is clunked.

#### Scenario: Body edit commits

- **WHEN** a client writes UTF-8 bytes to `body` and clunks the fid
- **THEN** subsequent reads of `body` return the committed text
- **AND** an edit event is appended to `event`

#### Scenario: Invalid UTF-8 is rejected at commit

- **WHEN** a client writes invalid UTF-8 bytes to `body` or `tag` and clunks the
  fid
- **THEN** the clunk fails with `ErrorCode::BadRequest`
- **AND** the previous text remains visible

### Requirement: Address ranges are revision-bound

Alan SHALL store the active body range in `addr` using `rev:<body-revision>
<start>..<end>` write syntax. Reads SHALL return the selected body revision,
address revision, and range as `rev:<body-revision> addr:<addr-revision>
<start>..<end>`.

#### Scenario: Address range is selected

- **WHEN** a client writes a valid current-revision range to `addr` and clunks
  the fid
- **THEN** subsequent reads of `addr` return that range
- **AND** an address event is appended to `event`

#### Scenario: Stale address fails at execution

- **WHEN** the body revision changes after an address range is selected
- **AND** a client writes `exec rev:<body-revision> addr:<addr-revision>
  <start>..<end>` to `ctl` and clunks the fid
- **THEN** the clunk fails with `ErrorCode::BadRequest`
- **AND** no command text from the stale range is executed

### Requirement: Explicit exec records accepted and denied outcomes

Alan SHALL handle `ctl exec` as an explicit policy-gated operation over a
caller-supplied `addr` snapshot that must match the active `addr` range and
body revision. Alan SHALL record the outcome in `event`.

#### Scenario: Execution is accepted by policy

- **WHEN** the editfs execution policy accepts the selected command text
- **AND** a client writes `exec rev:<body-revision> addr:<addr-revision>
  <start>..<end>` to `ctl` and clunks the fid
- **THEN** the `event` stream records an execution event with status `accepted`
  and the selected command text

#### Scenario: Execution is denied by policy

- **WHEN** the editfs execution policy denies the selected command text
- **AND** a client writes `exec rev:<body-revision> addr:<addr-revision>
  <start>..<end>` to `ctl` and clunks the fid
- **THEN** the `event` stream records an execution event with status `denied`
  and the selected command text

### Requirement: Event observation uses blocking reads

Alan SHALL expose `event` as a retained blocking-read stream.

#### Scenario: Event read blocks until activity

- **WHEN** a client reads `event` at the live edge before new activity exists
- **THEN** the read remains pending until an edit, address change, or execution
  record is appended
