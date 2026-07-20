# os-sandbox-enforcement Specification

## Purpose
Defines pluggable macOS and Linux Tool sandbox enforcement, safe degradation,
projected confinement inputs, sensitive-read controls, and reified namespace
behavior independent of command-text heuristics.
## Requirements
### Requirement: Pluggable OS sandbox backends
Tool execution SHALL be constrained by a selectable sandbox backend behind a common abstraction, with macOS and Linux backends and a degraded path-guard fallback.

#### Scenario: macOS selects the Seatbelt backend
- **WHEN** the agent runs on macOS with Seatbelt available
- **THEN** tool execution is confined by the Seatbelt backend

#### Scenario: Linux selects an OS backend when available
- **WHEN** the agent runs on Linux with the required kernel capabilities
- **THEN** tool execution is confined by the Linux backend (filesystem and network controls)

#### Scenario: Fallback to the path guard
- **WHEN** no OS sandbox backend is available
- **THEN** backend selection falls back to the namespace and Host Mount path guard

### Requirement: Kernel-enforced confinement independent of command syntax
When an OS sandbox backend is active, the operating system SHALL enforce
filesystem writes to the writable roots of the active `SandboxSpec` and control
network access regardless of how a command is written. Exact writable Host
Mount grants SHALL become writable roots; no ambient Host directory SHALL seed
the sandbox. Read-only Host Mounts SHALL NOT grant native-subprocess write
authority.

#### Scenario: Internal-write command is confined
- **WHEN** a command writes outside the active `SandboxSpec` writable roots
  through program-internal logic without an explicit path operand
- **THEN** the OS sandbox blocks the out-of-writable-roots write
- **AND** confinement does not depend on parsing the command string

#### Scenario: Network is controlled by the sandbox
- **WHEN** a confined command attempts network access that the active policy does
  not permit
- **THEN** the OS sandbox prevents it

#### Scenario: Declared writable host mount permits native writes there
- **WHEN** a host directory is declared with write access and projected into the
  active `SandboxSpec`
- **THEN** a native subprocess confined by the OS sandbox can write below that
  declared host path
- **AND** writes outside all writable roots remain blocked

#### Scenario: Declared read-only host mount does not permit native writes
- **WHEN** a host directory is declared read-only
- **THEN** that host path is not included in the active `SandboxSpec` writable
  roots
- **AND** a native subprocess is not granted write authority there by the
  sandbox projection

### Requirement: Safe degradation when no sandbox is available
When no enforcing sandbox backend is available, the system SHALL NOT auto-approve bash or network operations and SHALL escalate them instead. Sandbox-unavailable SHALL NOT be treated as sandbox-disabled-and-allow.

#### Scenario: No backend means escalate
- **WHEN** the host has no available OS sandbox backend
- **THEN** bash and network operations are escalated for human approval rather than auto-approved

#### Scenario: Degraded state is detectable
- **WHEN** the sandbox backend is unavailable or degraded
- **THEN** the active backend/degraded state is reported (e.g. via the decision audit or backend name)

### Requirement: Heuristic command parsing is not the enforcement mechanism
With an OS sandbox backend active, the bash command-string heuristic SHALL NOT be the line of defense for confinement and SHALL be reducible to advisory pre-flight or removed.

#### Scenario: Enforcement comes from the kernel
- **WHEN** a tool runs under an active OS sandbox backend
- **THEN** confinement correctness does not rely on the bash command-shape parser

### Requirement: Linux network confinement via seccomp
On Linux, the OS sandbox backend SHALL confine network access (in addition to Landlock filesystem confinement) so that filesystem and network are both contained, matching macOS Seatbelt. This removes the platform asymmetry so that network escalations can be reviewer-judged uniformly rather than always requiring a human on Linux.

#### Scenario: Linux confines network alongside the filesystem
- **WHEN** a tool runs under the active Linux sandbox backend
- **THEN** disallowed network access is prevented by the OS (via seccomp or equivalent), as filesystem writes are by Landlock

#### Scenario: Network escalation is reviewer-eligible on both platforms
- **WHEN** an operation requests network access and an OS backend that confines network is active
- **THEN** the escalation is eligible for reviewer judgment rather than being forced to a human due to a missing network backstop

#### Scenario: Missing network confinement falls back to the human
- **WHEN** no backend that confines network is available on the host
- **THEN** network operations are surfaced to the human rather than reviewer-judged or auto-allowed

### Requirement: Confinement input is a projected SandboxSpec
The Host adapter SHALL derive each Tool Process `SandboxSpec` from the explicit
service-owned Host Mount grants delegated to that launch, together with its
network policy and executable needs. The spec SHALL be attributable to that
Tool Process and SHALL contain the complete native inputs needed by the selected
OS sandbox backend. Agent Execution Engine, Alan Kernel, and composition roots
MUST NOT reconstruct native Host roots from Process namespace paths,
descriptors, grant IDs, or declaration lists.

#### Scenario: A Tool Process receives explicit-mount-only confinement
- **WHEN** a Tool Process is spawned with one writable Host Mount and no network authority
- **THEN** the selected OS sandbox backend receives a matching `SandboxSpec`
- **AND** the Host adapter maps only that delegated grant to native confinement

#### Scenario: Virtual namespace mounts grant no native authority
- **WHEN** a Tool Process receives only virtual Alan OS mounts and no delegated
  Host Mount grant
- **THEN** no composition root or backend infers a native Host root from those
  namespace paths

### Requirement: macOS sensitive-read denylist
Default sandbox specs SHALL include a sensitive-read denylist for common
home-directory secret, credential, keychain, and browser-profile locations.
When the active backend is macOS Seatbelt, those paths SHALL be projected into
the generated Seatbelt profile as read-deny rules. Backends that cannot express
broad reads with selected deny paths SHALL NOT claim sensitive-read denylist
enforcement.

#### Scenario: Default sandbox spec includes sensitive paths
- **WHEN** a sandbox spec is assembled for a Process on a host with a known user home directory
- **THEN** the spec includes read-deny entries for Alan Host Stores, common credential stores, macOS keychains, and browser profile directories

#### Scenario: Seatbelt profile denies sensitive reads
- **WHEN** a macOS Seatbelt profile is generated from a sandbox spec with read-deny entries
- **THEN** the profile contains `deny file-read*` rules for those read-deny entries

#### Scenario: Host adapter projection preserves read denies
- **WHEN** the Host adapter derives a sandbox spec from delegated read-write
  Host Mount grants
- **THEN** the resulting spec keeps the default sensitive-read denylist while
  adding only those grants' native writable roots

#### Scenario: Linux does not over-claim read-deny enforcement
- **WHEN** the Linux Landlock backend receives a sandbox spec with read-deny entries
- **THEN** write and network confinement remain active where supported
- **AND** sensitive-read denylist enforcement is not reported as provided by Landlock

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
- **THEN** the decision audit identifies whether the active Linux path is `linux_reified_namespace`, `landlock`, or `host_mount_path_guard`
- **AND** the audit distinguishes reified namespace paths from projected host paths
