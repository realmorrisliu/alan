## REMOVED Requirements

### Requirement: The client never silently drops events across a reconnect

**Reason**: The requirement defines event delivery through a client reconnect and daemon replay-buffer contract.

**Migration**: Preserve ordered delivery through offset-readable AgentFS streams and explicit retention-gap handling.

## ADDED Requirements

### Requirement: Renderer file streams preserve offsets across reattachment

A renderer reading an offset-addressable AgentFS stream SHALL retain its last delivered offset and SHALL NOT silently omit data when reopening the file or reattaching to the Agent Process. Overlap SHALL be deduplicated by stable file offset or record identity, and an unrecoverable retention gap SHALL be surfaced.

#### Scenario: Records written during reattachment are read

- **WHEN** a renderer's file watch ends and records are appended before it opens the stream again
- **THEN** the renderer resumes from its last delivered offset
- **AND** it delivers retained records in order before following new appends

#### Scenario: Snapshot and stream overlap is deduplicated

- **WHEN** hydrated snapshot state and an offset-readable stream contain the same durable record
- **THEN** the renderer presents the record once using its stable identity or offset

#### Scenario: Retention gap is surfaced

- **WHEN** the requested offset is older than retained stream data
- **THEN** the renderer reports a recoverable gap instead of pretending the stream is continuous
- **AND** recovery proceeds through current AgentFS snapshot and file semantics
