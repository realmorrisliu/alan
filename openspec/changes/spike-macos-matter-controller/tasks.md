## 1. Fakeable Matter Service Tree

- [ ] 1.1 Implement `/mnt/matter` controller, commissioning, device, action,
  status, result, events, and `ctl` semantics against a deterministic fake backend.
- [ ] 1.2 Post `/srv/matter`, mount `/mnt/matter`, and verify filtered-handle,
  access-right, multi-write commit, and event-offset behavior.
- [ ] 1.3 Add non-Apple tests proving no `Matter.framework` dependency outside the
  macOS adapter.

## 2. Apple Controller And Persistence

- [ ] 2.1 Connect `MTRDeviceController` behind the adapter with no framework types
  in Kernel, Agent Execution Engine, Tools, or portable domain code.
- [ ] 2.2 Implement protected controller/fabric storage and restart reopening.
- [ ] 2.3 Implement commissioning request commit, start/cancel/retry `ctl`, status,
  result, and events.

## 3. Direct-Light Operations

- [ ] 3.1 List the commissioned direct light and expose safe metadata/readiness.
- [ ] 3.2 Implement current On/Off reads with explicit unavailable state.
- [ ] 3.3 Implement whole-document On/Off writes, per-node serialization, action
  result records, events, and optional observed-state read-back.
- [ ] 3.4 Reject bridges, bridged endpoints, raw clusters, and excluded high-risk
  device types.

## 4. Debug And Physical Verification

- [ ] 4.1 Add a spike-only CLI or developer UI that uses only canonical Matter
  Service files.
- [ ] 4.2 Manually commission one real direct Matter light and retain environment
  notes plus result files for commissioning, restart, list, read, On, and Off.
- [ ] 4.3 Record concrete blockers without broadening the spike to vendor cloud,
  Apple Home, or bridge support.

## 5. Verification And Archive Readiness

- [ ] 5.1 Run fake-backend, persistence, file-contract, rights, invalid-write, and
  non-Apple build tests.
- [ ] 5.2 Run strict validation for this change and the full OpenSpec tree.
- [ ] 5.3 Decide in a separate proposal whether to productize UI, `/bin` Tools,
  additional device types, or governance.
- [ ] 5.4 After merge, sync `macos-matter-controller-spike` into canonical specs
  before archiving.
