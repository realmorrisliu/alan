## ADDED Requirements

### Requirement: Interactive Alan Shell is an ordinary Process
Each interactive Alan Shell SHALL run as a Shell Process with PID, parent,
Alan OS credentials, namespace, descriptors, and namespace cwd. Renderer hosts
SHALL attach input/output and MUST NOT invoke Kernel spawning as an out-of-band
execution manager.

#### Scenario: Shell spawns an Agent
- **WHEN** the user executes an Agent Executable with a definition descriptor
- **THEN** `/proc` records the Agent Process as a child of the Shell Process
