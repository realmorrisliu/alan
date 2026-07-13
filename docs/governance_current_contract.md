# Current Governance Contract

This document describes implemented Agent Execution Engine governance.
Normative target requirements live in OpenSpec.

## Policy resolution

Policy resolves in this order:

1. an explicit governance policy path;
2. `policy.yaml` in the explicit Agent Definition, when present;
3. builtin profile defaults.

An authored policy file replaces the builtin rule set; it is not merged
implicitly.

## Per-call decisions

Each Tool call receives one policy decision: `allow`, `deny`, or `escalate`.
An escalation creates a pending confirmation request. Approval applies only to
that checkpoint; there is no Process-wide approval cache.

Unknown prior effect state uses its own replay-confirmation request so audit
records distinguish policy decisions from idempotency safety.

## Execution backend

The default `host_mount_path_guard` is best-effort path and command-shape
enforcement over explicit Host Mounts. It does not claim full network or
Process isolation. Optional Seatbelt or Landlock backends provide stronger host
enforcement when active.

Policy and execution backend are separate: policy decides whether an effect is
authorized, while the backend constrains how an authorized effect executes.

## Current matchers

Policy rules support:

- Tool name;
- capability;
- command pattern;
- normalized path prefix.

Relative paths are evaluated against current Tool cwd when available. Shell
payloads remain conservatively constrained because arbitrary programs can hide
effects behind their own argument or script semantics.

## Audit identity

Audit records use call, Process, Tool, turn, request, action, rollout, and
checkpoint identity owned by the execution path. AgentFS and rollout evidence
surface those decisions without introducing a second lifecycle authority.
