> **Sequencing gate (2026-07-08).** §0 must pass before §2–§4 and §6 start.
> The load-bearing unproven assumption of this whole product is that aP file
> reads over relay on mobile-grade networks give acceptable latency for
> streamed output and interrupt round-trips (ADR-0028 risk list). If the spike
> fails, the product shape changes — build the cloud/account plumbing after
> the physics are confirmed, not before. The OS entry contract itself is owned
> by `define-remote-access-service`; this change owns only the product plane.

## 0. De-Risk Spike (gate)

- [ ] 0.1 Spike: attach to a remote namespace over aP through the existing
  environment-configured relay (`ALAN_RELAY_URL` dev/operator mode, dev-grade
  auth), tail a live generation stream, and issue interrupts — no accounts, no
  enrollment, no cloud endpoints.
- [ ] 0.2 Measure and record: stream first-byte latency, sustained token-delta
  latency, interrupt round-trip, and reattach-after-drop behavior under
  simulated mobile network conditions (latency injection / connection churn).
- [ ] 0.3 Go/no-go note in this change: either confirm the aP-over-relay path
  meets interactive expectations, or record what transport work (framing,
  batching, prefetch) must land first — before any §2–§4 build-out.

## 1. Tracking And Product Boundary

- [x] 1.1 Create the GitHub tracking issue for `add-alan-anywhere-mvp` and link
  it to this OpenSpec change.
- [x] 1.2 Close or mark issue `#9` as superseded by the Alan Anywhere MVP issue.
- [x] 1.3 Keep issue `#75` open as the iOS task-manager IA follow-up and link
  it to the Alan Anywhere MVP issue.
- [ ] 1.4 Decide the MVP account provider and document any remaining
  auth-provider assumptions before implementation starts.

## 2. Account And Device Model

- [ ] 2.1 Define account-owned device records for Mac and iPhone, including
  `device_id`, display name, platform, owner account, enrollment state, last
  seen, and revocation state.
- [ ] 2.2 Add Mac device enrollment after Alan for macOS account login with
  Keychain-backed device credentials.
- [ ] 2.3 Add iPhone device enrollment after mobile account login with platform
  secure credential storage.
- [ ] 2.4 Add device revocation handling that prevents future remote access and
  terminates or rejects active state-changing requests.

## 3. Cloud Presence And Relay Broker

- [ ] 3.1 Add Cloud service endpoints for listing account-owned devices and
  their online/offline/connectable status.
- [ ] 3.2 Add short-lived remote entry ticket issuance scoped to account, client
  device, target Mac device, entry intent, expiry, and revocation state.
- [ ] 3.3 Add Mac presence heartbeats that publish online/stale/offline status
  without moving runtime authority from the Mac.
- [ ] 3.4 Add audit records for enrollment, connection, revocation, and
  state-changing remote control attempts.

## 4. Mac Remote Availability

- [ ] 4.1 Start product-managed outbound relay connection automatically when
  Alan for macOS is signed in and Alan Anywhere is enabled.
- [ ] 4.2 Keep environment-configured relay mode as development/operator
  compatibility while making account/device relay the Mac default path.
- [ ] 4.3 Publish Mac-authored device availability status, including online,
  stale, offline, and remote-entry connectability state.
- [ ] 4.4 Ensure Mac remains the final authority for local namespace, process,
  app, governance, tool execution, and stream ordering.

## 5. Realtime Remote Streams And File Surfaces

- [ ] 5.1 Define remote stream delivery over aP/file-surface reads without
  adding HTTP, WebSocket, daemon-session, or compatibility gateway endpoints.
- [ ] 5.2 Implement relay byte transport without making relay the author of
  stream offsets, record order, process state, or runtime state.
- [ ] 5.3 Replace daemon `events/read` and `reconnect_snapshot` recovery in the
  target path with lease reattachment, stream offsets, and ordinary file reads.
- [ ] 5.4 Add tests or guardrails that prevent Alan Anywhere from extending
  daemon endpoint metadata, HTTP routes, WebSocket routes, or remote session APIs.

## 6. iPhone Alan Anywhere Experience

- [ ] 6.1 Replace manual daemon/relay connection with account device discovery
  and OS-native remote entry.
- [ ] 6.2 Show online Macs using product-facing labels, not relay node IDs or
  tunnel URLs.
- [ ] 6.3 Allow iPhone to enter the selected Mac through Remote Access Service
  and then interact through the returned remote namespace.
- [ ] 6.4 Ensure iPhone reattaches with its live lease, resumes remote stream
  reads from saved offsets, and rebuilds state by rereading current files when
  gaps are reported.
- [ ] 6.5 Keep transport, relay, routing, and ticket diagnostics behind
  explicit debug surfaces.

## 7. Security Verification

- [ ] 7.1 Add tests for account/device/target/entry-intent checks during remote
  entry ticket validation.
- [ ] 7.2 Add tests showing Cloud cannot read namespace files, spawn processes,
  or advance runtime state.
- [ ] 7.3 Add tests for revoked Mac and iPhone devices denying new remote entry
  and terminating or rejecting active remote lineages.
- [ ] 7.4 Add tests or harness scenarios for dropped mobile connections, lease
  reattach, stream-offset recovery, gap recovery, and no duplicate execution.

## 8. Documentation And OpenSpec Closure

- [ ] 8.1 Update product and maintainer docs to describe Alan Anywhere as
  device-to-device Alan continuation.
- [ ] 8.2 Update remote attachment architecture/security docs to reference
  Alan Anywhere as the product layer above direct/relay transport.
- [ ] 8.3 Run focused Rust/Swift tests for changed aP, Remote Access, relay-byte
  transport, Alan for macOS, and iPhone surfaces.
- [ ] 8.4 Run `openspec validate add-alan-anywhere-mvp --type change --strict --json`.
- [ ] 8.5 Run `openspec validate --all --strict --json`.
- [ ] 8.6 Run `git diff --check`.
- [ ] 8.7 Before archive, sync accepted delta requirements into `openspec/specs/`.
- [ ] 8.8 Archive the OpenSpec change after implementation is merged.
