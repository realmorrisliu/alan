# runtime-memory-contract Specification

## Purpose
Defines runtime-memory contracts for Memory Store layout, recall and write paths,
explicit dual-threshold compaction coordination, human-readable surfaces,
provenance, and truncation behavior.
## Requirements
### Requirement: Memory kind is separate from memory authority
alan SHALL treat working, episodic, semantic, and procedural memory as
agent-cognitive memory kinds rather than ownership buckets. Ownership and access
authority SHALL be modeled separately by Memory Stores such as personal,
system-continuity, app, and mounted-domain stores.

#### Scenario: Memory store is described
- **WHEN** runtime code, docs, or specs describe where memory is owned or who may
  authorize access
- **THEN** they use Memory Store language rather than redefining memory kinds as
  User Memory, System Memory, or App Memory
- **AND** Agent Processes access memory stores only through namespace-bound
  paths, Descriptors, and Access Checks — a Memory Store is bound into the
  agent's namespace — not through an Agent Run or Context Grant API (retired by
  ADR-0024)

### Requirement: Runtime validates model-mediated memory write plans
alan SHALL use model-mediated semantic judgment for automatic memory promotion
while keeping trigger timing, validation, provenance, and durable writes under
runtime authority.

Write-plan contract:

1. Runtime chooses when to invoke write planning and which active-turn messages
   are in scope.
2. The model returns bounded structured output with `kind`, canonical target,
   confidence, disposition, observation, evidence, and promotion rationale.
3. Runtime validates and canonicalizes the output before any file write.
4. Runtime remains the only component allowed to mutate memory files.
5. Invalid, mismatched, or over-broad candidates are dropped rather than
   written.
6. Low-confidence or ambiguous candidates fall back to inbox staging.

Direct stable writes require a validated `promote_now` disposition and at least
one of:

1. the user explicitly says to remember it
2. the user directly states the fact as stable identity, preference, or
   constraint
3. the user authorizes a source lookup that directly states the fact
4. the fact is already in stable memory and the new turn updates it

Observed captures that are useful but not stable enough become inbox entries or
daily notes rather than stable memory.

#### Scenario: Write plan is over-broad
- **WHEN** the model proposes a memory write that spans unrelated facts,
  mismatches its target, lacks evidence, or exceeds the bounded schema
- **THEN** runtime rejects or stages the candidate instead of mutating stable
  memory directly

#### Scenario: User asks alan to remember a stable preference
- **WHEN** a validated write plan marks the preference as `promote_now`
- **THEN** runtime writes the durable change and preserves source evidence

### Requirement: Memory remains auditable, private, and rollout-linked
alan SHALL preserve textual source trails and avoid silently absorbing sensitive
or unsupported data into stable memory.

Audit and privacy rules:

1. Tape and rollout remain the high-fidelity execution record.
2. Episodic records, handoffs, and topic pages are curated projections derived
   from rollout and tape evidence.
3. Retrieval prefers curated text before raw rollout.
4. Raw rollout grep is a fallback, not the primary recall mechanism.
5. Stable memory does not silently absorb sensitive data.
6. User-facing identity memory favors explicit confirmation or user-authorized
   source verification.
7. Rewrites and deletions leave a visible trail through summaries, frontmatter
   status, or source references.

#### Scenario: Sensitive fact appears in a turn
- **WHEN** a turn contains potentially sensitive information without stable
  memory intent or confirmation
- **THEN** alan avoids silently promoting it into stable memory and preserves
  only appropriate provenance or candidate state

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
- **THEN** the runtime reads the visible personal, system-continuity, app, and mounted-domain Memory Stores
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

### Requirement: Compaction configuration uses explicit dual thresholds
Alan SHALL configure context compaction with
`compaction_soft_trigger_ratio` and `compaction_hard_trigger_ratio`. The retired
single-threshold `compaction_trigger_ratio` field SHALL be unavailable and SHALL
fail normal unknown-field validation.

#### Scenario: Dual thresholds are configured
- **WHEN** valid soft and hard compaction ratios are present in agent
  configuration
- **THEN** Alan validates and applies the two thresholds to compaction
  coordination

#### Scenario: Retired single threshold is configured
- **WHEN** agent configuration contains `compaction_trigger_ratio`
- **THEN** configuration loading fails and identifies the unknown field
- **AND** Alan does not copy that value into either current threshold

#### Scenario: Thresholds are omitted
- **WHEN** neither current compaction threshold is configured
- **THEN** Alan uses the current soft and hard defaults
- **AND** no deprecated single-threshold default participates in resolution

### Requirement: Memory authority is not a workspace directory
Runtime memory SHALL be accessed through explicit Memory Store file trees or
descriptors and persisted by their owning service backing. Agent Process boot,
recall, flush, and promotion MUST NOT infer `<host-dir>/.alan` memory paths.

#### Scenario: Agent receives a Memory Store
- **WHEN** an Agent Process is launched with a Memory Store descriptor
- **THEN** recall and writes use that tree
- **AND** Host cwd contributes no implicit memory authority
