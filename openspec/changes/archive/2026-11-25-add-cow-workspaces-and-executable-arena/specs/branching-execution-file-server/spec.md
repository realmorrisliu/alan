# Spec Delta

## ADDED Requirements

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
