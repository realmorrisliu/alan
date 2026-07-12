# live-mount-grant-namespace-projection Specification

## Purpose
Defines how approved mount grants update a running Agent Process namespace,
preserve host/Kernel/engine layering, remain idempotent, and record application
outcomes independently.
## Requirements
### Requirement: Approved mount grants update the live Agent Process namespace
The runtime SHALL apply an approved `request_mount` grant to the running Agent
Process Alan OS namespace when a pending mount confirmation is resumed with
approval and a namespace mount applicator is available. The mounted path SHALL
be the requested absolute `/mnt/<name>` namespace path, and future aP file-tool
walks SHALL resolve through that namespace path. The same live namespace handle
SHALL be the process namespace source of truth for file walks,
`/proc/<pid>/namespace` descriptions, and namespace snapshots used for later
child process spawns.

#### Scenario: Approved read-write grant is mounted into the namespace
- **WHEN** a pending `request_mount` request for `/mnt/project`, host path `/host/project`, and `access = read_write` is approved
- **THEN** future aP file-tool walks under `/mnt/project` resolve to the approved host directory
- **AND** aP mutating operations under `/mnt/project` are permitted by the namespace mount access
- **AND** the `request_mount` tool result reports `namespace_applied = true`

#### Scenario: Approved read-only grant is mounted into the namespace
- **WHEN** a pending `request_mount` request for `/mnt/docs`, host path `/host/docs`, and `access = read_only` is approved
- **THEN** future aP file-tool walks and reads under `/mnt/docs` resolve to the approved host directory
- **AND** aP mutating operations under `/mnt/docs` are rejected by the namespace mount access
- **AND** the `request_mount` tool result reports `namespace_applied = true`
- **AND** the active tool sandbox writable roots are unchanged

#### Scenario: Proc namespace views and child spawns observe live grants
- **GIVEN** standard runtime assembly created `MountFs` and `ProcFs` before a mount grant was approved
- **WHEN** a pending `request_mount` request for `/mnt/project` is approved and applied to the live namespace
- **THEN** future `/proc/<pid>/namespace` reads include the `/mnt/project` mount
- **AND** future child processes spawned through `/proc/clone` inherit a namespace snapshot that includes `/mnt/project`
- **AND** the runtime does not report `namespace_applied = true` from a `MountFs`-only mutation while process namespace state remains stale

#### Scenario: Live namespace mutation invalidates namespace metadata
- **GIVEN** a client has observed qids or versions for mount-table-derived namespace metadata such as `/mnt` listings and `/proc/<pid>/namespace`
- **WHEN** an approved grant mutates the live namespace
- **THEN** the live namespace generation changes
- **AND** qids or versions for affected `MountFs` synthetic listings change
- **AND** qids or versions for affected `ProcFs` namespace descriptions change
- **AND** cache-by-qid clients can detect that namespace metadata should be reread

### Requirement: Namespace projection preserves host/kernel/engine layering
The runtime SHALL apply approved live namespace grants through a host-provided
mount applicator. `alan-agent-engine` SHALL NOT construct `HostDirFs` or depend
on `alan_hostfs`, and Alan Kernel SHALL NOT store host path provenance. Alan
Kernel live mutation SHALL accept only a namespace path, an `InProcessTransport`,
and `Access`.

#### Scenario: Host composition owns HostDirFs construction
- **WHEN** an approved mount grant is applied live
- **THEN** the host composition layer constructs the host-backed file server from the host path
- **AND** the engine observes only the application outcome
- **AND** Alan Kernel records only the mounted file server and access mode

#### Scenario: Missing applicator does not pretend success
- **WHEN** a pending mount request is approved in a runtime without a namespace mount applicator
- **THEN** the approval is still recorded
- **AND** the `request_mount` tool result reports `namespace_applied = false`
- **AND** the result includes a concise reason that live namespace application is unavailable

### Requirement: Namespace projection is idempotent by namespace path
The live namespace applicator SHALL replace the exact requested namespace mount
path for future walks instead of accumulating duplicate mounts at the same path.
Rejected mount requests SHALL NOT change the live namespace.

#### Scenario: Duplicate approved grant replaces the same namespace path
- **WHEN** the same namespace path is approved more than once
- **THEN** future walks under that namespace path resolve through one latest mounted backing tree
- **AND** namespace descriptions do not accumulate duplicate exact-path entries for the repeated grant

#### Scenario: Rejected grant leaves the namespace unchanged
- **WHEN** a pending mount request is resumed with a reject choice
- **THEN** the requested namespace path is not mounted
- **AND** the `request_mount` tool result reports `namespace_applied = false`

### Requirement: Namespace application outcome is audited independently
The `request_mount` tool result and `host_mount_grant` audit event SHALL report
namespace projection independently from tool sandbox projection. If namespace
application fails after approval, the runtime SHALL record the approval and SHALL
report `namespace_applied = false` with a concise `namespace_error`.

#### Scenario: Namespace apply failure is reported without hiding approval
- **WHEN** a pending mount request is approved but the host applicator cannot construct or mount the backing file server
- **THEN** the runtime records the approved `host_mount_grant` event
- **AND** the tool result reports `namespace_applied = false`
- **AND** the tool result includes `namespace_error`
- **AND** the tool result does not claim Linux reification or native subprocess visibility at `/mnt/<name>`

#### Scenario: Tool sandbox and namespace projection can differ
- **WHEN** an approved read-only grant is mounted into the namespace
- **THEN** the tool result reports `namespace_applied = true`
- **AND** the tool result reports `tool_sandbox_applied = false`
