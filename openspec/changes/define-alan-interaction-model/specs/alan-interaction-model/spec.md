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
pending-action surfaces, result and evidence views from execution records,
and Stop/Pause controls that write `/proc/<pid>/ctl`. The Files layer SHALL
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
Alan renderer hosts SHALL support conversation, background-servant, and
event-driven interaction as first-class modes. Conversation SHALL NOT be
assumed as the entry or primary posture. Background-servant mode SHALL let
agents run detached — closing a view detaches per ADR-0047 and never implies
stopping execution — and SHALL present completed work for review rather than
requiring the user to watch execution. Event-driven mode SHALL present
standing rules that cause agents to act and SHALL deliver proactive reports
to the same review surface as dispatched work.

#### Scenario: The user reviews instead of watching
- **WHEN** a user dispatches work and closes its view
- **THEN** the agent keeps running and the finished result appears in the
  review surface with its evidence
- **AND** no UI element required the user to keep a conversation or execution
  view open

#### Scenario: Event-driven outcomes share the review surface
- **WHEN** an agent acts because a standing rule fired
- **THEN** its report arrives in the same review surface as user-dispatched
  work
- **AND** consequential actions still pass through the same approval
  mechanism

### Requirement: Permission is the UX of mounting
Granting an agent access to Host files SHALL be presented as a permission
flow — drag-in, system file picker, or an approval sheet for an
agent-originated request — that creates a Host Mount Service grant per
ADR-0050. Mount and bind SHALL be side effects invisible to the default UI.
A single Permissions surface SHALL list active grants by label, scope, and
access, and revocation SHALL remove the projection. Raw Host paths SHALL NOT
appear in any layer; only grant labels and `/mnt` projections may be shown.

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

### Requirement: Workspace-first entry with shell as a tab type
The Alan for macOS entry SHALL be a workspace of agents, recent work, and
installed services — not a bare shell or terminal. Alan Shell SHALL be
available as one tab type among others, launched as an ordinary Process per
ADR-0048/0049. Services under `/srv` SHALL be presented as installed
services/apps that open their own file-backed interfaces. This UX ordering
governs renderer-presented defaults only: ADR-0039's system-level rule that
the `alan` CLI and Local Entry start Alan Shell before agent views is
unchanged, and this change carries the matching MODIFIED delta for the
macOS default-manifest presentation contract.

#### Scenario: The app opens
- **WHEN** a user launches Alan for macOS
- **THEN** the default presented view shows active agents, recent work, and
  installed services
- **AND** opening a shell is an explicit action that creates an ordinary
  shell Process
- **AND** the system-level Local Entry and Shell Process startup per
  ADR-0039/0049 proceeds independently of which view is presented first

#### Scenario: A service is opened from the workspace
- **WHEN** a user opens an installed service from the workspace
- **THEN** its interface renders from the service's mounted file tree
- **AND** the user never walks `/srv` paths to reach it

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
