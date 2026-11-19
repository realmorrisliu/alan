# Retire the desktop client and prefer existing terminal hosts

Lifetime direction revised 2026-09-27: the user selected independent foreground
Alan invocations and explicitly selected recovery. The older background lifetime
assumptions below are superseded as target direction by
[the archived OpenSpec change](../../openspec/changes/archive/2026-11-19-unify-agent-command-input/disposition.md).
This does not claim the foreground runtime or canonical spec sync is complete.

Amendment 2026-11-19 ([ADR-0061](0061-terminal-native-os-boundary-quartermaster-and-first-party-providers.md)):
the preference for a specific terminal host is withdrawn. Alan is standard-terminal-native
and targets ordinary POSIX/VT terminals (Ghostty, iTerm2, Kitty, WezTerm,
Terminal.app, tmux, SSH). No terminal host is a runtime, acceptance or
authorization dependency, and no host-specific agent detection is planned. The
text below records the original decision; read "preferred terminal host" as
"any ordinary terminal".

Status: accepted, 2026-09-19. Supersedes ADR-0001's product obligation and the
desktop-consumer assumptions of ADR-0027/0044, not Alan OS ownership.

Alan for macOS is retired as a product direction. Existing terminals are the
host; Alan remains a terminal-neutral programmable personal computing
environment. Alan Shell and its renderer do not recreate windows, workspaces,
tabs, panes or a terminal emulator already supplied by the terminal.

The terminal owns presentation topology and PTY delivery. Each foreground Alan
invocation owns its alan9 instance and application lifetime; Process, Agent
Machine, AgentFS, services and durable stores retain their existing ownership
within their respective instances. Terminal-host identifiers and detection states are
not Alan Process identity, execution evidence or authorization. No separate
background Host is required to keep these services available.

At adoption, desktop source, FFI, app distribution and associated contracts
were retained for maintenance pending scoped removal. macOS platform support,
credentials, Host Mounts, OS sandboxing and user data are not retired. Removal
must inventory live consumers and preserve safety and data boundaries. This
decision does not uninstall an app, remove accounts, revoke secrets or change
published feeds.

Ordinary terminal operation is the only supported mode; host-specific agent
detection is not planned and no such integration is claimed. When Alan owns a foreground invocation,
renderer exit ends that invocation and drains its Host; later continuity is
restored only through explicit user-selected recovery. A terminal view detach that
retains the Alan process does not end the invocation or stop its Host. This
explicitly supersedes the earlier rule that renderer closure could not stop its
Host.

Normative lifecycle scope: `documentation-governance`. Desktop source removal
is complete and recorded in the
[archived OpenSpec change](../../openspec/changes/archive/2026-09-23-remove-retired-desktop-source/);
terminal integration continues through its own OpenSpec changes.

Implementation follow-up (2026-09-20): standalone distribution retirement has
merged. The archived `remove-retired-desktop-source` change records the
completed deletion of App and shell-core/FFI source and desktop-only
requirements. No user migration or desktop compatibility is required.
