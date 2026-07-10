## ADDED Requirements

### Requirement: Linux reified namespace backend provides full read isolation
The Linux reified namespace backend SHALL run native subprocesses inside a
reified filesystem view derived from host-backed Alan OS mount declarations when
the host has the required namespace capabilities. In this mode, undeclared host
paths SHALL be absent from the subprocess view by default, providing full read
isolation through filesystem reification rather than command parsing or a
sensitive-read denylist.

#### Scenario: Reified backend exposes declared host mounts at namespace paths
- **WHEN** a Linux runtime starts a native subprocess with a declared host-backed mount `/mnt/project`
- **THEN** the subprocess sees the mounted content at `/mnt/project`
- **AND** the subprocess does not need to use the original host path to access that mount

#### Scenario: Undeclared host paths are absent
- **WHEN** a command running under the reified backend attempts to read an undeclared host path such as the user's home secret directory
- **THEN** that path is absent or unreachable from the subprocess filesystem view
- **AND** read isolation does not depend on the command-shape parser

#### Scenario: Virtual Alan OS mounts are not exposed as native paths
- **WHEN** the Alan OS namespace contains virtual mounts such as `/agent`, `/srv`, `/proc`, or `/mnt/llm`
- **THEN** the reified native subprocess view does not expose those mounts as host filesystem paths
- **AND** only host-backed declarations contribute native bind mounts

### Requirement: Reification preserves mount access and execution substrate boundaries
The reified Linux backend SHALL distinguish declared host mounts from execution
substrate. Declared read-write host mounts SHALL be writable where mounted;
declared read-only host mounts SHALL be readable but not writable; execution
substrate needed to launch commands SHALL be mounted read-only and SHALL NOT
grant access to user data outside declared mounts.

#### Scenario: Read-write host mount permits mutation at the reified path
- **WHEN** a declared host mount has read-write access
- **THEN** a native subprocess running under the reified backend can write under the corresponding reified namespace path
- **AND** writes outside declared writable mounts are rejected

#### Scenario: Read-only host mount rejects mutation at the reified path
- **WHEN** a declared host mount has read-only access
- **THEN** a native subprocess running under the reified backend can read under the corresponding reified namespace path
- **AND** write attempts under that path are rejected

#### Scenario: Execution substrate does not expose user data
- **WHEN** the reified backend mounts system paths needed to execute `/bin/sh` and common tools
- **THEN** those system paths are mounted only as execution substrate
- **AND** user home directories and secret stores are not exposed unless explicitly declared

### Requirement: Linux reification degrades safely
The Linux reified namespace backend SHALL be selected only when the host can
create the required user namespace, mount namespace, bind mounts, read-only
mounts, and network confinement. If any required capability is unavailable, the
runtime SHALL fall back to the existing Linux projection backend or path guard
according to current safe-degradation rules and SHALL report why reification is
unavailable.

#### Scenario: Missing namespace capability falls back
- **WHEN** the host cannot create an unprivileged user or mount namespace
- **THEN** the Linux reified backend is not selected
- **AND** backend reporting includes the missing capability reason
- **AND** the runtime continues with Landlock or path-guard fallback behavior

#### Scenario: Missing network confinement is degraded
- **WHEN** the filesystem view can be reified but network confinement is unavailable for a network-denied command
- **THEN** the backend reports degraded network confinement
- **AND** policy routes network-capable operations to a human, denies them, or
  falls back to a backend with network confinement
- **AND** the autonomous reviewer cannot approve network-capable execution
  without an OS network-confinement backstop

#### Scenario: Backend audit names the active path
- **WHEN** a native subprocess is evaluated or executed
- **THEN** the decision audit identifies whether the active Linux path is `linux_reified_namespace`, `landlock`, or `workspace_path_guard`
- **AND** the audit distinguishes reified namespace paths from projected host paths
