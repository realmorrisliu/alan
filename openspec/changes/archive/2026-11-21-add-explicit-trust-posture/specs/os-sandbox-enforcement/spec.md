# Spec Delta

## MODIFIED Requirements

### Requirement: Pluggable OS sandbox backends
This requirement applies to confined governed execution. Fully trusted with confined selection SHALL retain available enforcement and fail effects that cannot be contained without per-action escalation. Only a separate explicit trusted Host-account selection SHALL disable Alan native sandbox enforcement; missing backend capability alone MUST NOT select that mode.

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

### Requirement: Safe degradation when no sandbox is available
This requirement applies to confined governed execution. Fully trusted with confined selection SHALL retain available enforcement and fail effects that cannot be contained without per-action escalation. Only a separate explicit trusted Host-account selection SHALL disable Alan native sandbox enforcement; missing backend capability alone MUST NOT select that mode.

When no enforcing sandbox backend is available, the system SHALL NOT auto-approve bash or network operations and SHALL escalate them instead. Sandbox-unavailable SHALL NOT be treated as sandbox-disabled-and-allow.

#### Scenario: No backend means escalate
- **WHEN** the host has no available OS sandbox backend
- **THEN** bash and network operations are escalated for human approval rather than auto-approved

#### Scenario: Degraded state is detectable
- **WHEN** the sandbox backend is unavailable or degraded
- **THEN** the active backend/degraded state is reported (e.g. via the decision audit or backend name)

### Requirement: Linux network confinement via seccomp
This requirement applies to confined governed execution. Fully trusted with confined selection SHALL retain available enforcement and fail effects that cannot be contained without per-action escalation. Only a separate explicit trusted Host-account selection SHALL disable Alan native sandbox enforcement; missing backend capability alone MUST NOT select that mode.

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

### Requirement: Linux reification degrades safely
This requirement applies to confined governed execution. Fully trusted with confined selection SHALL retain available enforcement and fail effects that cannot be contained without per-action escalation. Only a separate explicit trusted Host-account selection SHALL disable Alan native sandbox enforcement; missing backend capability alone MUST NOT select that mode.

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

### Requirement: Confinement input is a projected SandboxSpec
This requirement applies to confined Tool execution. Explicit fully trusted Host-account native execution SHALL use its separately selected account authority without fabricating a projected SandboxSpec or extra service grant. aP handle rights and retained grant provenance remain unchanged.
The Host adapter SHALL derive each Tool Process `SandboxSpec` from the explicit
service-owned Host Mount grants delegated to that launch, together with its
network policy and executable needs. The spec SHALL be attributable to that
Tool Process and SHALL contain the complete native inputs needed by the selected
OS sandbox backend. Agent Execution Engine, alan9 Kernel, and composition roots
MUST NOT reconstruct native Host roots from Process namespace paths,
descriptors, grant IDs, or declaration lists.

#### Scenario: A Tool Process receives explicit-mount-only confinement
- **WHEN** a Tool Process is spawned with one writable Host Mount and no network authority
- **THEN** the selected OS sandbox backend receives a matching `SandboxSpec`
- **AND** the Host adapter maps only that delegated grant to native confinement

#### Scenario: Virtual namespace mounts grant no native authority
- **WHEN** a Tool Process receives only virtual alan9 mounts and no delegated
  Host Mount grant
- **THEN** no composition root or backend infers a native Host root from those
  namespace paths

### Requirement: Development dependencies retain explicit mount authority
This requirement applies to confined Tool execution. Explicit fully trusted Host-account native execution SHALL use its separately selected account authority without fabricating a projected SandboxSpec or extra service grant. aP handle rights and retained grant provenance remain unchanged.
Local dependencies outside a delegated project SHALL require their own live
Host Mount authority. Runtime projection MUST NOT infer that authority from a
manifest, PATH, prior successful build or cached output. Revoked, missing or
escaping dependencies SHALL fail without unauthorized reads/writes or replay;
network dependency retrieval remains subject to existing network approval.

#### Scenario: A project uses an explicitly mounted local dependency
- **WHEN** its dependency path resolves inside a separately delegated read-only mount
- **THEN** the supported build reads that dependency with its granted access
- **AND** the dependency cannot be modified by the build

#### Scenario: Dependency authorization is absent or revoked
- **WHEN** a dependency is outside live delegated authority or its mount is revoked
- **THEN** the next build cannot access the dependency through an ambient path or stale grant
- **AND** retained output is not reported as a new successful execution

#### Scenario: A dependency symlink escapes its mount
- **WHEN** a declared local dependency resolves beyond its authorized backing
- **THEN** execution refuses or the sandbox denies the escaped access
- **AND** no extra Host root is inferred from the manifest or symlink
