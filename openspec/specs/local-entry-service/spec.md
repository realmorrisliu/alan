# local-entry-service Specification

## Purpose
Defines Local Entry Service as the bounded creator and handoff owner for local
Alan Shell Processes.

## Requirements

### Requirement: Local entry creates a Shell Process
Local Entry Service SHALL create `/bin/alan-shell` as an ordinary Process with
Alan OS credentials, Login Namespace Template, descriptors, cwd, PID, and
parentage, then hand its namespace to an authorized local renderer.

#### Scenario: macOS requests local entry
- **WHEN** Host transport has authorized the peer
- **THEN** Local Entry Service creates a Shell Process
- **AND** commands launched by the Shell become child Processes

### Requirement: Entry state is not a Session
Local Entry Service SHALL retain only bounded Process-creation/handoff state. It
MUST NOT own Agent Processes, conversations, workspaces, or renderer continuity.

#### Scenario: Local socket disconnects
- **WHEN** the renderer loses its connection
- **THEN** its Shell Process may drain and exit
- **AND** independent Agent Processes continue according to `/proc`
