## 1. Capability Routing

- [ ] 1.1 Define the first capability vocabulary for workspace access, shell, network, GitHub, browser, side effects, and evidence-artifact access.
- [ ] 1.2 Add capability descriptors for built-in delegated targets and parent tool profiles.
- [ ] 1.3 Implement task capability classification before delegated launches that target another workspace or external state.
- [ ] 1.4 Block or narrow delegated launches when target capabilities do not satisfy task requirements.
- [ ] 1.5 Record capability-routing decisions and mismatch recovery paths in rollout or session metadata.
- [ ] 1.6 Add tests for GitHub issue review routing, local-only workspace inspection routing, and mismatch recovery.

## 2. Evidence Provenance

- [ ] 2.1 Design and implement a runtime evidence artifact record with id, source ids, digest, original size, redaction summary, scope, and storage reference.
- [ ] 2.2 Persist long tool stdout and structured outputs as evidence artifacts when rollout-inline durable payloads are truncated.
- [ ] 2.3 Persist or reference long delegated child output as parent-owned evidence associated with the child-run record.
- [ ] 2.4 Add model-facing preview plus evidence reference projection for truncated tool and child outputs.
- [ ] 2.5 Add final-answer evidence summary metadata linking answers to relevant tool calls, child runs, and artifacts.
- [ ] 2.6 Add tests proving prompt truncation does not remove durable evidence and that incomplete evidence is recorded as a limitation.

## 3. Authorized Artifact Access

- [ ] 3.1 Add runtime or daemon artifact-read APIs that authorize by parent session, child-run relationship, and workspace policy.
- [ ] 3.2 Update delegated `output_ref` generation to point at the authorized artifact surface while retaining raw rollout path only as debug metadata when appropriate.
- [ ] 3.3 Return structured errors for missing, expired, denied, or unresolved evidence references.
- [ ] 3.4 Add daemon endpoint registry entries, relay metadata, and client helper coverage for evidence-artifact read/list/preview surfaces.
- [ ] 3.5 Add tests for parent-readable child evidence, unrelated-session denial, and parent workspace guard compatibility.

## 4. Human-Visible Lifecycle

- [ ] 4.1 Extend run status mapping to distinguish approval wait, approval resume, active running, delegated child wait, and terminal states.
- [ ] 4.2 Ensure approval replay transitions run state back to running before the approved tool execution or next model step is observed.
- [ ] 4.3 Emit parent-visible child lifecycle events for child start, progress/heartbeat, and terminal state while delegated calls are pending.
- [ ] 4.4 Update daemon session/read/list or child-run DTOs to expose lifecycle and progress metadata.
- [ ] 4.5 Update TUI or client status rendering so approval and delegated-work states are understandable without raw rollout inspection.
- [ ] 4.6 Add tests for yielded-to-running approval resume, long-running child progress visibility, and child terminal timeline events.

## 5. Memory Surface Salience

- [ ] 5.1 Update fallback `Current Goal` derivation to ignore runtime control messages and tool approval controls.
- [ ] 5.2 Add salience filtering so short ambiguous follow-ups do not replace the previous substantive task goal.
- [ ] 5.3 Preserve evidence artifact, rollout, or child-run references in generated memory surfaces when recent work relied on truncated evidence.
- [ ] 5.4 Add tests for one-letter follow-ups, approval controls, new substantive requests, and evidence-reference continuity.

## 6. Skill And Documentation Updates

- [ ] 6.1 Update delegated skill injection guidance to describe capability descriptors, mismatch recovery, and authorized output references.
- [ ] 6.2 Update relevant built-in skill docs after the runtime surfaces exist so agents know how to inspect evidence without raw cross-workspace reads.
- [ ] 6.3 Document the user-visible lifecycle states and evidence artifact model in the appropriate runtime or daemon docs.

## 7. Verification And Archive Readiness

- [ ] 7.1 Run focused runtime tests for capability routing, evidence artifacts, delegated result handoff, lifecycle state transitions, and memory surfaces.
- [ ] 7.2 Run focused daemon/API tests for artifact endpoints, endpoint registry metadata, and lifecycle DTOs.
- [ ] 7.3 Run `cargo fmt --all`.
- [ ] 7.4 Run `cargo test --workspace` or document any environment-blocked subset with focused passing tests.
- [ ] 7.5 Run `openspec validate harden-agent-operating-system-contracts --strict`.
- [ ] 7.6 Before archiving, sync accepted delta specs into `openspec/specs/`, validate the full spec tree, and confirm archive readiness.
