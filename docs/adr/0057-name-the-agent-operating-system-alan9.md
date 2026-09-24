# Name the agent operating system alan9

Status: accepted naming direction, 2026-09-24; repository-wide prose alignment
is pending. Supersedes the `Alan OS` system label, without changing the ownership
decisions in ADR-0054/0055/0056.

Alan remains the programmable personal computing product. Its agent operating
system is named **alan9**, always lowercase. The existing Kernel and system
Host are called **alan9 Kernel** and **alan9 Host**. The user entry command
remains `alan`; Alan Shell and existing service role names remain available.

The name acknowledges Plan 9's influence on namespaces and file services.
alan9 runs on a host operating system and uses its own aP protocol; the name
does not claim a bootable replacement OS or Plan 9/9P compatibility.

This decision changes system terminology. Crate and executable names, Rust
identifiers, aP, namespace paths, store paths, channel identities and OpenSpec
capability IDs retain their existing spelling. Historical ADRs and archived
changes retain their original terminology. A later machine-identifier rename
would require its own justification and migration scope.

Whether Alan's TUI and Shell become one interaction/execution abstraction is
an independent design question. Neither their current separation nor a merge
is mandated by the alan9 name. Model-assisted command routing and Jev remain
unimplemented and are not approved runtime changes by this ADR.

Normative naming delta and adoption tasks live in
[rename-alan-os-to-alan9](../../openspec/changes/rename-alan-os-to-alan9/).
The interaction exploration lives in the existing
[unified-input change](../../openspec/changes/unify-agent-command-input/design-interview.md).
