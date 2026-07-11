## 1. Freeze The Contract Inventory

- [x] 1.1 Enumerate every non-archived canonical spec, active change, ADR, current doc, AGENTS.md section, CLI/help contract, and test contract that semantically depends on Alan daemon, Agent Session, HTTP/WebSocket, relay, reconnect, scheduler-extension, or client API ownership.
- [x] 1.2 Classify every `session` and `daemon` hit as retired Alan architecture, legitimate terminal/auth/third-party terminology, immutable archive history, or false positive, and record the classification in the change review evidence.
- [x] 1.3 Build a requirement-by-requirement ownership matrix for `daemon-api-contract`, `remote-control-contract`, and `runtime-core-contract`, assigning each invariant to a surviving capability or marking it obsolete with rationale.
- [x] 1.4 Confirm that the inventory contains no replacement Thread, Conversation, Run, globally addressable execution manager, or Alan for macOS attachment decision.

## 2. Fold Resolved Successor Contracts

- [x] 2.1 Reconcile the accepted requirements from `define-alan-app-service-integration` into this change's `alan-app-service-integration` delta without carrying compatibility transport language.
- [x] 2.2 Reconcile the accepted requirements from `define-remote-access-service` into this change's `remote-access-service` delta without carrying daemon, relay, or Session identity.
- [x] 2.3 Mark the two original active changes as superseded by `remove-daemon-era-contracts` using the repository's supported OpenSpec retirement workflow, preserving their historical artifacts.
- [x] 2.4 Verify that no other active change still depends on the superseded changes or treats them as the source of current authority.

## 3. Remove Obsolete Capability Owners

- [ ] 3.1 Apply the removal delta for `daemon-api-contract` and verify no surviving canonical capability inherits REST route, WebSocket, endpoint-schema, or API client ownership.
- [ ] 3.2 Apply the removal delta for `remote-control-contract` and verify no surviving capability inherits relay, remote Session attachment, or reconnect transport ownership.
- [ ] 3.3 Apply the removal delta for `runtime-core-contract` only after every still-valid invariant in its ownership matrix has a tested destination.
- [ ] 3.4 Remove the three obsolete capability directories from the canonical `openspec/specs/` surface during spec synchronization instead of leaving deprecated tombstones.

## 4. Reassign Process, Agent, Memory, And Governance Semantics

- [ ] 4.1 Apply the Process- and Agent Machine-shaped deltas for `agent-file-layout-contract`, `agent-runtime-ui-file-surfaces`, `child-run-lifecycle`, `namespace-sandbox-projection`, and `os-sandbox-enforcement`.
- [ ] 4.2 Apply the direct-launch and canonical-path deltas for `agent-root-layout` and `agent-root-layout-contract`, including repository hygiene for generated Process, machine, rollout, checkpoint, and Memory Store state.
- [ ] 4.3 Apply the Session-free provenance and path deltas for `runtime-memory-contract`, `runtime-memory-surfaces`, and `workspace-runtime-state-hygiene`.
- [ ] 4.4 Apply the Process/capability ownership deltas for `governance-tooling-contract` and `coding-steward-contract` without adding a replacement execution center object.
- [ ] 4.5 Review the resulting canonical specs together and verify lifecycle belongs to Process, machine state to Agent Machine, IO/control to AgentFS and `/proc`, execution evidence to rollout/checkpoint files, and continuity to Memory Stores and handoff.
- [ ] 4.6 Apply the positive dependency-isolation wording for `plan9-kernel-substrate` so Alan Kernel depends only on aP and does not preserve the old protocol as a comparison owner.

## 5. Reassign Provider, Skill, Tool, And Harness Semantics

- [ ] 5.1 Apply the direct CLI and Agent Process binding deltas for `provider-connection-contract`, `provider-request-controls`, and `openrouter-provider-adapter`.
- [ ] 5.2 Apply the package/local-authoring deltas for `skill-system-contract`, removing all daemon Skill management and response vocabulary.
- [ ] 5.3 Apply the Agent Execution Engine ownership update for `tool-result-presentation` while retaining only Event/Op records still used as the execution alphabet.
- [ ] 5.4 Apply the native-boundary rewrite for `runtime-harness-contract`, removing bridge roles, envelopes, reconnect, Session scopes, and bridge SLOs.
- [ ] 5.5 Apply the AgentFS offset/read-gap rewrite for `sandbox-autonomy-invariants` and the current-owner rewrite for `rust-test-placement-contract`.

## 6. Reset Renderer And macOS Contracts

- [ ] 6.1 Apply the mounted-file renderer deltas for `alan-renderer-host-contract`, `rust-inline-tui`, and `agent-runtime-ui-file-surfaces` without compatibility-path wording.
- [ ] 6.2 Apply the platform-neutral dependency rewrite for `shell-workspace-core-contract`.
- [ ] 6.3 Apply the active-macOS-only source ownership and deleted-consumer requirements for `macos-app-architecture-maintainability`.
- [ ] 6.4 Apply the channel verification and compatibility-consumer absence checks in `macos-shell-build-test-contract`.
- [ ] 6.5 Apply the local-only Settings navigation and row contracts in `macos-shell-ui-ux-conformance`, leaving Alan for macOS integration absent rather than stubbed.
- [ ] 6.6 Apply the terminal continuity wording update in `macos-shell-terminal-lifecycle` without disturbing legitimate terminal, helper, login, drag, or authentication Session terminology.
- [ ] 6.7 Apply the CLI-process activity wording in `macos-shell-workspace-persistence` and `macos-terminal-activity-semantics` without selecting a native macOS-to-Alan OS attachment.

## 7. Update Current Authority Surfaces

- [x] 7.1 Update AGENTS.md component names, project tree, dependency graph, build commands, environment variables, configuration guidance, and remove the HTTP API section so it describes only current Alan OS and surviving implementation owners.
- [x] 7.2 Update README.md, CONTEXT.md, current architecture docs, current ADR cross-references, current operator docs, and current examples to match the clean Process/file/service model.
- [x] 7.3 Update any active OpenSpec change that cites the removed capabilities or assumes daemon/Session ownership; do not edit `openspec/changes/archive/`.
- [ ] 7.4 Update canonical capability Purpose text where removal of bridge, Session, client, or compatibility ownership makes the old Purpose inaccurate.
- [x] 7.5 Verify current docs state that Alan for macOS attachment is deliberately undecided and link to ADR-0029 without proposing a transport or lifecycle owner.

## 8. Validate Contract Coherence

- [x] 8.1 Run strict validation for `remove-daemon-era-contracts` and every affected capability delta, resolving all errors and warnings owned by this change.
- [x] 8.2 Run strict validation across the full OpenSpec tree and confirm any pre-existing warning is identified separately from this change.
- [x] 8.3 Run a semantic current-tree audit excluding immutable archives and verify no current contract authorizes the retired daemon, Agent Session API, HTTP/WebSocket client path, relay, or compatibility transport.
- [x] 8.4 Verify all retained uses of Event/Op are execution-alphabet records rather than client/server Session protocol.
- [x] 8.5 Verify all retained uses of `session` or `daemon` in current contracts are legitimate terminal, authentication, Apple LaunchDaemon, or third-party concepts and document the allowlist rationale.

## 9. Review And Stacked Merge Gate

- [ ] 9.1 Open the contracts PR as the prerequisite of `remove-daemon-era-implementation`, link both changes and ADR-0029, and state that neither PR may create a supported intermediate release.
- [ ] 9.2 Keep the contracts PR unmerged until the implementation PR is implementation-complete, all required checks are green, all review threads are resolved, and a delayed refresh finds no new Codex review findings.
- [ ] 9.3 Continue polling checks, review threads, head SHA, and Codex review on both PRs without relying on the user to report status; any new commit invalidates the prior green gate.
- [ ] 9.4 Merge the contracts PR first only when both PRs satisfy the gate, then immediately rebase the implementation PR onto the merged contracts commit and rerun its full verification.

## 10. Synchronize And Archive

- [ ] 10.1 After both implementation PRs merge, synchronize these deltas into canonical `openspec/specs/`, including deletion of the three obsolete capability directories and correction of affected Purpose text.
- [ ] 10.2 Re-run strict per-change and full-tree OpenSpec validation against the synchronized canonical surface.
- [ ] 10.3 Archive `remove-daemon-era-contracts` first and `remove-daemon-era-implementation` second while preserving all older archive history unchanged.
- [ ] 10.4 Verify the archive commit leaves no open or superseded active change that still presents daemon-era contracts as current authority.
