## ADDED Requirements

### Requirement: Local system root is exportable over aP
The generic aP export/import implementation SHALL carry the Alan OS mounted
root across a Unix socket while preserving fid, typed failure, blocking stream,
and commit-on-clunk semantics.

#### Scenario: Client tails Agent output remotely in-process boundary
- **WHEN** an imported client reads a live AgentFS stream at its current offset
- **THEN** the read blocks and later returns appended bytes exactly as the
  in-process transport would
