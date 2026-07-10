## ADDED Requirements

### Requirement: Memory Stores own durable mutations
The selected Memory Store SHALL be the only authority that commits durable
changes to its memory documents, ledger, staging areas, or revert state. Agent
Execution Engine and models MAY propose writes but SHALL NOT directly mutate the
store's durable backing files outside the file-server contract.

#### Scenario: Runtime proposes a stable preference
- **WHEN** model-mediated planning produces a bounded stable-memory candidate
- **THEN** runtime writes the candidate to a proposal file in the selected
  writable Memory Store
- **AND** the store validates and commits or rejects the mutation

### Requirement: Write proposals and results are files
A writable Memory Store SHALL expose write proposal documents, status, result,
ledger records, and ordered write events as files under its mounted tree.
Proposal documents SHALL commit on clunk; partial writes SHALL NOT apply partial
memory mutations.

#### Scenario: A proposal commits successfully
- **WHEN** an authorized client writes a complete proposal and clunks it
- **THEN** the store atomically updates the target memory document and ledger
  record
- **AND** status, result, and events report the committed write id

#### Scenario: A proposal is invalid
- **WHEN** a proposal has an unsafe target, invalid schema, missing evidence,
  duplicate content, or insufficient rights
- **THEN** the store rejects or stages it without mutating stable memory
- **AND** the result file reports the reason

### Requirement: Store authority is independent of memory kind
Memory write proposals SHALL identify one namespace-mounted Personal,
System-Continuity, App, or Workspace Memory Store and a path within that store.
Working, episodic, semantic, and procedural memory SHALL remain usage kinds and
SHALL NOT determine ownership authority.

#### Scenario: A workspace agent proposes personal memory
- **WHEN** the agent namespace contains a writable Workspace Memory Store but no
  writable Personal Memory Store
- **THEN** the proposal cannot write the personal store
- **AND** a filename such as `USER.md` does not amplify its authority

### Requirement: Every committed stable write has a ledger record
The Memory Store SHALL create a durable ledger record for every committed stable
mutation containing the write id, namespace target, anchor or range, normalized
observation, confidence, evidence class and bounded references, rationale,
timestamps, redaction summary, and revert state.

#### Scenario: A user inspects a recent write
- **WHEN** an authorized client reads a ledger record
- **THEN** it can determine what changed, why, from which bounded evidence, and
  whether the write was reverted
- **AND** it does not need a daemon API or raw rollout path

### Requirement: Revert is store-owned lifecycle control
The Memory Store SHALL retain `/mnt/mem/<store>/writes/<write-id>/ctl` after
commit and expose precise revert there. Dated
`ledger/YYYY/MM/<write-id>.md` records are read-only audit documents. The store
SHALL verify the recorded anchor or content identity before atomically updating
the target and revert state.

#### Scenario: A write is reverted cleanly
- **WHEN** an authorized client writes `revert` to
  `/mnt/mem/<store>/writes/<write-id>/ctl` and the target still matches its
  recorded anchor
- **THEN** the store removes or reverses the memory mutation
- **AND** the ledger and events record the completed revert

#### Scenario: Manual edits conflict with revert
- **WHEN** the target no longer matches the recorded anchor
- **THEN** the store leaves the target unchanged and reports
  `manual_resolution_required`

### Requirement: Durable memory rejects plaintext secrets
The Memory Store SHALL reject or redact API keys, access tokens, passwords,
private credentials, and secret-like values before committing stable memory,
staging entries, daily notes, proposal results, or ledger evidence. Redacted
spans SHALL carry explicit reason markers distinct from truncation.

#### Scenario: Evidence contains a secret
- **WHEN** a proposal or referenced evidence contains secret-like material
- **THEN** plaintext secret content is not committed anywhere in the store
- **AND** any retained fact or evidence carries an explicit redaction marker

### Requirement: Reverted memory is absent from prompt-facing reads
A successful revert SHALL remove the reverted fact from current memory documents
or mark it with store metadata that every prompt-facing reader excludes. Ledger
history MAY retain the redacted prior observation for audit.

#### Scenario: A reverted fact is recalled
- **WHEN** runtime builds recall or handoff context after a successful revert
- **THEN** the reverted fact is not included as current memory
- **AND** the ledger remains separately inspectable

### Requirement: Disabled proactive memory withholds write capability
When proactive memory is disabled, Agent Execution Engine SHALL skip proactive
candidate planning and the Agent Process SHALL receive no writable proactive
memory proposal surface unless separately authorized.

#### Scenario: Memory is disabled
- **WHEN** an agent starts with proactive memory disabled
- **THEN** no stable, staged, inbox, daily-note, consolidation, or ledger write is
  attempted
- **AND** absence of a writable store surface prevents accidental mutation
