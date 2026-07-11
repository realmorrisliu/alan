# Implementation Removal Review Evidence

Snapshot date: 2026-07-11.

Implementation branch base: contracts commit `c4115bfd`.

## Removal boundary

The implementation deletes the Alan host server, public control command,
HTTP/WebSocket/relay/scheduler stores and routes, Rust network TUI client, Apple
Console/API consumers, Agent Engine Session owner, Session identity in Event
envelopes and persistence, and every owned compatibility reader or fallback.

It retains:

- Event/Op as the Agent Execution Engine and AgentFS execution alphabet;
- provider-owned HTTP clients and OAuth callbacks;
- OpenRouter SDK session_id request metadata;
- terminal and login sessions;
- Apple SMAppService.daemon privileged-helper registration;
- Docker daemon checks used by the SWE-bench package.

No Thread, Conversation, globally addressable Run, execution manager, migration
reader, dual write, or Alan for macOS attachment design was introduced.

## One-time local destruction

The pre-removal path helpers and actual stable/dev roots were inspected before
deletion. Candidate paths were rejected if they were symlinks, escaped their
Alan-owned root, or overlapped surviving configuration.

After stopping the active old host process, the following recognized legacy
stable paths were deleted:

    ~/.alan/daemon.pid
    ~/.alan/host.toml
    ~/.alan/sessions/
    ~/.alan/tasks/
    ~/.alan/memory/
    ~/.alan/runtime/stable/sessions/
    ~/.alan/.alan/sessions/
    ~/.alan/.alan/memory/

No corresponding legacy dev paths existed. Current auth.json,
connections.toml, AgentRoots, workspace registry data, and Process-shaped
rollouts were preserved. ~/.alan and ~/.alan-dev remain host-private backing
roots, not public Alan OS namespace or persistence-format contracts.

This was an explicit one-time operator action. The repository contains no
cleaner, startup deletion hook, backup, migrator, fallback reader, or dual-write
path.

## Built-product evidence

- just clean removed the prior build tree.
- just check passed formatting, Clippy with warnings denied, and all workspace
  tests from the clean tree.
- just build produced the release binary.
- Release help contains only direct connection, init, workspace, Skill, and
  shell-control commands.
- alan daemon --help is rejected as an unknown command.
- The release binary contains no old route, endpoint, reconnect, environment,
  or WebSocket strings.
- cargo tree -p alan --depth 1 contains no host-server or server-WebSocket
  dependency.
- In an isolated HOME, direct connection commands configured a test profile,
  bare release alan mounted /agent/1, rendered the file-backed TUI, and exited
  normally with Ctrl-Q.
- Stable and dev isolated init/TUI flows created only channel-scoped rollout and
  Memory Store paths; no deleted legacy format was recreated.

## Apple evidence

- Shell core and shell core FFI Rust tests passed.
- Swift shell-core adapter, runtime metadata, Settings, terminal account,
  terminal runtime, terminal surface, shell automation, performance, updater,
  and appcast checks passed.
- The active Xcode target built successfully after placing the runtime-loaded
  shell-core FFI copy phase after standard framework/resource phases.
- just install-dev installed a signed and validated Alan Dev.app plus alan-dev.
- A fresh installed app launch exposed shell control state with no old Console,
  endpoint, Session, run-status, pending-yield, or WebSocket keys.
- The isolated UI smoke created a space, opened a tab, split the terminal, and
  captured the rendered result. The active scene is the terminal workspace with
  no Console or daemon-backed Settings surface.

## Semantic absence guard

scripts/check-daemon-era-absence.sh rejects:

- deleted module and Apple consumer paths;
- old public environment variables, config files, routes, reconnect fields,
  API client/model names, and Session identity types;
- unclassified daemon or Agent Session terminology in source, current docs,
  active changes, effective canonical specs, tests, fixtures, build wiring, and
  public CLI help;
- a callable alan daemon command.

The guard excludes immutable OpenSpec archives. While the two removal changes
are active, it scans their deltas as the effective authority for matching
canonical capabilities; that exception disappears after synchronization and
archive. Narrow allowlists record the surviving owners listed above and do not
permit a general word-only bypass.

The guard runs in CI against the built debug binary.

## Validation result

    openspec validate remove-daemon-era-contracts --strict       passed
    openspec validate remove-daemon-era-implementation --strict  passed
    openspec validate --all --strict                              79 passed, 0 failed
    check-daemon-era-absence.sh                                   passed
