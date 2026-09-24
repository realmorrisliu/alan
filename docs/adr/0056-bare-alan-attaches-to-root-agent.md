# Bare `alan` attaches to the existing Root Agent

Status: accepted, 2026-09-24. Supersedes ADR-0039 for bare CLI startup and
narrows ADR-0048: the bare terminal renderer is an attached Agent client, not an
Alan Shell Process. ADR-0048 still applies to an explicitly provided
interactive Alan Shell Process.

Running bare `alan` attaches to the channel's Alan OS Host. With terminal stdin
and stdout, it attaches the file-backed renderer to the Service Manager-owned
`/agent/root`. With redirected stdin, it submits one task to that same Agent
and writes the final result to stdout. If stdin is a terminal but stdout is
not, the CLI reports an error instead of waiting for terminal EOF.

The renderer is a non-owning client. It does not spawn, restore, or supervise
the Root Agent, and it does not create a second Shell Process. `/proc` remains
Process lifecycle truth; AgentFS remains the authority for Agent input, output,
status, and UI state. Agent-originated effects continue through Agent Runtime
governance and existing namespace, mount, credential, and sandbox boundaries.

This decision changes only the bare terminal entry path. It does not change
Alan Kernel, Process semantics, Service Manager ownership, Alan Shell builtins,
or the lifecycle of any explicitly provided Shell Process. Herdr continues to
own terminal topology under ADR-0054.

Normative behavior lives in the [`alan-shell` spec](../../openspec/specs/alan-shell/spec.md)
and [`alan-renderer-host-contract` spec](../../openspec/specs/alan-renderer-host-contract/spec.md).
The first implementation is recorded in [PR #929](https://github.com/realmorrisliu/alan/pull/929).
