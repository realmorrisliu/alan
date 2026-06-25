## 1. Kernel Boundary Prep

- [x] 1.1 Confirm the code target is `alan-kernel`.
- [x] 1.2 Add module placement for Process / Agent Process anchors without
  introducing dependencies on `alan-runtime`, `alan-protocol`, compatibility
  transport clients, provider clients, memory stores, or sandbox
  implementations.

## 2. Anchor Types

- [x] 2.1 Add typed ids or descriptors for Process and Agent Process identity,
  parent process identity, Credentials, Paths, Files, stream Files,
  Descriptors, Access Rights, namespaces/mounts, Access Checks, status, and
  exit state.
- [x] 2.2 Add service-handle and service-mount anchors for standard namespace
  roots such as `/proc`, `/agent`, `/srv`, `/bin`, `/lib`, `/man`, and `/mnt`
  without implementing the services.
- [x] 2.3 Add compatibility runtime references where current V1 code still
  references session ids, rollout ids, protocol ids, or projection ids.
- [x] 2.4 Document that AgentFS request/action/tape schemas, Tool manifests,
  Skill packages, Memory Stores, policies, and execution guards remain above
  Kernel.

## 3. Tests

- [x] 3.1 Add serialization round-trip tests for the core Agent Process anchor
  types.
- [x] 3.2 Add tests proving compatibility ids remain runtime references.
- [x] 3.3 Add dependency-boundary tests proving Kernel remains independent from
  runtime, protocol, compatibility transport, providers, memory stores, and
  sandbox implementations.

## 4. Verification

- [x] 4.1 Run focused tests for the Kernel crate.
- [x] 4.2 Run formatting and relevant workspace checks.
- [x] 4.3 Run `openspec validate add-agent-process-kernel-types --strict`.
- [x] 4.4 Run `openspec validate --all --strict`.
