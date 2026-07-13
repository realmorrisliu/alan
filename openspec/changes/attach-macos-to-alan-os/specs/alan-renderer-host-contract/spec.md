## ADDED Requirements

### Requirement: Renderer attachments use Process Reference and offsets
An Alan renderer host SHALL identify a Process by boot ID and PID, verify it
through `/proc`, and own the byte offsets of each stream it reads. Recreating or
duplicating a renderer MUST NOT create, restore, or become authority for the
Process.

#### Scenario: Second renderer attaches
- **WHEN** another renderer opens the same Agent Process
- **THEN** it maintains independent fids and offsets
- **AND** both observe the same Alan OS file authority

### Requirement: Retention gaps are visible
When a saved stream offset can no longer be served, the renderer SHALL surface
the gap and MUST NOT silently jump forward or claim complete continuity.

#### Scenario: Saved offset predates retained history
- **WHEN** a renderer reattaches after retention has truncated earlier bytes
- **THEN** it reports the missing range before continuing
