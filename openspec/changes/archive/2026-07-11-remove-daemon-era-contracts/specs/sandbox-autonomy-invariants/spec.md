## MODIFIED Requirements

### Requirement: OS-sandbox confinement is independent of command syntax

The syntactic shape parser SHALL be dropped only for a backend that
kernel-enforces protected-subpath writes (Seatbelt); such a backend enforces
deterministic filesystem confinement at the kernel rather than via the
workspace-path-guard command parser. A backend that confines the workspace but
cannot carve out protected subpaths (Landlock) SHALL keep the full shape parser
so opaque writers cannot hide a protected write the kernel will not deny.
Protected-subpath writes SHALL remain blocked on every backend.

#### Scenario: Wrappers run under a protected-write-enforcing sandbox
- **WHEN** a backend that kernel-denies protected-subpath writes (Seatbelt) is active and a command uses a shell wrapper or interpreter (`bash -lc …`, `python -c …`)
- **THEN** it is not rejected by the syntactic preflight or execution-path shape parser; the kernel sandbox confines it (protected subpaths included)

#### Scenario: Opaque writers stay rejected under Landlock
- **WHEN** Landlock is active (it cannot carve a protected subdir out of the writable workspace) and a command is an opaque writer the path check cannot inspect (`python -c 'open(".git/config","w")…'`, `python scripts/setup.py`)
- **THEN** it is rejected by the shape parser, the same posture as the path-guard fallback, because the kernel cannot deny the protected write

#### Scenario: Direct/nested protected-subpath tampering is blocked
- **WHEN** a command writes to a protected subpath (`.git`, `.alan`, `.agents`) via an explicit path operand, directly or hidden inside a shell-wrapper inline script (`bash -lc 'echo x > .git/config'`)
- **THEN** the write is blocked by the path-guard parser, which checks direct operands and recurses into shell-wrapper inline scripts
- **AND** program-internal writes by purpose-built owners remain possible, including git porcelain writing `.git` and Agent memory workflows writing the active channel-scoped Memory Store

#### Scenario: Out-of-workspace reads stay contained under an OS sandbox
- **WHEN** an auto-approved read-classified bash command references a path outside the workspace (`cat ~/.ssh/id_rsa`, `cat /etc/passwd`), under any backend including a wrapper form
- **THEN** it is rejected by the path-guard parser's containment check — the OS sandbox confines writes and network but permits reads, so dropping the shape parser must NOT drop path containment; secrets cannot be read into tool output without approval

#### Scenario: Channel-scoped Memory Store carve-outs are preserved under recursion
- **WHEN** an OS-sandboxed command writes beneath `.alan/runtime/stable/memory/` or `.alan/runtime/dev/memory/`, directly or inside a wrapper
- **THEN** the recursive protected check allows the write through the same narrow carve-out as the direct path check
- **AND** unscoped `.alan/memory/`, unknown runtime channels, rollout, cache, shell-restore, policy, and other protected state remain blocked

#### Scenario: Approved network intent is preserved
- **WHEN** a command classified as a network capability is approved and executed
- **THEN** it runs with the sandbox network restriction lifted (still filesystem-confined) so the approved network call is not futile

## REMOVED Requirements

### Requirement: The client never silently drops events across a reconnect

**Reason**: The requirement defines event delivery through a client reconnect and daemon replay-buffer contract.

**Migration**: Preserve ordered delivery through offset-readable AgentFS streams and explicit retention-gap handling.

## ADDED Requirements

### Requirement: Renderer file streams preserve offsets across reattachment

A renderer reading an offset-addressable AgentFS stream SHALL retain its last delivered offset and SHALL NOT silently omit data when reopening the file or reattaching to the Agent Process. Overlap SHALL be deduplicated by stable file offset or record identity, and an unrecoverable retention gap SHALL be surfaced.

#### Scenario: Records written during reattachment are read

- **WHEN** a renderer's file watch ends and records are appended before it opens the stream again
- **THEN** the renderer resumes from its last delivered offset
- **AND** it delivers retained records in order before following new appends

#### Scenario: Snapshot and stream overlap is deduplicated

- **WHEN** hydrated snapshot state and an offset-readable stream contain the same durable record
- **THEN** the renderer presents the record once using its stable identity or offset

#### Scenario: Retention gap is surfaced

- **WHEN** the requested offset is older than retained stream data
- **THEN** the renderer reports a recoverable gap instead of pretending the stream is continuous
- **AND** recovery proceeds through current AgentFS snapshot and file semantics
