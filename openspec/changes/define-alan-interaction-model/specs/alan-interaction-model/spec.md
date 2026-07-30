## ADDED Requirements

### Requirement: The file system is the API, not the default UI
Alan renderer hosts SHALL treat the namespace as the shared-truth substrate
and SHALL NOT require users to understand OS concepts — entering an OS,
starting a shell, mounting paths, walking namespaces, or addressing PIDs — to
accomplish any default workflow. Default workflows SHALL be expressed through
user objects: intent, agents, work, results, conversations, folders,
permissions, services, and rules.

#### Scenario: A new user completes a task without OS vocabulary
- **WHEN** a user gives an intent, an agent works on it, and the user reviews
  the outcome
- **THEN** every step is presented through user objects and native
  affordances
- **AND** no step requires typing or understanding `mount`, namespace paths,
  fids, or `/proc` addresses

### Requirement: Three disclosure layers over one truth
Alan renderer hosts SHALL provide three disclosure layers — Intent, Work, and
Files — as live views of the same mounted file state per ADR-0046. The Intent
layer SHALL present stating goals and receiving outcomes. The Work layer
SHALL render agent file surfaces as domain-native affordances: conversation
views from tape surfaces, plan cards from plan surfaces, approval sheets from
pending-action surfaces, result and evidence views from rollout and
checkpoint evidence surfaces per the evidence-retention contracts,
and lifecycle controls. A Stop control SHALL write `/proc/<pid>/ctl`
(interrupt/cancel). Any further lifecycle control SHALL write the control
surface that owns the corresponding semantics per the canonical AgentFS and
Agent Machine contracts and SHALL NOT be mapped onto `/proc/<pid>/ctl` unless
that surface owns it. The Files layer SHALL
expose the raw namespace as an explicit inspect-and-program mode. No layer
SHALL own copied runtime state.

#### Scenario: A user peels back an approval to its file truth
- **WHEN** a user views an approval sheet in the Work layer and switches to
  the Files layer
- **THEN** the same pending action is visible as its backing files
- **AND** acting in either layer is reflected in the other without
  synchronization state

#### Scenario: Renderer state remains references only
- **WHEN** a renderer presents any layer
- **THEN** its durable presentation state remains Process References, offsets,
  and display state per ADR-0046
- **AND** no layer introduces a renderer-owned copy of tape, machine, or
  service truth

### Requirement: Conversation is one of three interaction modes
Alan renderer hosts SHALL support conversation and background-servant
interaction as first-class modes. Conversation SHALL NOT be assumed as the
entry or primary posture. Background-servant mode SHALL let
agents run detached — closing a view detaches per ADR-0047 and never implies
stopping execution — and SHALL present completed work for review rather than
requiring the user to watch execution. Completed outcomes and their retained
evidence references SHALL remain discoverable after Process exit and Alan OS
Host restart through the Agent Runtime Service-owned read-only Rollout history
surface; renderer hosts SHALL NOT scan System Store backing or persist a
private results database. Event-driven interaction — standing
rules that cause agents to act, with proactive reports — is the recorded
third mode and the designated direction of this model; because no runtime or
service contract yet owns rule storage and trigger semantics, renderer hosts
SHALL implement it only once such an owning contract lands, and SHALL keep
the review surface unified so event-driven outcomes join user-dispatched work
when it does.

#### Scenario: The user reviews instead of watching
- **WHEN** a user dispatches work and closes its view
- **THEN** the agent keeps running and the finished result appears in the
  review surface with its evidence
- **AND** no UI element required the user to keep a conversation or execution
  view open

#### Scenario: The user reviews work after an Alan OS restart
- **WHEN** an Agent Process completed before the current Alan OS Host boot
- **THEN** its outcome and retained evidence references remain discoverable
  from its retained Rollout through the Rollout history surface
- **AND** the renderer reconstructs the review from mounted files without a
  renderer-owned results database

#### Scenario: Event-driven mode awaits its owning contract
- **WHEN** no runtime or service contract yet owns event rules and triggers
- **THEN** renderer hosts are not required to present a rules surface or
  proactive reports
- **AND** the review surface remains the designated destination for
  event-driven outcomes once the owning contract lands

### Requirement: Permission is the UX of mounting
Granting an agent access to Host files SHALL be presented as a permission
flow — drag-in, system file picker, or an approval sheet for an
agent-originated request — that creates a Host Mount Service grant per
ADR-0050. Mount and bind SHALL be side effects invisible to the default UI.
A single Permissions surface SHALL list active grants by label, scope, and
access, and revocation SHALL remove the projection. Raw Host paths SHALL NOT
appear in any layer; default layers SHALL show grant labels only, and `/mnt`
projection paths MAY appear only in the Files layer and power-user surfaces.

#### Scenario: A user shares a folder with an agent
- **WHEN** a user gives an agent access to a folder through any default flow
- **THEN** a grant is created through Host Mount Service and the folder
  appears to the agent under its `/mnt` projection
- **AND** the user never names or sees a mount command, a bind, or the raw
  Host path

#### Scenario: An agent requests new access mid-run
- **WHEN** a running agent needs access beyond its current grants
- **THEN** the user is shown an approval sheet naming the folder label and
  requested access
- **AND** approving creates the grant; denying publishes an immutable terminal
  `rejected` result per the Host Mount Service contract, creating no grant
  and no projection, so the waiting Agent Process resumes

#### Scenario: The user audits and revokes access
- **WHEN** a user opens the Permissions surface
- **THEN** every active grant is listed by label, scope, and access
- **AND** revoking a grant removes its projection without affecting authored
  content

### Requirement: OS vocabulary is quarantined from default UI copy
Default UI copy in every renderer host SHALL name user objects and SHALL NOT
name OS objects — mount, namespace, fid, descriptor, `/proc`, tape, rollout,
or qid. Concepts that need user-facing names SHALL use them: execution
records are "history" or "evidence"; grants are "permissions"; bound folders
are "shared folders". OS vocabulary MAY appear in the Files layer, power-user
surfaces, debugging views, and documentation.

#### Scenario: A default screen is reviewed
- **WHEN** any default-UI screen in a renderer host is reviewed
- **THEN** its copy names only user objects
- **AND** any OS term is confined to the Files layer, a power-user surface, or
  a debug view
