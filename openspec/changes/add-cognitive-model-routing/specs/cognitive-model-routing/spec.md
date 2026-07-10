## ADDED Requirements

### Requirement: Cognitive roles resolve to mounted LLM Connections
Alan SHALL configure System 1 and System 2 as callable llmfs Connections bound
under stable cognitive-role aliases in the coordinating Agent Process namespace.
Provider, model, and Credential material SHALL remain owned by the Connection;
cognitive routing SHALL NOT dispatch through a provider SDK or global opaque id.

#### Scenario: Both cognitive roles are available
- **WHEN** the spawner resolves valid System 1 and System 2 connection profiles
- **THEN** it binds each callable Connection under the corresponding role alias
  in the coordinator namespace
- **AND** the aliases expose no plaintext credential material

#### Scenario: A cognitive role is unavailable
- **WHEN** a configured Connection is missing, unauthorized, or not mounted
- **THEN** Alan reports that route unavailable before spawning an attempt
- **AND** it does not bypass the namespace through profile metadata

### Requirement: Routed attempts are ordinary Processes
Each System 1 or System 2 attempt SHALL execute as an ordinary Process visible in
`/proc` and, when agent-conforming, the `/agent` overlay. Attempts SHALL expose
normal IO, status, events, and parentage rather than existing only as hidden
runtime phases.

#### Scenario: A System 1 attempt starts
- **WHEN** the coordinator chooses the fast route
- **THEN** it spawns a child Agent Process with the System 1 Connection and
  bounded task descriptors
- **AND** the attempt is inspectable through process and agent files

#### Scenario: A System 2 attempt follows escalation
- **WHEN** the coordinator accepts a System 1 escalation suggestion
- **THEN** it spawns a sequential System 2 attempt with its own namespace and
  Connection
- **AND** both attempts remain linked by process/action provenance

### Requirement: Speculative System 1 has a restricted namespace
A speculative System 1 attempt SHALL receive read-only context mounts and a
`/bin` union that omits side-effecting Tools. It SHALL NOT gain a withheld mount
or executable through `/srv`, opaque ids, retained parent descriptors, or an
in-process Tool registry.

#### Scenario: System 1 inspects context
- **WHEN** a System 1 attempt needs repository or app context
- **THEN** it may read only the explicitly mounted read-only trees and run only
  the read-only Tools present in its `/bin`

#### Scenario: System 1 proposes a mutation
- **WHEN** System 1 output suggests a state-changing action
- **THEN** the suggestion is returned as data for coordinator review or deeper
  routing
- **AND** no side effect executes from the speculative namespace

### Requirement: Routing precedence is deterministic and observable
The coordinator SHALL resolve routing in this order: explicit System 2 next
intent, deterministic System 2 gates, eligible explicit System 1 next intent,
configured default, then System 1 fallback. Every forced, refused, automatic, or
explicit decision SHALL append a bounded record to routing events.

#### Scenario: Explicit System 1 conflicts with a gate
- **WHEN** `next system-1` is pending but a deterministic gate requires System 2
- **THEN** the coordinator selects System 2
- **AND** routing status/events identify the refused intent and gate reason

#### Scenario: No override or gate applies
- **WHEN** no explicit next intent or deterministic gate applies
- **THEN** the configured default role is selected, falling back to System 1

### Requirement: Explicit routing intent uses the owning machine ctl
The coordinating Agent Process SHALL accept `auto` and next-attempt cognitive
role intent as `route` commands on the agent-runtime-owned `machine/ctl` (for
example `route next system-2`, `route auto`). `machine/routing/` SHALL carry no
`ctl` file: per `agent-file-layout-contract`, the agent overlay's only control
surfaces are the kernel `/proc/<pid>/ctl` and the runtime `machine/ctl`, and new
control actions are added as new commands on the owning `ctl`, not as new files.
A next-attempt intent SHALL be consumed by one logical input and SHALL NOT
create an independent session or daemon override authority.

#### Scenario: User requests the deep route
- **WHEN** an authorized client writes `route next system-2` to `machine/ctl`
- **THEN** the next logical input uses System 2 unless the command is invalidated
  before consumption
- **AND** routing status/events record the command and consumption

#### Scenario: A routing ctl file is proposed
- **WHEN** an implementation change proposes `machine/routing/ctl` or another
  routing-specific control file
- **THEN** the change is rejected in favor of new `route` commands on
  `machine/ctl`, unless it also modifies `agent-file-layout-contract` explicitly

### Requirement: System 1 escalation is typed stream content
Alan SHALL allow System 1 to emit a provider-neutral `route/escalate` record
with a bounded reason and needed-context labels. The record SHALL be treated as
a suggestion read from the attempt stream, not as a Tool or capability, and the
coordinator SHALL record its decision before spawning System 2.

#### Scenario: System 1 requests escalation
- **WHEN** the System 1 events/output stream contains a valid escalation record
- **THEN** the coordinator suppresses the speculative draft as the accepted
  result and evaluates the deeper route
- **AND** no `escalate_to_system2` Tool or virtual action is required

### Requirement: Routing state is projected under machine routing
AgentFS SHALL expose routing `config`, `status`, `current`, `result`, and
`events` under `machine/routing/`. Snapshot files and the offset-resumable events
stream SHALL be the canonical client observability surface; they are read-only
state and carry no control file.

#### Scenario: A renderer attaches mid-attempt
- **WHEN** a renderer opens the coordinating agent after an attempt has started
- **THEN** it reads routing snapshots for the current role, attempt pid,
  Connection alias, status, and bounded reason
- **AND** it resumes ordered updates by reading routing events from its offset

### Requirement: Accepted output identifies its attempt provenance
The coordinator SHALL publish one accepted logical result while retaining the
accepted attempt pid, cognitive role, Connection alias, reasoning controls,
routing reason, and prior-attempt references in routing result and tape/action
records. It SHALL NOT expose hidden reasoning content.

#### Scenario: System 2 answer is accepted
- **WHEN** a System 1 escalation is followed by a successful System 2 attempt
- **THEN** the parent output uses the System 2 result
- **AND** routing files identify both attempts and why the second was selected

### Requirement: Provider continuation is role and namespace compatible
Provider-native continuation SHALL be reused only when Connection identity,
model, credential scope, cognitive role, prompt fingerprint, visible Tool
manifest fingerprint, and relevant request controls are compatible. A role
change SHALL default to a fresh Generation with accepted context reprojected.

#### Scenario: Routing switches from System 1 to System 2
- **WHEN** the selected cognitive role or Connection changes
- **THEN** Alan does not pass System 1 provider-native continuation into System 2
- **AND** the deeper Generation receives only accepted, provider-neutral context
