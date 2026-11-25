# branching-execution-file-server Specification

## Purpose
Defines the headless `branchfs` tree for forking visible execution branches,
explicit scoring and selection, evidence-preserving discard, and blocking
branch observation.
## Requirements
### Requirement: branchfs exposes a branching execution tree

Alan SHALL provide a headless `branchfs` aP file server whose root directory
contains `ctl`, `branches`, `selected`, and `events`.

#### Scenario: Root lists branch files

- **WHEN** a client reads the `branchfs` root directory
- **THEN** the listing includes `ctl`, `branches`, `selected`, and `events`

### Requirement: Branches fork from visible branch roots

Alan SHALL create branch candidates by writing a complete fork command to `ctl`
and clunking the fid. A fork command SHALL name an existing visible branch id as
its source; a bare content hash SHALL NOT authorize branch creation.

#### Scenario: Candidate branch is forked

- **WHEN** `branches/base` is visible
- **AND** a client writes a fork command naming `from = "base"` to `ctl` and
  clunks the fid
- **THEN** `branches/<candidate>` appears as an inspectable JSON file
- **AND** the candidate root is a content-addressed fork sharing unchanged state
  with the base root
- **AND** a fork event is appended to `events`

#### Scenario: Unknown source branch is rejected

- **WHEN** a client writes a fork command naming a source branch that is not
  visible under `branches/`
- **THEN** the clunk fails with `ErrorCode::NotFound`
- **AND** no candidate branch is created

### Requirement: Branches can be scored explicitly

Alan SHALL allow clients to record an explicit score and summary for a visible
branch by writing a score command to `ctl`.

#### Scenario: Branch is scored

- **WHEN** a client scores a visible branch through `ctl`
- **THEN** `branches/<id>` reports the score and summary
- **AND** a score event is appended to `events`

### Requirement: Branch selection is explicit

Alan SHALL publish the selected branch through the `selected` file only after a
client writes an explicit select command to `ctl`.

#### Scenario: Branch is selected

- **WHEN** a client selects a visible branch through `ctl`
- **THEN** `selected` reports that branch id and root hash
- **AND** the selected branch remains inspectable under `branches/`
- **AND** a select event is appended to `events`

### Requirement: Branches can be discarded without deleting lifecycle evidence

Alan SHALL allow clients to discard a visible branch through `ctl`. Discarded
branches SHALL stop appearing under `branches/`, while the event stream retains
the discard record.

#### Scenario: Branch is discarded

- **WHEN** a client discards a visible branch through `ctl`
- **THEN** `branches/` no longer lists that branch
- **AND** a discard event is appended to `events`

### Requirement: Branch observation uses blocking reads

Alan SHALL expose `events` as a retained blocking-read stream of JSON-line branch
lifecycle records.

#### Scenario: Event read blocks until branch activity

- **WHEN** a client reads `events` at the live edge before new branch activity
  exists
- **THEN** the read remains pending until a fork, score, select, or discard
  record is appended

### Requirement: branchfs coordinates CoW workspace candidates without Host paths
`alan-branchfs` (`/mnt/branch`) SHALL allow `BranchRecord` entries to attach opaque `workspace_ref` handles, backing kinds, diff summaries, and check summaries alongside scores and selection state, while rejecting raw Host filesystem paths in `ctl` commands and branch records.

#### Scenario: Candidate workspace metadata publishes through branches without leaking Host paths
- **WHEN** Host Mount Service backs a forked branch with a CoW workspace and records diff and test summaries
- **THEN** `/mnt/branch/branches/<id>` exposes the opaque `workspace_ref`, diff summary, and verifier score
- **AND** no raw Host backing directory path appears in `branches/<id>` or `events`

#### Scenario: Discarding or exiting cleans physical clones while preserving evidence
- **WHEN** a candidate branch is discarded via `ctl` or an interactive instance exits before applying a selected workspace
- **THEN** Host adapters delete the physical CoW clone directory immediately (persisting unified patch evidence on exit for unapplied winners)
- **AND** `/mnt/branch/events` retains the lifecycle audit record
