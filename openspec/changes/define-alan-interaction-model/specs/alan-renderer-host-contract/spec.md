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
defined by `alan-interaction-model`, present the conversation,
background-servant, and event-driven modes, and keep OS vocabulary out of
default UI copy. Hosts MAY differ in entry emphasis — a terminal-native host
may center the shell — provided all three layers and modes remain reachable.

#### Scenario: A renderer host is reviewed for interaction-model conformance
- **WHEN** a renderer host's default UI is reviewed
- **THEN** all three disclosure layers are reachable, all three interaction
  modes are supported, and default copy passes the vocabulary rule
- **AND** any host-specific entry emphasis does not remove a layer or mode
