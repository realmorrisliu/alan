## ADDED Requirements

### Requirement: One mount declaration list projects into two enforcement mechanisms
Alan OS SHALL treat a single **mount declaration list**, assembled at session
startup, as the source of truth for what a hosted agent may touch. From that one
list the system SHALL derive two enforcement surfaces: (1) the **namespace**
(`alan-kernel`), which enforces `Access` in-process over aP file operations, and
(2) a **sandbox manifest** (`SandboxSpec`), which the OS sandbox
(Seatbelt/Landlock) enforces over native subprocesses (`bash`). Both surfaces
SHALL be projections written at the declaration site, never reconstructed by
introspecting one surface to rebuild the other. Only host-backed declarations
(those carrying a real `(host_path, access)`) SHALL contribute to the sandbox
manifest; purely virtual mounts SHALL contribute nothing, because a native
subprocess cannot see them.

#### Scenario: A host directory is declared once, projected twice
- **WHEN** a host directory is declared into the mount list with write access
- **THEN** it appears in the namespace as a writable aP mount
- **AND** its `host_path` appears in the sandbox manifest as a writable root
- **AND** the OS sandbox confining `bash` permits writes there and denies writes
  outside the manifest's writable roots

#### Scenario: A virtual mount does not reach the OS sandbox
- **WHEN** a virtual file server (e.g. `/agent`, `/mnt/llm`) is mounted
- **THEN** it is reachable by aP file tools through the namespace
- **AND** it produces no rule in the sandbox manifest, because a native
  subprocess has no host path for it to see

#### Scenario: The namespace does not replace the OS sandbox
- **WHEN** a native subprocess (`bash`) runs
- **THEN** its confinement comes from the OS sandbox derived from the manifest,
  not from the namespace, which it cannot see
- **AND** the two enforcement surfaces agree because both derive from one list

### Requirement: The projection preserves crate layering
Deriving the sandbox manifest from the mount list SHALL NOT couple the kernel to
host concerns or the agent engine to the kernel. `alan-kernel` SHALL remain
ignorant of host filesystem paths (it records only aP path → server + `Access`).
`alan-agent-engine` SHALL remain hosting-agnostic and SHALL NOT depend on
`alan-kernel`; its sandbox SHALL accept a `SandboxSpec` value rather than
reaching for a namespace. The projection wiring SHALL live only in the `alan`
composition root that assembles the session.

#### Scenario: Kernel stays host-agnostic
- **WHEN** the namespace is assembled
- **THEN** `alan-kernel` stores no `host_path`, only aP paths, servers, and access
- **AND** host provenance for a mount is recorded in the declaration list held by
  the composition root, not in the namespace

#### Scenario: Engine takes a spec, not a namespace
- **WHEN** the agent engine confines a subprocess
- **THEN** it consumes a `SandboxSpec { writable_roots, read_denylist, network }`
- **AND** it has no dependency on `alan-kernel`

### Requirement: Mounts are authorized outside the agent's control
Because a mount grants access to a host path, the agent SHALL NOT be able to
expand its own namespace at landing. Mounts SHALL be human/config-declared at
session assembly; the agent SHALL have no mount tool, and the manifest SHALL be
fixed for the session absent human action. The existing workspace SHALL be
modeled as the seed (first, default) host-backed entry of the manifest, not a
special case. Any future agent-requested mount SHALL be routed through the
`PolicyEngine` as an escalation and approved by a human/reviewer before taking
effect.

#### Scenario: The agent cannot mount
- **WHEN** an agent attempts to broaden its own access
- **THEN** no tool exists for it to add a host mount
- **AND** its reachable host paths remain exactly those declared by a human

#### Scenario: The workspace is the seed mount
- **WHEN** a session starts with only a workspace
- **THEN** the manifest contains one host-backed entry (the workspace, RW)
- **AND** behavior is identical to the prior single `workspace_root` confinement

#### Scenario: A future mount request escalates
- **WHEN** the agent-requestable mount feature exists and an agent requests a new
  host mount
- **THEN** the request surfaces as a `PolicyEngine` escalation (a `Yield` event)
- **AND** the mount takes effect only after human/reviewer approval
