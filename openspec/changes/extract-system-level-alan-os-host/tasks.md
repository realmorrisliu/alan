## 1. Extract the Host module

- [x] 1.1 Add a dedicated Alan OS Host crate/module with boot identity, readiness, attachment, and shutdown interface
- [x] 1.2 Move Kernel, System Store adapters, fixed File-Server assembly, and Root Agent boot out of CLI and Agent Execution Engine
- [x] 1.3 Narrow Agent Execution Engine startup to an assembled Process namespace and explicit descriptors
- [x] 1.4 Add a guard documenting and locating the temporary fixed composition for mandatory deletion by Service Manager

## 2. Add system-level process ownership

- [x] 2.1 Add stable/dev dedicated Host executables and per-user singleton enforcement
- [x] 2.2 Add platform System Store and runtime-endpoint paths isolated by install channel
- [x] 2.3 Publish a fresh boot ID and file-proven readiness state for every Host boot
- [x] 2.4 Reject product attachment until Standard Namespace, required services, and `/agent/root` are readable
- [x] 2.5 Add explicit test-only ephemeral Host selection with no product fallback

## 3. Export local aP attachment

- [x] 3.1 Export the mounted namespace root through the existing aP wire server on a Unix domain socket
- [x] 3.2 Import the endpoint as an ordinary FileServer in CLI clients
- [x] 3.3 Enforce socket ownership/permissions and peer UID validation before namespace access
- [x] 3.4 Preserve independent fid lifecycle, concurrent blocking reads, typed errors, and commit-on-clunk across the socket
- [x] 3.5 Add disconnect/reconnect tests proving Processes continue and streams resume from caller offsets

## 4. Convert Alan CLI entry

- [x] 4.1 Make `alan` discover, request platform start, and attach only the matching channel Host
- [x] 4.2 Enter Alan Shell rather than booting the linked file-backed Agent runtime
- [x] 4.3 Remove renderer-owned product startup and shutdown of Alan OS
- [x] 4.4 Add clear Host Command Plane status/start/stop diagnostics without duplicating namespace commands

## 5. Verify lifecycle and isolation

- [x] 5.1 Test stable/dev Hosts, endpoints, boot IDs, System Stores, and clients cannot cross-attach
- [x] 5.2 Test Host restart invalidates old Process References and creates no Process restoration
- [x] 5.3 Test CLI exit leaves Host and Agent Processes running
- [x] 5.4 Run workspace tests, release builds, lint/fmt, aP transport tests, binary smoke, and strict OpenSpec validation

## 6. Review and archive readiness

- [x] 6.1 Submit one Host extraction PR after `remove-workspace-runtime-model` is merged and archived
- [ ] 6.2 Complete current-HEAD Codex review, zero unresolved threads, green CI, and delayed recheck before merge
- [ ] 6.3 Sync Host and attachment deltas into canonical specs after implementation merge
- [ ] 6.4 Archive only after canonical sync is merged and `implement-minimal-service-manager` can consume the Host seam
