## 1. Immediate Connection Service PR

- [x] 1.1 Confirm `enforce-clean-code-architecture-gates` is merged and branch
  the next PR from that exact `main` before unrelated feature work.
- [x] 1.2 Characterize current profile metadata, default selection, mounted
  connection publication, and engine consumption with focused contract tests.
- [x] 1.3 Move one complete profile metadata/selection responsibility from
  Agent Execution Engine into Connection Service without adding a bridge or
  dual writer.
- [x] 1.4 Delete the displaced engine path, remove any dependency it made
  unnecessary, and tighten the source-size and dependency baselines.
- [x] 1.5 Run the canonical gate and focused tests, open the behavior-preserving
  PR, and resolve Codex review until no new issue remains.

## 2. Agent Runtime Service Ownership

- [x] 2.1 Record the live Process clone, mount, AgentFS lifecycle, and child
  namespace assembly call paths with focused characterization tests.
- [x] 2.2 Consolidate root Agent Process namespace and AgentFS lifecycle
  assembly behind Agent Runtime Service and delete the duplicated Service
  Manager supervisor composition path.
- [x] 2.3 Move child Agent Process clone and namespace assembly into Agent
  Runtime Service while preserving narrower mount behavior.
- [x] 2.4 Narrow Agent Execution Engine inputs to the assembled namespace and
  transition-owned files.
- [x] 2.5 Remove obsolete `alan-agent-engine` normal dependencies on Kernel and
  file-server implementation crates as each responsibility moves, tightening
  the graph gate in the same PR.

## 3. Rust Source Debt Burn-down

- [x] 3.1 Split remaining oversized Agent Execution Engine runtime owners along
  transition, Tool, policy, memory, rollout, and prompt-cache responsibilities.
- [x] 3.2 Split oversized Agent Definition, Skill, Tool adapter, and sandbox
  owners into cohesive modules with focused tests.
- [x] 3.3 Split oversized provider, file-server, Service Manager, CLI, and TUI
  owners at their adapter/domain boundaries.
- [x] 3.4 Split oversized shell-core production owners without moving reusable
  domain behavior back into Swift.
- [x] 3.5 Extract oversized Rust test sources into adjacent white-box or public
  contract suites according to `rust-test-placement-contract`.
- [x] 3.6 Tighten the baseline after every slice and finish with no Rust source
  over 1,000 lines and an empty oversized-source inventory.

## 4. Apple Architecture Debt Burn-down

- [x] 4.1 Reconfirm and classify the 15-warning ledger by durable owner before
  changing Apple source.
- [x] 4.2 Split large shell root/controller owners into named orchestration,
  presentation, persistence, and command collaborators.
- [x] 4.3 Split terminal host/runtime owners along attachment, input, surface,
  overlay, and metadata responsibilities.
- [x] 4.4 Delete shallow pass-through bridges and keep shell-core FFI operations
  in their documented narrow adapter owners.
- [x] 4.5 Lower the Apple warning ledger and executable ceiling in every focused
  PR, with fresh Alan Dev rendered verification for touched surfaces.
- [x] 4.6 Remove the non-zero ledger and make report and strict modes both pass
  with zero warnings.

## 5. Final Verification And Archive

- [x] 5.1 Run the canonical quality gate, workspace tests, focused runtime and
  Apple tests, and strict OpenSpec validation.
- [x] 5.2 Verify the dependency inventory contains no removed transitional edge,
  the Rust oversized baseline is empty, and Apple strict mode reports zero.
- [x] 5.3 Keep every implementation PR and Codex review clean, and confirm no
  slice intentionally changed product behavior.
- [ ] 5.4 After all implementation PRs merge, sync delta specs into canonical
  specs and archive this change.
