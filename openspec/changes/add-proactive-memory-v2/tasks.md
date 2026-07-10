## 1. Memory Store Transaction Surface

- [ ] 1.1 Add store-owned proposal, status, result, events, ledger, and `ctl`
  surfaces to `alan-memfs` using ordinary aP operations and commit-on-clunk.
- [ ] 1.2 Implement atomic target-document plus ledger commit with namespace
  target containment and access-right checks.
- [ ] 1.3 Project the current `.alan/memory/` layout through a Workspace Memory
  Store compatibility adapter under `/mnt/mem`.

## 2. Planning And Validation

- [ ] 2.1 Change Agent Execution Engine promotion output to a bounded candidate
  containing memory kind, selected mounted store, namespace target, evidence
  class and references, confidence, disposition, observation, and rationale.
- [ ] 2.2 Remove direct durable memory mutation from engine promotion, flush, and
  consolidation paths; submit eligible proposals to the Memory Store.
- [ ] 2.3 Enforce direct-statement, repeated-behavior, external-evidence, dedupe,
  staging, and disabled-memory rules across runtime planning and store commit.
- [ ] 2.4 Apply mandatory sensitive-data rejection/redaction at the store commit
  boundary before any target, staging, result, or ledger persistence.

## 3. Inspection And Revert

- [ ] 3.1 Implement store-owned recent-write listing through ledger directories
  and write events.
- [ ] 3.2 Implement precise `revert` control with anchor/hash verification,
  atomic target+ledger update, and `manual_resolution_required` failure.
- [ ] 3.3 Reimplement `alan memory recent|show|revert` as file clients over the
  mounted Memory Store; add no daemon endpoints.

## 4. Memory Surface Integration

- [ ] 4.1 Update recall, handoff, session-summary, and daily-note readers to use
  namespace Memory Store paths and bounded ledger/evidence references.
- [ ] 4.2 Ensure prompt-facing surfaces exclude reverted content and never copy
  complete ledger records.
- [ ] 4.3 Update the built-in memory skill to propose or explain store writes
  without directly mutating stable, staged, inbox, daily, or ledger files.

## 5. Verification And Archive Readiness

- [ ] 5.1 Add unit and integration tests for proposal commit, target containment,
  store authority, disabled mounts, redaction, dedupe, staging, atomic ledger
  writes, revert, conflict failure, and prompt exclusion.
- [ ] 5.2 Add compatibility tests proving `.alan/memory/` remains readable through
  the Workspace Memory Store adapter without leaking raw host paths.
- [ ] 5.3 Run focused engine/memfs tests and `cargo test --workspace` or document
  unrelated blockers with the focused suites green.
- [ ] 5.4 Run `openspec validate add-proactive-memory-v2 --strict` and full strict
  OpenSpec validation.
- [ ] 5.5 After merge, sync accepted deltas into `openspec/specs/` before
  archiving the change.
