## MODIFIED Requirements

### Requirement: Local entry creates a Shell Process
Local Entry Service SHALL create `/bin/alan-shell` as an ordinary Process with
Alan OS credentials, Login Namespace Template, descriptors, cwd, PID, and
parentage. Its Process namespace SHALL contain an ordinary quota-scoped
`/agent` handle and SHALL omit `/mnt/agent-runtime`. The Host SHALL hand an
authorized local renderer a distinct attachment view over that Shell Process
namespace. The attachment view SHALL overlay `/agent` with a handle backed by
the renderer-reserved history capacity and SHALL add the Agent Runtime Service
top-level launch capability at `/mnt/agent-runtime`. The overlay MUST NOT alter
the Process namespace described by `/proc/self/namespace`. Service Manager
MUST NOT publish the launch capability through `/srv` or include it in any
child Process namespace template. Ordinary Process handles SHALL NOT consume
the renderer reserve.

#### Scenario: macOS requests local entry
- **WHEN** Host transport has authorized the peer
- **THEN** Local Entry Service creates a Shell Process
- **AND** commands launched by the Shell become child Processes
- **AND** the renderer attachment view can open
  `/mnt/agent-runtime/clone`
- **AND** its overlaid `/agent` history handle retains reserved capacity when
  an ordinary Process exhausts the ordinary pool

#### Scenario: Shell launches an ordinary child Process
- **WHEN** the Shell constructs a child namespace from
  `/proc/self/namespace`
- **THEN** that manifest contains the Shell Process's ordinary `/agent` handle
- **AND** it omits `/mnt/agent-runtime`
- **AND** the child cannot inherit either the renderer-reserved history
  account or top-level Agent launch authority

#### Scenario: Agent Process namespace is assembled
- **WHEN** Agent Runtime Service creates an Agent Process
- **THEN** `/mnt/agent-runtime` is absent
- **AND** read-write access to `/agent` does not imply top-level launch
  authority
