## ADDED Requirements

### Requirement: The terminal CLI attaches to the existing Root Agent
A local terminal renderer SHALL receive a mounted Alan OS namespace and the
concrete `/agent/root` Agent Process path. It MUST NOT spawn, restore, or
supervise an Agent Process. AgentFS remains the authority for input, streamed
output, status, and Agent UI state.

#### Scenario: Bare Alan opens the terminal renderer
- **WHEN** bare `alan` runs with interactive stdin and stdout after attaching to
  the dedicated Host
- **THEN** it opens the file-backed renderer on `/agent/root`
- **AND** it does not create a second Agent or Shell Process

#### Scenario: A user submits a task
- **WHEN** the user submits text in the renderer
- **THEN** one complete framed input is written to
  `/agent/root/io/input` and new Agent output is observed from
  `/agent/root/io/output`
- **AND** the renderer does not privately call a provider or Tool

#### Scenario: No callable Connection is configured
- **WHEN** the Root Agent has no callable Connection and the user submits a task
- **THEN** the renderer displays a clear unavailable-Connection error
- **AND** the Agent does not report a model-control incompatibility or start a
  provider request
- **AND** the Host and Root Agent remain available

### Requirement: Root Agent interruption is turn-scoped
The renderer SHALL interrupt a running Root Agent turn through the Agent Runtime
control file `/agent/root/machine/ctl`. It MUST NOT send turn interruption to
`/proc/<pid>/ctl`, which owns Process lifecycle. The Root Agent SHALL remain
usable for subsequent input after a turn is interrupted.

#### Scenario: Ctrl-C interrupts a running turn
- **WHEN** the user presses Ctrl-C while a Root Agent turn is running
- **THEN** the renderer writes the Agent Runtime interrupt control
- **AND** the Root Agent Process remains running and accepts a subsequent task

#### Scenario: Renderer exits
- **WHEN** the user quits or closes the local renderer
- **THEN** it closes its own file streams and restores the terminal
- **AND** it does not stop the shared Alan OS Host or Root Agent Process
