# Retire the desktop client and prefer existing terminal hosts

Status: accepted, 2026-09-19. Supersedes ADR-0001's product obligation and the
desktop-consumer assumptions of ADR-0027/0044, not Alan OS ownership.

Alan for macOS is retired as a product direction. Herdr is the preferred
terminal host; Alan remains a terminal-neutral programmable personal computing
environment. Alan Shell and its renderer do not recreate windows, workspaces,
tabs, panes or a terminal emulator already supplied by that host.

Herdr owns presentation topology and PTY delivery. Alan OS Host owns system
lifetime; ordinary Process, Agent Machine, AgentFS, services and durable stores
retain their existing ownership. Herdr identifiers and detection states are
not Alan Process identity, execution evidence or authorization.

The existing desktop source, FFI, app distribution and associated contracts
remain maintenance-only pending scoped removal. macOS platform support,
credentials, Host Mounts, OS sandboxing and user data are not retired. Removal
must inventory live consumers and preserve safety and data boundaries. This
decision does not uninstall an app, remove accounts, revoke secrets or change
published feeds.

Ordinary terminal operation precedes optional Herdr-native agent detection.
Herdr 0.9.1's inspected CLI does not advertise an Alan agent kind. No such
integration is claimed implemented. Closing a renderer must not own system
Host shutdown; foreground cancellation versus detach needs explicit terminal
acceptance tests before new behavior ships.

Normative lifecycle scope: `documentation-governance`. Implementation removal
and terminal integration require their own subsequent OpenSpec deltas.

Implementation follow-up (2026-09-20): standalone distribution retirement has
merged. `remove-retired-desktop-source` tracks the approved deletion of App and
shell-core/FFI source and desktop-only requirements; its tasks carry delivery
status. No user migration or desktop compatibility is required.
