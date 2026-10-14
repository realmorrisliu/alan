# Bare `alan` owns a foreground instance and renders its Root Agent

Status: accepted 2026-09-27; foreground lifetime and explicit recovery shipped.
Current delivery note updated 2026-10-14; broader qualification is not implied.

The user selected one independent foreground Alan invocation per Herdr
terminal session, with recovery only after explicit user selection. Ordinary
terminals follow the same model. This supersedes the channel-wide background
Host premise in [ADR-0044](0044-one-system-level-alan-os-instance-per-channel.md)
and the earlier bare-client behavior recorded below.

Each bare `alan` invocation boots and owns its foreground alan9 instance,
including the instance-local `/agent/root`, Process table, services, endpoint,
and shutdown. The Service Manager remains the owner of services and Agent
Process lifecycle inside that instance; it does not require a separate
always-running Host. When `ALAN_INSTANCE_RUNTIME_DIR` is unset, the CLI chooses
a unique temporary runtime directory. When set, it selects that exact endpoint
and only one invocation can own it. The product uses one persistent System Store / Host Store pair, not a singleton
live runtime or separate installation channels.

With terminal stdin and stdout, the CLI renders its own `/agent/root`. With
redirected stdin, it submits one task to that instance and writes the result to
stdout. Actual Alan process exit shuts down that instance. A terminal host may
retain the native process when only its view detaches; Alan does not start a
background replacement. A new invocation starts fresh and does not implicitly
join or resume another invocation's work. Explicit `alan --resume` selects durable recovery with current authority checks
and paused pending work. Completed effects are not blindly replayed.

The renderer remains a non-owning file client. It does not create a second
Agent or Shell Process. `/proc` remains Process lifecycle truth; AgentFS remains
the authority for Agent input, output, status, and UI state. Agent-originated
effects continue through Agent Runtime governance and existing namespace,
mount, credential, and sandbox boundaries.

Implementation evidence: PR [#1009](https://github.com/realmorrisliu/alan/pull/1009)
ships CLI-only foreground startup; PR
[#1011](https://github.com/realmorrisliu/alan/pull/1011) verifies independent
simultaneous endpoints, input streams, shutdown, and Tool Process cwd bindings.
Both are merged. PR #1032 records the subsequent explicit recovery, Herdr
view-detach, revocation and no-repeat acceptance; broader or different host/authority
matrices require their own evidence. The [personal continuity plan](../../openspec/changes/restore-project-continuity/)
adds discovery and summaries without reopening background lifetime.

Normative lifecycle behavior lives in the
[`alan-os-host-lifecycle`](../../openspec/specs/alan-os-host-lifecycle/spec.md),
[`alan-shell`](../../openspec/specs/alan-shell/spec.md),
[`alan-renderer-host-contract`](../../openspec/specs/alan-renderer-host-contract/spec.md),
and [`local-alan-os-attachment`](../../openspec/specs/local-alan-os-attachment/spec.md)
specifications.

Historical decision (2026-09-24): bare `alan` attached to the channel's Alan OS
Host. TTY input rendered the Service Manager-owned `/agent/root`; redirected
stdin submitted one task to that Agent. The renderer did not create a second
Shell Process. That channel-wide attachment model is superseded by the
foreground ownership decision above.
