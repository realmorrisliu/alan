## 1. Contract

- [x] 1.1 Consolidate the forty draft remote ADRs (0028–0067) into ADR-0028
  "Remote Attachment Model (Consolidated)" and delete the drafts.
- [x] 1.2 Write the `remote-access-service` capability spec (bootstrap tree,
  one-shot handoff, entry process, lease atomicity, revocation,
  `/mnt/remote` context, `/mnt/peer` imports, attachment-scope threat model,
  gateway prohibition).
- [x] 1.3 Add the `remote-control-contract` freeze delta (legacy, no
  extension, deletion target; durable invariants transferred).
- [ ] 1.4 `openspec validate define-remote-access-service --strict`.

## 2. Vocabulary re-homing

- [x] 2.1 Slim the `CONTEXT.md` Remote* entries to name + one-line meaning +
  Avoid list, pointing semantics at ADR-0028 and this capability.
- [ ] 2.2 Verify no other doc (`docs/`, `openspec/`) still treats the glossary
  or the deleted draft ADRs as the normative source for remote semantics.

## 3. Product-change alignment

- [x] 3.1 Re-scope `add-alan-anywhere-mvp` to the product plane: proposal
  declares the dependency on `remote-access-service` and the
  `remote-control-contract` freeze honestly (no "Modified Capabilities:
  None"), and its tasks gain the relay-latency spike as a gate before
  cloud/account build-out.

## 4. Follow-on implementation (owned by future changes, listed for the map)

- [ ] 4.1 `remote-accessfs`: the Remote Access Service file server above the
  aP wire transport, started by Service Manager, posting
  `/srv/remote-access`.
- [ ] 4.2 Login Namespace Template assembly and Remote Entry Process spawn
  path.
- [ ] 4.3 Lease store, reattach flow, revocation teardown, `/mnt/remote`
  context tree, `/mnt/peer` import wiring.
