## ADDED Requirements

### Requirement: Approved read-write mount grants update the runtime tool sandbox
The runtime SHALL add the normalized host path to the running tool sandbox's
writable roots for subsequent tool calls when a pending `request_mount`
confirmation is resumed with approval and the mount request has
`access = read_write`. The update SHALL be idempotent for duplicate roots.

#### Scenario: Approved read-write grant is applied to future tool calls
- **WHEN** a pending `request_mount` request for `/mnt/project` and host path `/host/project` is approved with `access = read_write`
- **THEN** the runtime adds `/host/project` to the active tool sandbox writable roots
- **AND** the `request_mount` tool result reports that tool sandbox projection was applied

#### Scenario: Duplicate approved grant is idempotent
- **WHEN** the same read-write host path is approved more than once
- **THEN** the active tool sandbox contains the host path only once

### Requirement: Non-writable mount grants do not expand writable roots
The runtime SHALL record approved non-writable mount grants but SHALL NOT add the
host path to `SandboxSpec.writable_roots` when a pending `request_mount`
confirmation is resumed with approval and the mount request has
`access = read_only`.

#### Scenario: Approved read-only grant remains audit-only for tool sandbox
- **WHEN** a pending `request_mount` request is approved with `access = read_only`
- **THEN** the runtime records the approved grant
- **AND** the `request_mount` tool result reports that tool sandbox projection was not applied
- **AND** the active tool sandbox writable roots are unchanged

### Requirement: Tool sandbox path checks honor all writable roots
Tool sandbox path checks SHALL treat every configured `SandboxSpec.writable_roots`
entry as an allowed execution root. OS sandbox profile generation SHALL continue
to receive the complete writable root list.

#### Scenario: Workspace-local tool can access an approved writable root
- **WHEN** the active tool sandbox writable roots contain the workspace root and an approved host root
- **THEN** a workspace-local tool path under the approved host root passes sandbox containment checks
- **AND** a path outside every writable root is still rejected

#### Scenario: Bash can run from an approved writable root
- **WHEN** the active tool sandbox writable roots contain the workspace root and an approved host root
- **THEN** bash execution with cwd under the approved host root passes sandbox cwd containment checks
- **AND** bash execution with cwd outside every writable root is rejected
