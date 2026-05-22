## 1. Spike Boundaries And Project Setup

- [ ] 1.1 Confirm the macOS deployment target and Apple `Matter.framework` availability for the Alan for macOS target.
- [ ] 1.2 Add a macOS-only Matter controller service boundary under the Apple client without importing Matter types into `alan-runtime`.
- [ ] 1.3 Define a narrow spike/debug invocation path for setup payload intake, device list/read, and OnOff commands.
- [ ] 1.4 Add compile-time gates so non-macOS runtime crates and non-Apple builds do not require Apple Matter APIs.

## 2. Controller State And Commissioning

- [ ] 2.1 Create or load a local `MTRDeviceController` through Apple `Matter.framework`.
- [ ] 2.2 Implement spike storage for controller/fabric state with restricted local access and clear failure reporting.
- [ ] 2.3 Accept a setup payload for one directly Matter-capable light in pairing or multi-admin pairing mode.
- [ ] 2.4 Commission the light into Alan's own Matter fabric and record structured success or failure details.
- [ ] 2.5 Verify app restart reloads controller state and can address the commissioned light without repeating commissioning.

## 3. Light Registry And Low-Risk Operations

- [ ] 3.1 Project the commissioned light into a minimal local device registry with stable node identity and human-readable debug metadata.
- [ ] 3.2 Implement list commissioned devices for the spike path.
- [ ] 3.3 Implement read OnOff state with a structured current-state or unavailable result.
- [ ] 3.4 Implement set OnOff state for the commissioned light only.
- [ ] 3.5 Record each physical write with target, requested action, status, timestamp, and error details when available.

## 4. Safety Constraints

- [ ] 4.1 Reject unsupported target categories such as HomeKit-only devices, bridges, bridged endpoints, locks, cameras, security systems, appliances, and high-power devices.
- [ ] 4.2 Keep raw Matter endpoint, cluster, and command invocation out of LLM-visible surfaces.
- [ ] 4.3 Document that final `home.*` tools, device naming, governance risk levels, and skill instructions belong to the follow-up product change.

## 5. Verification

- [ ] 5.1 Add fake-service or adapter tests for setup payload validation, state-load failure, list/read/write result shaping, and unsupported target rejection where practical.
- [ ] 5.2 Run focused Apple build checks covering the Matter-gated code path.
- [ ] 5.3 Manually commission a real directly Matter-capable light and capture verification evidence for commissioning, restart persistence, list, read OnOff, set On, and set Off.
- [ ] 5.4 Run `openspec validate spike-macos-matter-controller --strict`.
- [ ] 5.5 Run `openspec validate --all --strict`.

## 6. Archive Readiness

- [ ] 6.1 Summarize spike findings, including framework limitations, storage decision gaps, and real-device failure modes.
- [ ] 6.2 Decide whether the follow-up product change should proceed as `add-home-control-tools`.
- [ ] 6.3 After implementation merges, sync accepted requirements into `openspec/specs/` before archiving.
