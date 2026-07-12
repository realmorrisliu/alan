# message-routing Specification

## Purpose
Defines `routefs` typed-message routing by file-backed rules, clunk-committed
message framing, auditable delivery, blocking receiver streams, and its role as
a composition mechanism rather than a control path.
## Requirements
### Requirement: Routefs routes typed messages by rule, sender decoupled
Alan OS SHALL provide `routefs`, a file server where a sender writes a typed
message to a `send` file and rule files route it by content/type to a destination
port. The sender SHALL NOT name the receiver; the rules SHALL decide the
destination. A receiver SHALL consume its port as a stream (blocking read). A
message MAY span multiple aP writes; routefs SHALL frame it as one message and
match/route only on clunk (close) of the `send` fid — never on a partial write —
so large messages route deterministically (the same commit-on-clunk framing as
the llmfs request).

#### Scenario: A typed message is routed
- **WHEN** a sender writes a typed message (for example "patch" or "citation") to
  `send`, possibly over several writes, then clunks the `send` fid
- **THEN** the complete message is matched and routed to a destination port at
  clunk (not on a partial write)
- **AND** the sender did not name the receiving actor

#### Scenario: Handoff is by type, not by actor
- **WHEN** an agent finishes work and emits a result type
- **THEN** the rules dispatch it (for example to a review agent, an apply-patch
  tool, or a human inbox)
- **AND** the agent does not hardcode a call to a specific actor

### Requirement: Routing is auditable and never silently drops
Alan OS SHALL append every routed message to an observable log stream, and rule
files SHALL be plain inspectable files. Rule matching SHALL be deterministic, and
a message that matches no rule SHALL go to a default dead-letter port rather than
being dropped.

#### Scenario: Routing is inspected
- **WHEN** an operator needs to see what was routed where
- **THEN** they read the message log stream and the rule files
- **AND** no routing happens through a hidden side channel

#### Scenario: A message matches no rule
- **WHEN** a message matches no rule
- **THEN** it is delivered to a default dead-letter port
- **AND** it is recorded in the log, not silently discarded

### Requirement: Routing is a composition mechanism, not the control path
Alan OS SHALL treat message routing as a way to compose actors, not as the primary
control path. An agent's own loop and governance SHALL remain explicit; critical
control SHALL NOT be expressed only through routing rules.

#### Scenario: Governance routing is used
- **WHEN** a result needs human judgment
- **THEN** it routes to a human inbox port, and the rule that decides this is an
  inspectable file
- **AND** the approval itself is still an explicit action (a request/response
  per the agent file-layout contract), not an implicit routing side effect

### Requirement: Routefs has a canonical namespace path
Alan OS SHALL implement `routefs` as a user-space file server over the aP
protocol with one canonical namespace path (ADR-0025 D3): it posts a handle at
`/srv/route` and serves its tree (`send`, rules, ports, log) at `/mnt/route`. It
SHALL NOT be kernel state, and the ownership map SHALL record `/mnt/route` as its
owned tree.

#### Scenario: Routefs is mounted
- **WHEN** `routefs` starts
- **THEN** it posts a handle at `/srv/route` and serves `send`, rule files,
  ports, and the message log under `/mnt/route`
- **AND** clients use that canonical path rather than choosing their own location
- **AND** ports are blocking-read streams per the kernel stream model
