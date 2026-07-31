## ADDED Requirements

### Requirement: Renderer hosts render file surfaces as domain-native affordances
Renderer hosts SHALL translate agent and service file surfaces into
domain-native UI affordances — conversation views, plan cards, approval
sheets, result and evidence views, and lifecycle controls — instead of
presenting raw file listings or raw protocol content as the default
interface. User gestures on those affordances SHALL be expressed as file
writes and `ctl` commands, never as renderer-local state mutation. Stop
controls SHALL write `/proc/<pid>/ctl`. Renderer hosts SHALL NOT expose
lifecycle controls whose owning file surface the canonical AgentFS and Agent
Machine contracts have not defined.

#### Scenario: A user stops an agent from the Work layer
- **WHEN** a user activates a Stop control in a renderer host
- **THEN** the host writes the corresponding `/proc/<pid>/ctl` command
- **AND** no renderer-local execution state is mutated to simulate the stop

### Requirement: Renderer hosts implement the Alan Interaction Model layers
Renderer hosts SHALL provide the Intent, Work, and Files disclosure layers
defined by `alan-interaction-model`, present conversation, treat event-driven
interaction as the recorded dependent mode per `alan-interaction-model`, and
keep OS vocabulary out of default UI copy. A renderer attached through the
authorized attachment view over a Local Entry Shell Process namespace SHALL
also present background-servant mode through its mounted
`/mnt/agent-runtime/clone` capability. The underlying Shell Process namespace
SHALL NOT gain that mount. A Remote Entry renderer SHALL NOT be required to
present that mode until a separate remote-launch contract grants it an
appropriately revocable capability. Hosts MAY differ in entry emphasis — a
terminal-native host may center the shell — provided the layers and modes
required by its attachment remain reachable.

#### Scenario: A renderer host is reviewed for interaction-model conformance
- **WHEN** a renderer host's default UI is reviewed
- **THEN** all three disclosure layers are reachable, conversation is
  supported, and default copy passes the vocabulary rule
- **AND** background-servant mode is supported when the host is attached
  through the authorized Local Entry renderer attachment view
- **AND** any host-specific entry emphasis does not remove a layer or mode
- **AND** event-driven surfaces are required only once their owning runtime
  or service contract exists

## MODIFIED Requirements

### Requirement: A mounted namespace is sufficient for local renderer launch

A local renderer host SHALL start from a mounted Alan OS root. Launching an
agent view additionally requires a concrete Agent Process path. Launching a
`/srv` service view SHALL require only the mounted namespace and the
corresponding service path and SHALL NOT require or invent an Agent Process.

#### Scenario: Renderer opens a root Agent Process

- **WHEN** the renderer receives a namespace root and `/agent/root`
- **THEN** it reads and tails AgentFS output and state files
- **AND** it writes input and Process control through the corresponding files

#### Scenario: Renderer opens a service view

- **WHEN** the renderer opens an installed service
- **THEN** it renders the service interface from the service's mounted `/srv`
  file tree
- **AND** no Agent Process path is required
