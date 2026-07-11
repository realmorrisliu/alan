## 1. Pin The Stack And Build The Removal Inventory

- [x] 1.1 Base the implementation branch on the exact reviewed `remove-daemon-era-contracts` head and record that prerequisite in the PR and change artifacts.
- [x] 1.2 Inventory daemon-era production modules, public commands, configuration fields, environment variables, dependencies, features, tests, fixtures, generated artifacts, docs, Apple target members, and local data paths before editing.
- [x] 1.3 Trace every `Session`, `session_id`, `SessionMeta`, `EventEnvelope.session_id`, `SessionReducer`, Session store, and Session-shaped persistence or Memory Store path to its surviving owner or deletion outcome.
- [x] 1.4 Classify shared crates and terms semantically so provider HTTP clients, OAuth callbacks, Apple LaunchDaemon support, terminal/login/auth sessions, and third-party protocol sessions are not removed accidentally.
- [x] 1.5 Add temporary review evidence that proves the removal inventory is exhaustive; do not commit a permanent compatibility registry or migration map into production code.

## 2. Delete Apple Compatibility Consumers First

- [x] 2.1 Verify the active Xcode target graph is macOS-only and that `MacShellRootView` is the product scene before deleting legacy sources.
- [x] 2.2 Delete the legacy Console view tree, Console view models, daemon API client, daemon request/response models, event polling or streaming services, and Session reducers from `clients/apple/`.
- [x] 2.3 Remove daemon-backed Agent, profile, runtime, Skill, endpoint, and local-service rows from macOS Settings without adding unavailable placeholders or substitute data sources.
- [x] 2.4 Remove deleted source files from Xcode groups, build phases, test targets, fixtures, previews, architecture ledgers, and source-membership scripts.
- [x] 2.5 Build and run focused tests for the remaining macOS shell, terminal, shell core, updater, privileged helper, install-channel, and shell-control owners before proceeding.

## 3. Remove Public Daemon Entry Points And Host Configuration

- [x] 3.1 Delete the `alan daemon` command family and all public start, stop, status, foreground, API-contract, remote-control, relay, and scheduler command paths.
- [x] 3.2 Remove `BIND_ADDRESS`, `ALAN_AGENTD_URL`, daemon endpoint defaults, daemon bind and URL fields, daemon PID/binding files, remote-access settings, and their parsing, help, examples, and tests.
- [x] 3.3 Remove daemon-oriented `just` recipes, install/package steps, generated route or endpoint manifests, smoke scripts, and release documentation while preserving unrelated direct CLI and app recipes.
- [x] 3.4 Verify bare `alan`, direct connection management, Skill authoring/inspection, workspace commands that still have a direct owner, and shell-control commands no longer discover or invoke a server path.

## 4. Delete Server, Relay, Scheduler, And Store Implementation

- [x] 4.1 Delete `crates/alan/src/daemon/` in full, including Axum routing, HTTP API handlers, WebSocket transport, auth/connection endpoints, relay, remote control, scheduler, runtime manager, task store, and Session store.
- [x] 4.2 Remove daemon module exports, shared state constructors, route-contract types, server bootstrap code, host lifecycle hooks, and compatibility adapters from the rest of `crates/alan`.
- [x] 4.3 Delete remote attach, replay buffer, reconnect snapshot, fork/resume transport, scheduler-extension, and app-server-specific code outside the daemon directory.
- [x] 4.4 Remove server-only integration fixtures and tests rather than replacing them with ignored, disabled, or mock compatibility suites.
- [x] 4.5 Prune Axum, server WebSocket, relay, endpoint-schema, and other now-unowned dependencies, features, build dependencies, and lockfile entries using actual remaining references as evidence.

## 5. Decompose Agent Engine Session State

- [x] 5.1 Replace the Agent Engine `Session` owner with existing Agent Machine and turn-execution structures; do not introduce a renamed manager or globally addressable Thread, Conversation, or Run object.
- [x] 5.2 Move lifecycle identity assumptions to Process/Agent Process launch and exit state, keeping tape, transition-local state, approval state, and checkpoints under Agent Machine ownership.
- [x] 5.3 Remove `session_id`, `SessionMeta`, Session registries, Session lookup, Session restoration, and Session-scoped locks from Agent Engine public and private APIs.
- [x] 5.4 Update child Agent Process, policy, Tool orchestration, provider binding, compaction, memory flush/recall/promotion, and persistence call sites to receive only the narrower owner-specific inputs they need.
- [x] 5.5 Add or update focused Agent Engine tests proving turns, approvals, Tool calls, child Agent Processes, compaction, and persistence work without Session identity.

## 6. Remove Session Transport Shape From Protocol And TUI

- [x] 6.1 Remove `EventEnvelope.session_id` and all client/server negotiation, route, reconnect, replay-buffer, and compatibility capabilities that exist only for Session transport.
- [x] 6.2 Retain and, where necessary, narrow Event/Op types used by the Agent Execution Engine, AgentFS projection, Tools, approvals, plans, and renderer-visible execution records.
- [x] 6.3 Rename or decompose `SessionReducer` and Session-hydration state into AgentFS snapshot, offset-stream, Agent Machine, transcript, and pending-request owners.
- [x] 6.4 Replace TUI network/client code with the mounted AgentFS and `/proc` file boundary and remove daemon-backed local or remote modes.
- [x] 6.5 Add focused protocol and TUI tests for file hydration, offset continuation, overlap deduplication, retained-data gaps, input writes, control writes, yields, activity, plans, thinking, and Tool presentation.

## 7. Rewrite Persistence And Memory Paths Without Compatibility

- [x] 7.1 Remove Session identity and Session directory naming from rollout records, checkpoint metadata, persistence APIs, filenames, indexes, and recovery fixtures.
- [x] 7.2 Make each rollout and checkpoint self-identifying execution evidence associated with its Agent Process or Agent Machine record, without a global replacement registry.
- [x] 7.3 Replace Session-shaped Working Memory keys and directories with Agent-Process-local ownership and update Episodic Memory and handoff provenance to refer to past Agent Processes or execution records.
- [x] 7.4 Remove readers, fallbacks, migrations, dual writes, aliases, and diagnostics for old Session rollout and memory layouts.
- [x] 7.5 Add fresh-state tests proving current execution, restart evidence, memory recall/promotion, and handoff use only the new Process/machine/rollout/Memory Store layout.

## 8. Remove Stale Tests, Docs, And Dependencies

- [x] 8.1 Delete or rewrite daemon API, WebSocket, relay, reconnect, scheduler, remote Console, and Session-manager tests, fixtures, snapshots, examples, and generated clients according to their surviving behavior owner.
- [x] 8.2 Update current README, AGENTS.md, CONTEXT.md, architecture docs, current ADR references, operator docs, help snapshots, config samples, and scripts to describe the actual post-removal binaries and file-backed usage.
- [x] 8.3 Remove obsolete module directories and files proactively; do not leave commented code, empty compatibility modules, deprecated aliases, tombstone commands, or disabled feature flags.
- [x] 8.4 Prune unused Cargo features/dependencies and Apple source/test dependencies after all call sites are gone, then verify provider HTTP and OAuth owners still compile.
- [x] 8.5 Implement the `documentation-governance` semantic absence guard over source, current docs, canonical specs, active changes, public help, config, tests, and fixtures, excluding immutable OpenSpec archives and explicitly allowlisting legitimate terminal/auth/LaunchDaemon concepts.

## 9. Execute One-Time Local State Destruction

- [x] 9.1 Derive the exact stable and dev legacy data paths from the pre-removal path helpers and inspect actual local state, including daemon bindings/PIDs, Session rollouts/indexes, reconnect state, Session-shaped working/episodic memory, and generated metadata.
- [x] 9.2 Stop any active Alan daemon-era process and preview a bounded deletion list that is limited to recognized Alan-owned roots under the expected stable/dev homes.
- [x] 9.3 Reject the cleanup if any candidate is a symlink, escapes its expected Alan home, aliases a surviving owner, or cannot be tied to a retired format.
- [x] 9.4 Delete the reviewed legacy paths as an explicit one-time operator action, with no committed cleaner, startup hook, installer migration, backup format, or compatibility reader.
- [x] 9.5 Launch and exercise the newly built CLI, TUI, and macOS dev app, then verify none of the deleted legacy paths or formats are recreated.

## 10. Verify The Built Products And Source Absence

- [x] 10.1 Run formatting, Clippy with warnings denied, the full Rust workspace tests, and targeted Agent Engine, protocol, tools, TUI, CLI, persistence, memory, and harness tests from a clean build tree.
- [x] 10.2 Build the release `alan` binary and verify its help, subcommands, linked symbols, config behavior, and representative file-backed TUI flow expose no daemon, Session API, HTTP/WebSocket, relay, reconnect, or scheduler surface.
- [x] 10.3 Run focused Apple architecture tests, shell-core tests, terminal/runtime tests, a clean Xcode build, and a fresh Alan Dev launch smoke using the repository's macOS verification workflow.
- [x] 10.4 Run negative CLI, source, dependency, fixture, project-membership, and current-document checks that prove the deleted implementation cannot be reached or packaged.
- [x] 10.5 Run strict validation for both OpenSpec changes and the full OpenSpec tree, then run the semantic absence guard with archive and legitimate-term exclusions reviewed explicitly.
- [x] 10.6 Inspect the final diff and filesystem for empty directories, orphan modules, stale generated files, accidental archive edits, and any replacement center object introduced during refactoring.

## 11. Review, Merge, And Archive Readiness

- [x] 11.1 Open the implementation PR stacked on the contracts PR and include the removal inventory, one-time cleanup evidence, built-binary evidence, Apple verification, and explicit no-replacement statement.
- [x] 11.2 Keep watching CI, head SHA, all review threads, and Codex review until every finding is addressed or explicitly resolved and a delayed refresh produces no new findings; do not rely on the user to report status.
- [x] 11.3 Confirm the implementation PR is complete and review-green before permitting the prerequisite contracts PR to merge.
- [x] 11.4 After contracts merges, immediately bring the implementation branch onto the merged contracts commit, rerun the full product, source-absence, local-state non-recreation, and OpenSpec validation matrix, and restart the delayed review gate for the new head SHA.
- [x] 11.5 Merge the implementation PR only after the rebased head remains CI-green with zero unresolved threads and no new Codex review findings.
- [x] 11.6 Confirm both changes' deltas are synchronized into canonical specs, the three obsolete capabilities are removed, and both changes are ready to archive in contracts-then-implementation order.
