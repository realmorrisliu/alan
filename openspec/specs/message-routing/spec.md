# message-routing Specification

## Purpose
Defines `routefs` typed-message routing by file-backed rules, clunk-committed
message framing, auditable delivery, blocking receiver streams, and its role as
a composition mechanism rather than a control path.

## Requirements

### Requirement: Routefs routes typed messages by rule, sender decoupled
alan9 SHALL provide `routefs`, a file server where a sender writes a typed
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
alan9 SHALL append every routed message to an observable log stream, and rule
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
alan9 SHALL treat message routing as a way to compose actors, not as the primary
control path. An agent's own loop and governance SHALL remain explicit; critical
control SHALL NOT be expressed only through routing rules.

#### Scenario: Governance routing is used
- **WHEN** a result needs human judgment
- **THEN** it routes to a human inbox port, and the rule that decides this is an
  inspectable file
- **AND** the approval itself is still an explicit action (a request/response
  per the agent file-layout contract), not an implicit routing side effect

### Requirement: Routefs has a canonical namespace path
alan9 SHALL implement `routefs` as a user-space file server over the aP
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

### Requirement: Cross-workstream handoffs route through Root over routefs with loop bounds
When a focused child workstream receives input or discovers a dependency belonging to another stream, it SHALL emit a typed handoff message to `/mnt/route/send` instead of calling sibling Processes directly. Root SHALL consume `/mnt/route/ports/root`, log decisions to `/mnt/route/log`, enforce `max_handoff_hops = 2` with `dead-letter` escalation, and never widen authority.

#### Scenario: Focused stream reroutes misdirected prompt through Root to target stream
- **WHEN** a user focused on stream A submits a prompt belonging to stream B
- **THEN** stream A writes a typed `reroute_request` message to `/mnt/route/send`
- **AND** Root consumes the message, appends an audit record to `/mnt/route/log`, and dispatches the task to stream B

#### Scenario: Handoff exceeding loop bound escalates to Root
- **WHEN** a routed handoff message reaches `hop_count > 2`
- **THEN** `routefs` delivers it to `dead-letter` and Root surfaces the ambiguity to the user instead of forwarding in a loop

#### Scenario: Cross-stream handoff never widens authority
- **WHEN** stream A triggers a handoff to stream B
- **THEN** stream B executes strictly under its own validated Host Mount grants and `task_budget` without inheriting extra privileges from stream A

### Requirement: Routefs executes as a crash-isolated ActorFileServer
`alan-routefs` SHALL execute its routing state inside a single-owner
`FileServerActor` spawned via `spawn_file_actor`, preserving all rule-matching,
`DEAD_LETTER_PORT`, and `MAX_HANDOFF_HOPS` semantics without holding shared
`tokio::sync::Mutex` guards across `.await`.

#### Scenario: Blocked port readers do not stall send commits on RouteFs
- **WHEN** a subscriber blocks reading `/mnt/route/ports/root` while senders commit documents to `/mnt/route/send`
- **THEN** `send` commits complete immediately and wake the blocked port reader with the routed record

#### Scenario: RouteFs panic isolates failure cleanly
- **WHEN** the `RouteFs` actor encounters an injected panic inside its mailbox loop
- **THEN** in-flight and new `RouteFs` calls fail with `ErrorCode::Io` without poisoning kernel mounts and notify `ActorExitHandle`
