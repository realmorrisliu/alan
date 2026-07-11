## REMOVED Requirements

### Requirement: Memory contracts live in OpenSpec
**Reason**: The existing block defines part of its scope through Session summaries.
**Migration**: Use the added Process-shaped memory contract and the unchanged OpenSpec ownership rule.

### Requirement: Memory surfaces remain human-readable and provenance-aware
**Reason**: The existing provenance vocabulary includes Session context and Session summary ownership.
**Migration**: Use Agent Process, turn, rollout/checkpoint, handoff, and Memory Store provenance.

### Requirement: Memory vocabulary and kinds are stable
**Reason**: Working, episodic, summary, and handoff kinds are defined around Session boundaries.
**Migration**: Use the added Process-shaped memory vocabulary.

### Requirement: Pure-text memory layout is inspectable
**Reason**: The canonical layout keys working and episodic records by Session identity.
**Migration**: Use the added Process-shaped pure-text layout.

### Requirement: Memory file contracts preserve required sections and provenance
**Reason**: Required frontmatter and sections carry `session_id` and Session summary fields.
**Migration**: Use Agent Process, turn, rollout/checkpoint, source evidence, and handoff fields.

### Requirement: Runtime owns session bootstrap and pre-turn recall
**Reason**: Session bootstrap is removed as an Agent Runtime concept.
**Migration**: Use Agent Process bootstrap and pre-turn recall.

### Requirement: Session finalization and consolidation keep memory curated
**Reason**: Session finalization is removed as a lifecycle boundary.
**Migration**: Curate memory at Agent Process exit, explicit handoff, durable checkpoint, or service-owned maintenance boundaries.

### Requirement: Memory and compaction remain adjacent but distinct
**Reason**: The existing distinction is expressed as current-Session versus cross-Session lifetime.
**Migration**: Use Agent Machine context pressure versus Memory Store continuity.

## ADDED Requirements

### Requirement: Memory contracts use Process-shaped provenance
Memory records SHALL identify their owning Agent Process, contributing turns, rollout/checkpoint
evidence, and Memory Store authority. Working Memory SHALL be local to one Agent Process; Episodic
Memory SHALL describe past Agent Process activity and handoffs.

#### Scenario: Runtime writes working memory
- **WHEN** an Agent Process records state needed to continue its current task
- **THEN** the record identifies the owning Agent Process and source evidence
- **AND** it records contributing turns and durable record identifiers when applicable

### Requirement: Pure-text memory layout is Process-shaped and inspectable
Memory Stores SHALL keep human-readable text records under owner-defined working, episodic, daily,
topic, handoff, user, and system-continuity trees. Generated working and episodic paths SHALL use
Process or durable-record identity.

#### Scenario: A human inspects generated memory
- **WHEN** a user walks the Memory Store after Agent Process activity
- **THEN** working, episodic, handoff, daily, and topic records are readable as text
- **AND** generated records are located by their Memory Store kind and Process or durable-record provenance

### Requirement: Agent Process bootstrap owns pre-turn recall
Agent Runtime Service SHALL assemble pre-turn recall when an Agent Process starts and before each
turn from the Memory Stores visible in that Process namespace.

#### Scenario: A new Agent Process starts
- **WHEN** memory is enabled for a newly spawned Agent Process
- **THEN** the runtime reads the visible user, system-continuity, app, and workspace Memory Stores
- **AND** the selected recall becomes input to that Process's initial Agent Machine state

### Requirement: Process and service boundaries curate durable memory
Alan SHALL curate episodic memory and handoff state at explicit handoff, durable checkpoint, Agent
Process exit, or Memory Store maintenance boundaries. Compaction SHALL protect the current Agent
Machine from context pressure, while Memory Stores preserve continuity beyond one Agent Process.

#### Scenario: An Agent Process exits after substantive work
- **WHEN** the owning policy requires continuity capture
- **THEN** the runtime writes an episodic record and handoff with Process and rollout/checkpoint
  provenance
- **AND** the resulting records are published through their owning Memory Stores
