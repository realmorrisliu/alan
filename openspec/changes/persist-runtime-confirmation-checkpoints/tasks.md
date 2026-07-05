## 1. Implementation

- [x] 1.1 Add a `Session` helper that persists checkpoint records with an
  optional `knowledge_root`, while keeping the existing no-root API available.
- [x] 1.2 Persist rollout checkpoint records from runtime confirmation resume
  handling and attach the current namespace `machine/tape` root when available.

## 2. Verification

- [x] 2.1 Add regression tests for runtime confirmation checkpoint persistence
  with a namespace tape root and for fallback persistence when the root read
  fails.
- [x] 2.2 Run targeted agent-engine tests and `openspec validate
  persist-runtime-confirmation-checkpoints --strict`.

## 3. Archive Readiness

- [ ] 3.1 Sync the `runtime-core-contract` delta into `openspec/specs/` before
  archiving this change after implementation lands.
