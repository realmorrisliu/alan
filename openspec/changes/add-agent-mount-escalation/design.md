## Context

P2 added `HostDirFs` and an `alan` composition-root helper that projects human
declared host mounts into both the namespace and `SandboxSpec`. D5 in
`define-namespace-driven-sandbox` keeps the landing model intentionally static:
the agent has no mount tool, because a mount grants authority over a host path.

The current runtime can already pause for confirmation through
`request_confirmation` and policy-driven tool escalation. `MountFs`, however,
wraps an already assembled namespace as a static file server; the runtime does
not yet have a composition-root mutator that can add a `HostDirFs` mount and
re-derive the `SandboxSpec` while a session is running.

## Goals / Non-Goals

**Goals:**

- Add a built-in `request_mount` virtual tool with a narrow host-directory mount
  request schema.
- Validate mount requests before presenting them for approval.
- Ensure mount requests are never auto-allowed, even if the active policy would
  otherwise allow write-like operations.
- Reuse the existing confirmation/Yield mechanism so approvals flow through the
  same client and recorder surfaces as other runtime approvals.
- Record approved mount grants as structured audit events that a later
  reconfiguration slice can consume.

**Non-Goals:**

- Live mutation of `MountFs` or the running process namespace.
- Re-deriving the active `SandboxSpec` after approval.
- Persisting mount grants as durable user configuration.
- Linux mount namespace reification.

## Decisions

1. **Expose `request_mount` as a runtime virtual tool.**

   This matches existing agent-facing request tools (`request_confirmation`,
   `request_user_input`) and keeps the request in the Agent Process tool loop.
   The tool does not grant access directly; it only asks the host to authorize a
   host-path grant.

2. **Use confirmation/Yield rather than adding a new protocol operation.**

   A mount request is an approval, not a new transport primitive. `Op::Resume`
   already carries approve/reject choices and works across TUI/daemon clients.
   The mount-specific shape lives in the confirmation payload details and the
   final tool result.

3. **Force escalation after policy evaluation.**

   Policy may deny a mount request, and policy metadata should appear in the
   audit trail. But an `Allow` result is upgraded to `Escalate` because a mount
   is an authority grant and must be authorized outside agent control. This
   preserves the D5 security boundary even under permissive policy files.

4. **Record approved grants, do not apply them yet.**

   On approval, the runtime records a `host_mount_grant` event and returns a
   structured `request_mount` tool result. The grant includes normalized
   `namespace_path`, `host_path`, `access`, `reason`, `checkpoint_id`, and
   `approved` status. A follow-up change can add the composition-root mutator
   that consumes these grants and rebuilds namespace/sandbox projections.

## Risks / Trade-offs

- **Approved grants are not live mounts yet** -> The tool result and spec state
  this explicitly; the next slice owns live namespace/sandbox reconfiguration.
- **Permissive policy might otherwise authorize mounts** -> The handler upgrades
  `Allow` to `Escalate` for this tool.
- **A malformed request could target reserved Alan OS roots** -> Validation
  accepts only absolute namespace paths under `/mnt/<name>` and rejects relative
  components, `/mnt` itself, known service-owned `/mnt` roots (`/mnt/llm`,
  `/mnt/mem`, `/mnt/route`), host filesystem roots including Windows drive/UNC
  roots, and non-absolute host paths.
