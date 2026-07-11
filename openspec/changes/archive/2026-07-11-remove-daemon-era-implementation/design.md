## Context

The daemon implementation is not isolated to `crates/alan/src/daemon/`. It includes the public CLI
and host configuration, Axum/WebSocket/relay surfaces, runtime and session stores, connection and
skill endpoints, scheduler extensions, macOS API clients and legacy Console, Agent Engine Session
state, rollout metadata, protocol envelopes, TUI reducer names, generated memory paths, fixtures,
tests, and documentation guards.

This change implements the canonical reset in `remove-daemon-era-contracts`. It intentionally
removes working behavior without replacement and actively deletes recognized local daemon/session
state. It must be complete and review-green before the prerequisite contracts change merges, then
land immediately after it.

## Goals / Non-Goals

**Goals:**

- Remove all executable Alan daemon, Session API, relay, reconnect, scheduler-extension, and
  daemon-backed Apple consumer code.
- Remove Session identity from Agent Engine, protocol envelopes, rollout/checkpoint metadata, TUI,
  and Memory Store paths.
- Preserve the file-backed TUI, direct CLI management, Agent Engine transition behavior, AgentFS
  file surfaces, and macOS terminal workspace where independent of daemon consumers.
- Prune unused dependencies, features, configuration, fixtures, tests, docs, and build recipes.
- Actively delete recognized local stable/dev daemon-session data exactly once during execution of
  this change, leaving no permanent cleaner or compatibility reader in the repository.
- Add a durable semantic guard preventing the deleted architecture from returning.

**Non-Goals:**

- No replacement remote access, scheduling service, mobile Console, macOS agent workspace, or
  macOS-to-Alan OS attachment.
- No data migration, backup, resume compatibility, dual read, dual write, or hidden legacy feature.
- No deletion or rewriting of immutable OpenSpec archive history.
- No removal of legitimate Apple `LaunchDaemon`, terminal-session, authentication-session, or
  third-party protocol concepts solely because they share a word.

## Decisions

### 1. Delete by ownership slice, not directory

The implementation proceeds through an exhaustive ownership inventory:

| Slice | Removal or rewrite |
| --- | --- |
| host server | `crates/alan/src/daemon/`, router, WebSocket, relay, server state |
| public CLI/config | `alan daemon`, API contract output, daemon URL/bind/env/host settings |
| runtime compatibility | daemon runtime manager, bindings, fork/resume/reconnect/scheduler paths |
| engine state | `Session`, `session_id`, SessionMeta, session storage naming and restoration |
| protocol/TUI | session envelopes/client capabilities that exist only for transport, SessionReducer |
| Apple | AlanAPIClient, daemon DTOs/reducers, legacy Console, daemon-backed Settings projections |
| tooling | stale smoke/API guards, fixtures, generated clients, help/docs, dependencies |
| local state | recognized stable/dev daemon bindings, rollouts, working/session memory paths |

Each slice is complete only when production code, tests, fixtures, docs, build wiring, and package
dependencies agree. Moving a file to a `compat` module or hiding a command does not count as
removal.

### 2. Keep the Agent Execution Engine alphabet, remove its transport shell

Event/Op structures still used inside Agent Engine, AgentFS projections, tools, and the file-backed
TUI remain. REST/WebSocket/client-session-specific envelopes, identifiers, negotiation, and payload
members are removed. Types are renamed only when their current name encodes the retired owner; this
change does not redesign the transition alphabet from first principles.

Alternative considered: delete `alan-agent-protocol` wholesale. Rejected because the current engine
and AgentFS still use its transition, approval, plan, tool, and UI records independently of daemon
transport.

### 3. Session state is decomposed without a replacement manager

The former `Session` state is divided among existing structures. Tape and transition-local fields
move under Agent Machine state; Process/Agent Process supplies live identity; rollout/checkpoint
metadata identifies its own record rather than a Session; working memory keys by the owning Agent
Process or machine record; episodic memory and handoff provide continuity. No globally resolvable
Thread, Run, Conversation, or manager is introduced.

### 4. Apple daemon consumers are deleted, not stubbed

The Xcode target is macOS-only and its primary scene already uses `MacShellRootView`; the legacy
Console is unused product source. Delete it, its view model, daemon models/services, and daemon-
backed Settings rows. Do not leave an unavailable placeholder, mock daemon, disabled switch, or
temporary alternative data source. Unrelated terminal, shell-core, Ghostty, update, privileged
helper, and shell-control features remain and must still build.

### 5. Local legacy state is destroyed by a one-time operator step

The tasks enumerate exact stable/dev paths after inspecting all path helpers and current runtime
outputs. During implementation, the operator previews that bounded list and deletes the recognized
daemon bindings, session rollouts, session-shaped working/episodic state, and related generated
metadata. No cleanup utility, migration module, startup detector, installer branch, or backup format
is committed. The final verification proves the current binaries do not recreate those paths.

Alternative considered: automatic first-launch cleanup. Rejected because it would permanently keep
legacy path and format knowledge in the product. Alternative considered: leave old files ignored.
Rejected by the clean-break decision.

### 6. Remove unused dependencies and configuration at the same boundary

Axum, WebSocket, relay, HTTP-server, endpoint-schema, and daemon-only dependencies/features are
removed when no surviving owner uses them. `BIND_ADDRESS`, `ALAN_AGENTD_URL`, daemon URL/bind fields,
port defaults, API manifests, release help, and examples are removed. Dependencies still used for
provider HTTP calls, browser OAuth callbacks, Apple LaunchDaemon support, or another live owner stay
under that owner's terminology and tests.

### 7. A semantic guard owns the lasting deletion invariant

`documentation-governance` gains a current-tree guard covering canonical specs, active changes,
source, public commands, configuration, tests, fixtures, and current docs. The guard rejects known
Alan daemon modules, CLI names, route roots, environment variables, session transport identifiers,
and compatibility language. It excludes OpenSpec archive history and explicitly distinguishes
Apple `LaunchDaemon` and legitimate terminal/auth transport sessions.

## Risks / Trade-offs

- [Removing Session breaks hidden Engine invariants] → Move one ownership slice at a time, keep
  focused Engine/AgentFS/TUI tests green, and prohibit a replacement manager shortcut.
- [Destructive cleanup deletes unrelated data] → Derive exact paths from code, preview the bounded
  list, reject symlinks/out-of-root paths, and delete only Alan-owned recognized roots.
- [Deleting Apple consumers breaks the project file] → Remove source membership and run focused
  Apple tests plus a clean Xcode build and current Alan Dev launch smoke.
- [Removing server dependencies breaks OAuth/provider clients] → Use dependency ownership evidence,
  not package names, before pruning shared HTTP crates.
- [A stale script falsely passes] → Replace positive daemon smoke with negative absence checks and
  run commands from a clean build tree.
- [The two PRs drift] → Pin the implementation proposal/design to the prerequisite change and
  re-validate after rebasing onto its merged commit.

## Migration Plan

1. Inventory every live contract identifier and code/data owner; freeze additions to the old
   surface while both changes are in review.
2. Remove Apple consumers and public daemon entrypoints so no product target depends on the server.
3. Delete daemon server, remote, scheduler, session-store, and runtime-manager modules and prune
   dependencies/configuration.
4. Decompose Agent Engine/TUI/protocol Session identity into existing owners and update persistence
   paths and formats without compatibility readers.
5. Remove stale tests/docs/scripts and add the lasting semantic guard.
6. Preview and execute the bounded one-time local state deletion; verify deleted roots are not
   recreated.
7. Run Rust workspace, targeted Engine/AgentFS/TUI, CLI negative, Apple focused/build/UI, OpenSpec,
   dependency, and source-absence verification.
8. After both PRs are green, merge contracts first and implementation immediately after it; archive
   both in dependency order.

Source rollback remains possible before merge. Destructively deleted local daemon/session data is
intentionally not recoverable and has no rollback path.

## Open Questions

None. Any future Alan for macOS attachment or replacement capability requires a separate OpenSpec
change after this cleanup.
