## Context

P2 introduced host mount declarations that can project human/config-declared
host roots into both an Alan OS namespace and `SandboxSpec` at startup. P3 then
added the agent-facing `request_mount` approval flow, but approval currently
only records `host_mount_grant` and returns `approved_not_applied`.

The runtime tool path has a separate static projection: `ToolContext` derives a
fresh sandbox from the workspace root with `SandboxSpec::seed`, and
`Sandbox::is_in_workspace` checks only the first writable root. Even if a host
mount grant is approved, later `read_file`, `write_file`, `edit_file`, `list_dir`,
and `bash` calls still behave as though only the original workspace root exists.

## Goals / Non-Goals

**Goals:**

- Carry a runtime-owned `SandboxSpec` through `ToolExecutionBinding` and
  `ToolContext`.
- Treat every `SandboxSpec.writable_roots` entry as an allowed execution root
  for tool path checks and OS sandbox profile generation.
- Apply approved `request_mount` grants with `access = read_write` to the
  running runtime's tool sandbox for subsequent tool calls.
- Return a `request_mount` result that distinguishes sandbox projection from
  Alan OS namespace remounting.
- Keep duplicate approved roots idempotent and canonical where possible.

**Non-Goals:**

- Live mutation of the Alan OS `/mnt/<name>` namespace or `MountFs`.
- Adding explicit read-allow roots to `SandboxSpec`.
- Persisting approved grants across session restart.
- Linux mount namespace reification.

## Decisions

1. **Store the projected sandbox on the tool execution binding.**

   `ToolExecutionBinding` already owns workspace root, cwd, and scratch state for
   the runtime's tool execution context. Adding an optional `SandboxSpec` there
   keeps sandbox authority in the same runtime-owned binding instead of adding
   a parallel global registry.

2. **Preserve workspace root as the first writable root.**

   Existing callers and error messages assume the first root is the workspace
   seed. Approved writable mounts append additional roots. This keeps backward
   compatibility while allowing `SandboxSpec` to express the expanded authority.

3. **Update the active default binding on approval.**

   The runtime executes built-in tools through `state.tool_catalog`. When a
   mount grant is approved, the resume handler updates the catalog's default
   binding before the next turn resumes, so subsequent tool calls inherit the
   expanded sandbox projection without replaying the approved `request_mount`
   call.

4. **Project read-write grants only.**

   `SandboxSpec` currently models writable roots and a sensitive-read denylist;
   it does not model read-only roots. A read-only mount approval remains
   auditable but does not expand the tool sandbox until a read-allow projection
   exists.

5. **Keep `/mnt` namespace remounting separate.**

   Applying a `HostDirFs` to the live Alan OS namespace belongs in a host
   composition hook that can own `HostDirFs` construction and `MountFs`
   mutation. This change avoids adding a hidden hostfs dependency to the generic
   Agent Execution Engine.

## Risks / Trade-offs

- **Approved grant is usable by host-path tools but not `/mnt/<name>` yet** ->
  The tool result states `tool_sandbox_applied` separately from
  `namespace_applied`.
- **Read-only approval remains audit-only** -> Keep the result explicit and
  leave read-allow roots to a follow-up `SandboxSpec` extension.
- **Path checks may still report "workspace" in some messages** -> Expand the
  allow logic now, then polish wording where touched so failures name allowed
  roots rather than only the workspace root.
