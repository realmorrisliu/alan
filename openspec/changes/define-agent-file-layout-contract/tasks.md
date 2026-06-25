## 1. Contract spec

- [x] 1.1 Define agent-as-conforming-process (no kernel type).
- [x] 1.2 Define the generic process layout (`io/`, `status`, `ctl`).
- [x] 1.3 Define the agent superset (`requests/`, `actions/`, `machine/`,
  `context/`, `children/`, and the top-level aggregate `events` stream).
- [x] 1.4 Define `ctl`-command control.
- [x] 1.5 Define `/agent` as an overlay over `/proc` (union of generic `/proc`
  files with the runtime's agent surfaces).
- [x] 1.6 Define the LLM-stream consumer model and namespace-governed effects.
- [x] 1.7 Define namespace-assembled requests, compaction-as-view, and
  tools-as-`/bin`.
- [x] 1.8 Define request/action files with events streams.
- [x] 1.9 Define durable identity as a home tree.
- [x] 1.10 Define provider-server metering.
- [x] 1.11 Add `children/` and a top-level aggregate `events` stream to the agent
  layout (PR #572 review gap).
- [x] 1.12 Define request/action status integrity invariants: reject responses to
  terminal requests; record failed-not-partial and partial-not-satisfied
  (carries the invariants behind the legacy capability-service bugs).
- [x] 1.13 Define Root Agent broad-awareness / narrow-authority via read-only vs
  read-write mounts (PR #572 review gap).
- [x] 1.14 Add an `io/output` completeness invariant (append-only, offset-
  resumable, tail-reachable) so newest output is never a data gap.

## 2. Verification

- [x] 2.1 Run `openspec validate define-agent-file-layout-contract --strict`.
- [x] 2.2 Run `openspec validate --all --strict`.

## 3. Conformance test-kit

- [ ] 3.1 Provide a conformance checker that, given a process directory, verifies
  the generic process layout (`io/input`, `io/output`, `io/events`, `status`,
  `ctl`) and, for agents, the full superset (`requests/`, `actions/`, `machine/`,
  `context/`, `children/`, and the top-level aggregate `events` stream). This
  gives the convention teeth without a kernel type.
- [ ] 3.2 Verify dynamic containers (`requests/`, `actions/`) expose an `events`
  stream observable by blocking read (D8).
- [ ] 3.3 Verify `/agent` resolves as an overlay over `/proc` and that `/agent/root`
  follows the current root pid while durable identity stays the home path (D4/D7).
- [ ] 3.4 Make the checker runnable by any third-party runtime against its own
  exported tree, so conformance — not a kernel flag — is what makes a runtime's
  agents operable.

## 4. Follow-up (separate changes)

- [ ] 4.1 Map current session / tape / yield / tool-call behavior onto this
  layout in `introduce-alan-kernel-runtime` (the projection file server) and run
  the conformance test-kit against it.
- [ ] 4.2 Specify the LLM provider, memory, tool, and skill file servers that
  this contract references.
