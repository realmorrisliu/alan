## 1. Kernel Boundary Prep

- [x] 1.1 Confirm the code target is `alan-kernel`.
- [x] 1.2 Add module placement for Agent Capability semantic types without
  introducing dependencies on `alan-runtime`, `alan-protocol`, daemon clients,
  provider clients, memory stores, or sandbox implementations.

## 2. Semantic Types

- [x] 2.1 Add typed ids for Agent Capability descriptors, Agent Runs, Context
  Grants, Result Contracts, execution guards, and audit records.
- [x] 2.2 Add Agent Capability descriptor structs for `explain`, `summarize`,
  `plan`, `propose_commands`, and `delegate`.
- [x] 2.3 Add Context Grant structs for app identity, target refs, view refs,
  selected ranges, allowed reads, allowed commands, privacy policy, and evidence
  requirements.
- [x] 2.4 Add Result Contract structs for answers, summaries, plans, citations,
  evidence, proposed commands, follow-up questions, uncertainty, and audit
  summary.
- [x] 2.5 Add Effect Class, Command Risk, Execution Guard metadata, and
  audit/evidence reference types.

## 3. Tests

- [x] 3.1 Add serialization round-trip tests for the core Agent Capability types.
- [x] 3.2 Add descriptor taxonomy tests for the five V1 descriptors.
- [x] 3.3 Add dependency-boundary tests proving Kernel remains independent from
  `alan-runtime`, `alan-protocol`, daemon clients, provider clients, memory
  stores, and sandbox implementations.

## 4. Verification

- [x] 4.1 Run focused tests for the Kernel crate.
- [x] 4.2 Run formatting and relevant workspace checks.
- [x] 4.3 Run `openspec validate add-agent-capability-kernel-types --strict`.
- [x] 4.4 Run `openspec validate --all --strict`.
