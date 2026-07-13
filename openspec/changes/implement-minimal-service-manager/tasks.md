## 1. Implement Boot Units and manager state

- [ ] 1.1 Define the minimal Boot Unit schema and reject scripts, templates, user units, reload, and unknown fields
- [ ] 1.2 Load the system-package-owned `/lib/boot` tree and validate dependency order/cycles
- [ ] 1.3 Implement manager-owned unit status, PID, attempts, errors, degraded state, and `ctl` tree
- [ ] 1.4 Seed units for required File-Server Services, Agent Runtime Service, Local Entry, Host Mount, Connection, and Root Agent

## 2. Make Service Manager the boot owner

- [ ] 2.1 Start Service Manager as the first normal Kernel Process
- [ ] 2.2 Launch all later system Processes from Boot Units with explicit namespaces/descriptors
- [ ] 2.3 Determine readiness from `/proc` liveness plus declared `/srv` handles only
- [ ] 2.4 Invalidate handles and terminate stale launches on exit or publication timeout
- [ ] 2.5 Delete the temporary fixed composition and assert Alan OS Host starts only Service Manager

## 3. Add bounded supervision

- [ ] 3.1 Implement `never`, `on-failure`, and `always` restart decisions
- [ ] 3.2 Implement calibrated bounded exponential backoff, restart budget, and stable reset window
- [ ] 3.3 Fail boot when a required unit exhausts budget before readiness
- [ ] 3.4 Mark a running system degraded after required-unit exhaustion and support explicit `ctl` retry
- [ ] 3.5 Supervise Root Agent with `always`, publish `/agent/root`, and test replacement without PID continuity

## 4. Implement Local Entry Service

- [ ] 4.1 Define the Login Namespace Template and Alan OS single-user credential
- [ ] 4.2 Implement clone/status/process/handoff/ctl entry files
- [ ] 4.3 Create `/bin/alan-shell` as an ordinary Shell Process and hand off its namespace to authorized local clients
- [ ] 4.4 Test Shell child parentage, disconnect/drain, and independent Agent Process survival

## 5. Implement Host Mount Service

- [ ] 5.1 Implement request, grant, status, audit, projection, and revocation files
- [ ] 5.2 Connect CLI/native Host adapters without exposing raw Host paths in Alan OS
- [ ] 5.3 Project approved hostfs exports into requesting live namespaces and pass grants explicitly to children
- [ ] 5.4 Derive native sandbox roots from the same grants and invalidate future authority on revocation
- [ ] 5.5 Test unknown grant IDs, read-only/write access, revocation, and non-inheritance

## 6. Implement Connection Service

- [ ] 6.1 Implement channel-scoped profile/default/selection/status/ctl files in System Store
- [ ] 6.2 Publish callable LLM connection trees and pass selections through Process launch references
- [ ] 6.3 Implement native login/credential request and opaque-reference response files
- [ ] 6.4 Connect CLI Host adapters and prove secrets never enter namespace or System Store
- [ ] 6.5 Remove remaining Host-owned connection profile/default authority

## 7. Verify the system boot contract

- [ ] 7.1 Add unit parser, dependency, readiness, timeout, crash-loop, degraded, and retry tests
- [ ] 7.2 Add full boot smoke proving `/proc`, `/srv`, `/agent/root`, Shell entry, Host Mount, and Connection service behavior
- [ ] 7.3 Add architecture guards rejecting Host-side service supervision and engine boot composition
- [ ] 7.4 Run `just test`, `just check`, `just fmt`, `just lint`, release builds, and strict OpenSpec validation

## 8. Review and archive readiness

- [ ] 8.1 Submit after `extract-system-level-alan-os-host` is merged and archived
- [ ] 8.2 Complete current-HEAD Codex review, zero unresolved threads, green CI, and delayed recheck before merge
- [ ] 8.3 Sync all service deltas into canonical specs and verify package-management rewrite prerequisites
- [ ] 8.4 Archive only after implementation and canonical sync are merged
