## ADDED Requirements

### Requirement: Agent knowledge is content-addressed and deduplicated
Alan OS SHALL store agent knowledge blocks (tape, memory, context) under the hash
of their content, write-once. Writing identical content SHALL be idempotent and
SHALL store one copy (dedup), including across different agents.

#### Scenario: Identical content is stored once
- **WHEN** two agents write the same context block
- **THEN** the store keeps a single copy addressed by its content hash
- **AND** both agents reference it by the same hash

#### Scenario: A block is immutable
- **WHEN** knowledge changes
- **THEN** new blocks are written and addressed by their new content hash
- **AND** existing blocks are never overwritten in place

### Requirement: Checkpoints are root hashes; forks are cheap
Alan OS SHALL model agent state (such as `machine/tape` or a memory version) as a
Merkle DAG of content-addressed blocks whose root hash is a checkpoint. Forking an
agent from any checkpoint SHALL share all unchanged blocks and write only the
delta.

#### Scenario: A checkpoint is taken
- **WHEN** an agent reaches a point worth saving
- **THEN** the checkpoint is the root hash of its current knowledge DAG
- **AND** restoring it later retrieves exactly that state by hash

#### Scenario: An agent is forked
- **WHEN** an agent is forked from a checkpoint
- **THEN** the fork shares all unchanged blocks and writes only what diverges
- **AND** forking does not copy the whole tape

### Requirement: History is tamper-evident and verifiable
Alan OS SHALL make agent history verifiable by content hash: a root hash
integrity-checks the state it names, and altering past content SHALL change the
hash.

#### Scenario: Audit verifies a claim
- **WHEN** an audit needs to confirm what an agent saw or did at a point
- **THEN** it reads that state through a permitted namespace path and verifies it
  against the recorded root hash
- **AND** any silent rewrite is detectable because the hash would differ

### Requirement: A content hash is not a capability
Alan OS SHALL NOT let possession of a content hash grant access to content.
Retrieval SHALL be gated by namespace reachability and access rights (ADR-0024
D6): a process reads only blocks reachable from roots bound into its namespace,
subject to access checks. The hash verifies integrity; it does not authorize
retrieval. Cross-agent dedup SHALL be internal to the store and SHALL NOT make
one agent's private content readable by another that merely knows or guesses its
hash.

#### Scenario: A hash is presented without access
- **WHEN** a process presents a content hash for content not reachable in its
  namespace
- **THEN** retrieval is denied by access checks
- **AND** the hash alone is not a global capability that bypasses the namespace

#### Scenario: Dedup does not leak across agents
- **WHEN** two agents happen to store identical content (stored once by hash)
- **THEN** each can read it only through its own namespace and access rights
- **AND** dedup does not expose one agent's private content to another

### Requirement: Storage is bounded by reachability GC and retention
Alan OS SHALL bound knowledge storage by garbage-collecting blocks unreachable
from any live root past a retention policy. It SHALL NOT keep all content forever
(no Venti-style immortality). Retention and GC policy SHALL belong to the storing
file server, not the kernel.

#### Scenario: Unreachable blocks are collected
- **WHEN** a fork or checkpoint is discarded and its blocks are referenced by no
  live root past retention
- **THEN** those blocks become eligible for garbage collection
- **AND** still-referenced blocks (including shared ones) are retained

#### Scenario: Audited state is retained
- **WHEN** a retention policy must preserve audited history
- **THEN** the policy can pin the relevant roots so GC does not drop them
- **AND** pinning is a retention decision, not a kernel concept

### Requirement: File surfaces are views over the store
Alan OS SHALL keep agent-facing surfaces as files: `machine/tape`, memory, and
context are read as files materialized from the content-addressed DAG. Content
addressing SHALL be the backing model, not a new agent-facing API.

#### Scenario: Tape is read
- **WHEN** a client reads `machine/tape`
- **THEN** it reads a file view materialized from the content-addressed DAG
- **AND** it does not need to know hashes or the DAG to read the tape

#### Scenario: The durable home is backed by the store
- **WHEN** an agent's durable home (ADR-0024 D7) persists tape and memory
- **THEN** it is backed by the content-addressed store and resumes from a root
  hash
- **AND** an ephemeral (for example tmpfs) home simply does not persist roots
