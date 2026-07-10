## 1. Consumer Re-Audit (Pre-Implementation Gate)

- [x] 1.1 Re-run the consumer audit at implementation time: grep `child_runs` across `crates/`, `clients/apple/`, and scripts; confirm the endpoints still have no callers beyond the payload contract test before deleting anything.
- [x] 1.2 Confirm `align-delegation-capability-with-namespace` has merged and rebase this change's `child-run-lifecycle` delta against the synced spec. (align merged as PR #617 and archived 2026-07-10; delta applied against the synced spec at sync time)

## 2. Remove The Dead Daemon Control Plane

- [x] 2.1 Delete the `/api/v1/sessions/{id}/child_runs*` route handlers in `crates/alan/src/daemon/routes.rs` and their wiring in the router.
- [x] 2.2 Delete `SessionChildRunsList/Get/Terminate` endpoint ids, route constants, and URL builders from `crates/alan/src/daemon/api_contract.rs`; keep route-contract verification green.
- [x] 2.3 Remove child-run coverage from `daemon_payload_contract_test.rs` and any endpoint-registry tests.
- [x] 2.4 Verify the runtime-side registry, liveness classification, progress metadata, and governed termination tool are untouched and their tests still pass.

## 3. Projection Authority

- [x] 3.1 Implement (or verify existing) reconciliation so a child-run record that disagrees with `/proc` process state is corrected from `/proc`, never the reverse.
- [x] 3.2 Add a test: exited child with stale `running` record reconciles to terminal from the authoritative surface.

## 4. Handoff Vocabulary

- [x] 4.1 Replace raw rollout-path fields in failed-handoff metadata with namespace-path references (child home tree or parent-side action record), coordinating with `define-evidence-retention-and-projection`'s `output_ref` work to share the reference shape.
- [x] 4.2 Add tests: timed-out and terminated handoffs carry namespace-path references and no raw host paths.

## 5. Verification And Archive Readiness

- [x] 5.1 Run `just verify` and fix fallout.
- [x] 5.2 Run `openspec validate reconcile-child-run-lifecycle-with-namespace --strict`.
- [x] 5.3 Open PR (call out the endpoint removal explicitly for out-of-tree consumers), address review, merge. (PR #619, merged 2026-07-10)
- [x] 5.4 After merge, sync delta specs into `openspec/specs/` (order: after align and evidence changes) and confirm archive readiness. (synced 2026-07-11)
