## 1. Contract spec

- [x] 1.1 Define agent-as-conforming-process (no kernel type).
- [x] 1.2 Define the generic process layout: the full `/proc` substrate layout
  (identity/parentage/credentials/namespace/exit state) plus the `io/`/`status`/
  `ctl` IO/control subset.
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

## 1a. Access discipline (consolidated from D7–D12 + external-writers)

- [x] 1a.1 Define `ctl` scoping: one `ctl` per lifecycle-bearing object
  (`/proc/<pid>/ctl` generic, `machine/ctl` runtime), no global/per-leaf `ctl`;
  leaf state (`machine/status`, `requests/<id>/status`) is read-only.
- [x] 1a.2 Define `machine/tape`/`events` append-only and the generation
  exclusive-write tape lease (one writer, open readers; amend-window is the yield).
- [x] 1a.3 Define actor-keyed write authority and extension-by-interpose (the iron
  law: who-may-{read,write,mount,interpose}, no out-of-band API).
- [x] 1a.4 Define the external-writer prerequisite: no non-engine writer until the
  tape lease is enforced at the aP protocol layer (folds in the retired
  `add-external-namespace-writers` requirement).
- [x] 1a.5 Define the in-band self-describing namespace (per-stream record
  vocabulary + `ctl`-help; minimal form, expanded incrementally).

## 2. Verification

- [x] 2.1 Run `openspec validate define-agent-file-layout-contract --strict`.
- [x] 2.2 Run `openspec validate --all --strict`.

## 3. Conformance test-kit

- [x] 3.1 Provide a conformance checker that, given a process directory, verifies
  the generic process layout (`io/input`, `io/output`, `io/events`, `status`,
  `ctl`) and, for agents, the full superset (`requests/`, `actions/`, `machine/`,
  `context/`, `children/`, and the top-level aggregate `events` stream). This
  gives the convention teeth without a kernel type.
  Done 2026-07-02: `alan-agentfs` now exports `AgentConformanceChecker`, an
  aP-only checker over an arbitrary `InProcessTransport`. It verifies the
  generic process files plus the agent superset under a supplied process path,
  so agent-ness is tested by walking files rather than consulting a kernel type.
  Coverage runs it against the current `/agent/<pid>` overlay.
- [x] 3.2 Verify dynamic containers (`requests/`, `actions/`) expose an `events`
  stream observable by blocking read (D8).
  Done 2026-07-02: the checker opens `requests/events` and `actions/events`,
  records the current stream offset, clone-opens a child in the corresponding
  container, and proves the events stream unblocks with new bytes. This is
  covered by `conformance_checker_verifies_dynamic_container_event_streams`.
- [x] 3.3 Verify `/agent` resolves as an overlay over `/proc` and that `/agent/root`
  follows the current root pid while durable identity stays the home path (D4/D7).
  Done 2026-07-02: `AgentRootFs` now unions proc-owned generic files
  (`status`, `ctl`, `parent`, `credentials`, `exit`, `namespace`) into
  `/agent/<pid>` while keeping agent-owned surfaces (`io`, `machine`,
  `requests`, `actions`, `context`, `children`, `events`) backed by AgentFS.
  The checker proves `/agent/root` has the same conforming surface as the
  current root pid.
- [x] 3.4 Make the checker runnable by any third-party runtime against its own
  exported tree, so conformance — not a kernel flag — is what makes a runtime's
  agents operable.
  Done 2026-07-02: the checker API takes only an aP transport and paths; it has
  no dependency on `alan-kernel`, `alan-agent-engine`, or Alan process types.
  Any third-party runtime that exports a compatible aP tree can run the same
  checker against its own process directory.

## 4. Follow-up (separate changes)

- [x] 4.1 Map current session / tape / yield / tool-call behavior onto this
  layout in `introduce-alan-kernel-runtime` (the projection file server) and run
  the conformance test-kit against it.
  Done 2026-07-02: the superseding `refactor-engine-namespace-native` path maps
  session IO, tape, yielded requests, and tool/action records onto AgentFS. The
  namespace-native M2 test now runs through the real `/agent` overlay (not a
  direct `/agent/1` mount), then runs `AgentConformanceChecker` against the live
  process tree. The request/action file test also runs against the overlay and
  verifies the same conformance after writing `requests/<id>/` and
  `actions/<id>/` state.
- [x] 4.2 Specify the LLM provider, memory, tool, and skill file servers that
  this contract references.
  Done 2026-07-02: the contract now names the referenced external file-server
  boundaries: `alan-llmfs` at `/srv/llm` and `/mnt/llm`, Memory Stores through
  `/srv/mem` handles and `/mnt/mem` mounts/descriptors, Tools through `/bin`
  plus `/lib/exec/<tool>/manifest` and `/man/1`, and Skills through
  `/lib/skill/<name>` plus `/man/skill/<name>`. It also states that the agent
  layout consumes these surfaces by namespace/descriptors while their detailed
  protocols remain owned by separate OpenSpec capabilities.
