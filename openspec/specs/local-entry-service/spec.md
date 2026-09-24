# local-entry-service Specification

## Purpose
Defines Local Entry Service as the bounded creator and handoff owner for local
Alan Shell Processes.

## Requirements

### Requirement: Local entry creates a Shell Process
Local Entry Service SHALL create `/bin/alan-shell` as an ordinary Process with
alan9 credentials, Login Namespace Template, descriptors, cwd, PID, and
parentage, then hand its namespace to an authorized local renderer.

#### Scenario: A local client explicitly requests a Shell Process
- **WHEN** Host transport has authorized the peer and the client requests a Shell Process
- **THEN** Local Entry Service creates a Shell Process
- **AND** commands launched by the Shell become child Processes

### Requirement: Entry state is not a Session
Local Entry Service SHALL retain only bounded Process-creation/handoff state. It
MUST NOT own Agent Processes, conversations, workspaces, or renderer continuity.

#### Scenario: Local socket disconnects
- **WHEN** the renderer loses its connection
- **THEN** its Shell Process may drain and exit
- **AND** independent Agent Processes continue according to `/proc`
