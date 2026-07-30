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
Alan renderer hosts SHALL support conversation. A local renderer host attached
through the Local Entry Login Namespace SHALL additionally support
background-servant interaction as a first-class mode. Conversation SHALL NOT
be assumed as the entry or primary posture for such a local renderer.
Background-servant mode SHALL let agents run detached — closing a view detaches
per ADR-0047 and never implies stopping execution — and SHALL present completed
work for review rather than requiring the user to watch execution. A local
background dispatch SHALL request the strict-durability launch behavior in an
`AgentExecutableRequest` committed through Agent Runtime Service-owned
`/mnt/agent-runtime/clone`. Service Manager SHALL mount this capability only in
the Local Entry Login Namespace; Remote Entry and Agent Process namespaces
SHALL NOT inherit it. The resulting ordinary Agent Process SHALL be parented
by the current Root Agent Process, not by the renderer-attached Shell Process.
Before opening
`/mnt/agent-runtime/clone`, the renderer SHALL read `/proc/host/boot_id` and
list the currently discoverable `rollout_id` values. The dispatch SHALL be
acknowledged as accepted only after
`/agent/rollouts` exposes a valid producing Rollout whose ID was absent from
that pre-spawn listing, whose first-record `process_path` matches the returned
`/proc/<pid>`, and whose current `/proc/host/boot_id` still matches the pinned
value. If the boot identity changes or the Process exits before that new
Rollout is discoverable, the dispatch SHALL fail rather than continue as
untracked background work.

Strict durability SHALL guarantee that an accepted dispatch established its
producing Rollout; it SHALL NOT guarantee that a later terminal storage write
cannot fail. Completed outcomes from accepted local background dispatches
SHALL remain discoverable after Process exit and Alan OS Host restart only
when their terminal `process_exit` was successfully persisted. Retained
evidence references SHALL remain discoverable independently. A Rollout whose
terminal persistence failed SHALL appear only as unterminated or incomplete
evidence, and a renderer SHALL NOT infer completion or fabricate its missing
`AgentExecutableResult`. Best-effort foreground conversation without a Rollout
SHALL NOT carry this durable-evidence guarantee. Renderer hosts SHALL NOT scan
System Store backing or persist a private results database. This change SHALL
NOT require a Remote Entry renderer to offer background dispatch and SHALL NOT
grant it `/mnt/agent-runtime/clone`; remote launch authority and revocation
require a separate owning contract. Event-driven interaction —
standing
rules that cause agents to act, with proactive reports — is the recorded
third mode and the designated direction of this model; because no runtime or
service contract yet owns rule storage and trigger semantics, renderer hosts
SHALL implement it only once such an owning contract lands, and SHALL keep
the review surface unified so event-driven outcomes join user-dispatched work
when it does.

#### Scenario: The user reviews instead of watching
- **WHEN** a local renderer user closes the view of a strict-durability
  background dispatch launched through `/mnt/agent-runtime/clone` and whose
  producing Rollout was correlated through `/agent/rollouts`, and terminal
  `process_exit` later persists successfully
- **THEN** the agent keeps running and the finished result appears in the
  review surface with its evidence
- **AND** no UI element required the user to keep a conversation or execution
  view open

#### Scenario: The user reviews work after an Alan OS restart
- **WHEN** a Rollout-backed background Agent Process persisted its terminal
  `process_exit` before the current Alan OS Host boot
- **THEN** its outcome and retained evidence references remain discoverable
  from its retained Rollout through the Rollout history surface
- **AND** the renderer reconstructs the review from mounted files without a
  renderer-owned results database

#### Scenario: Terminal outcome cannot be persisted
- **WHEN** an accepted strict-durability background dispatch exits but its
  terminal `process_exit` cannot be appended or flushed
- **THEN** its retained Rollout remains discoverable as unterminated or
  incomplete evidence
- **AND** the renderer does not label it completed or fabricate the missing
  `AgentExecutableResult`
- **AND** strict durability remains satisfied only in its launch-time promise
  that the producing Rollout was established

#### Scenario: Background dispatch cannot create a Rollout
- **WHEN** a strict-durability Process launched through
  `/mnt/agent-runtime/clone` exits before a matching producing Rollout absent
  from the pre-spawn listing becomes discoverable
- **THEN** the background dispatch fails explicitly
- **AND** no best-effort in-memory execution is acknowledged as reviewable
  background work

#### Scenario: Alan OS Host restarts during background dispatch
- **WHEN** `/proc/host/boot_id` changes before the producing Rollout is
  acknowledged
- **THEN** the background dispatch fails explicitly
- **AND** no Rollout from the new boot is associated with the prior request

#### Scenario: A renderer attaches through Remote Entry
- **WHEN** a renderer receives a Remote Entry namespace without
  `/mnt/agent-runtime/clone`
- **THEN** it remains conformant by providing conversation and the disclosure
  layers reachable through that namespace
- **AND** this change does not require background dispatch or synthesize a
  remote launch path

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
