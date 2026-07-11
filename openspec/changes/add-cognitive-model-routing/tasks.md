## 1. Cognitive Connection Mounts

- [ ] 1.1 Add cognitive config for System 1/System 2 connection profiles,
  configured default role, and optional per-role reasoning-effort intent.
- [ ] 1.2 Resolve profiles to callable llmfs Connections and bind stable role
  aliases in the coordinating Agent Process namespace.
- [ ] 1.3 Reject missing or unauthorized role mounts without provider/profile id
  fallback outside the namespace.

## 2. Routing Files And Control

- [ ] 2.1 Add AgentFS `machine/routing/{config,status,current,result,events}`
  read-only surfaces with bounded renderer-safe metadata and offset-resumable
  events; no routing-specific `ctl` file.
- [ ] 2.2 Implement `route auto`, `route next system-1`, and `route next
  system-2` commands on the agent-runtime-owned `machine/ctl`, with one-input
  consumption, deterministic gate precedence, and refusal records.
- [ ] 2.3 Verify routing identity and control exist only in Process,
  `machine/ctl`, and `machine/routing` file owners.

## 3. Attempt Process Orchestration

- [ ] 3.1 Spawn System 1 as an Agent Process with bounded task descriptors, one
  active Connection, read-only mounts, and a `/bin` union containing only
  read-only Tools.
- [ ] 3.2 Assert the speculative namespace, `/srv` filtering, and Tool manifests
  before starting the first System 1 Generation.
- [ ] 3.3 Parse provider-neutral `route/escalate` stream records and spawn a
  sequential System 2 attempt with an explicitly assembled namespace.
- [ ] 3.4 Publish one accepted parent result with attempt/process/action
  provenance; keep speculative output out of accepted `io/output`.
- [ ] 3.5 Route proposed System 1 mutations to parent/deeper review rather than
  executing them from the restricted attempt.

## 4. Request Controls And Continuation

- [ ] 4.1 Resolve reasoning effort after cognitive Connection selection and
  write it into the provider-neutral llmfs Generation document.
- [ ] 4.2 Keep provider adapters role-neutral and verify unsupported controls
  fail before Generation starts.
- [ ] 4.3 Partition or clear provider-native continuation by Connection, model,
  Credential scope, role, prompt fingerprint, Tool manifest fingerprint, and
  relevant controls.

## 5. Verification And Archive Readiness

- [ ] 5.1 Add tests for role mount resolution, unavailable mounts, routing
  precedence, `machine/ctl` route-command consumption, forced gates, and routing
  event resume.
- [ ] 5.2 Add tests proving System 1 cannot see or execute side-effecting Tools,
  escalation spawns System 2, speculative drafts stay unaccepted, and accepted
  provenance names both attempts.
- [ ] 5.3 Add request-control and provider-continuation compatibility tests.
- [ ] 5.4 Run focused AgentFS/engine/llmfs tests and `cargo test --workspace` or
  document unrelated blockers with focused suites green.
- [ ] 5.5 Run strict validation for this change and the full OpenSpec tree.
- [ ] 5.6 After merge, sync accepted deltas into `openspec/specs/` before
  archiving the change.
