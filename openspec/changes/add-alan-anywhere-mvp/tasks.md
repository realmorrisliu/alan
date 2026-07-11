## 1. Remote Access Prerequisite

- [ ] 1.1 Build a headless local Remote Access Service proof covering ticket
  validation, handoff, entry Process creation, lease reattachment, and
  revocation.
- [ ] 1.2 Verify destination-host credentials, namespace, and policy remain
  authoritative throughout the proof.
- [ ] 1.3 Measure interactive file and stream behavior under mobile-grade loss,
  latency, and network changes.

## 2. Account And Device Model

- [ ] 2.1 Specify Alan account identity, enrolled-device records, device keys,
  display names, creation/revocation timestamps, and audit ownership.
- [ ] 2.2 Implement explicit Mac enrollment and another-device revocation.
- [ ] 2.3 Implement coarse, expiry-bounded availability without publishing
  workspace, app, or Process catalogs.

## 3. Remote Entry Tickets

- [ ] 3.1 Specify a short-lived ticket bound to account, source device,
  destination device, entry intent, nonce, and expiry.
- [ ] 3.2 Implement destination validation, replay rejection, and local-policy
  refusal.
- [ ] 3.3 Record bounded ticket and entry audit evidence without retaining user
  content.

## 4. Transport Spike

- [ ] 4.1 Compare viable encrypted byte-delivery implementations against
  reachability, latency, energy, operational cost, and trust boundaries.
- [ ] 4.2 Record the selected mechanism in a separate implementation OpenSpec
  change; keep this product contract transport-neutral.

## 5. Mac And iPhone Product Surfaces

- [ ] 5.1 Add Mac account, enrollment, availability, active-entry, and
  revocation surfaces.
- [ ] 5.2 Add iPhone sign-in, device list, entry progress, terminal/file view,
  pending-request handling, and explicit disconnect.
- [ ] 5.3 Discover workspaces, apps, and Agent Processes only from the handed-off
  namespace.
- [ ] 5.4 Keep identifiers and transport diagnostics behind an explicit debug
  surface.

## 6. Continuity And Security

- [ ] 6.1 Implement lease reattachment plus caller-held stream offsets without
  recreating execution.
- [ ] 6.2 Terminate remote lineage on expiry, device revocation, account
  revocation, or explicit operator action.
- [ ] 6.3 Threat-model account takeover, lost devices, replay, ticket theft,
  destination impersonation, transport compromise, and confused-deputy paths.
- [ ] 6.4 Add multi-device consistency, churn, revocation, and audit tests.

## 7. Validation And Archive Readiness

- [ ] 7.1 Run strict OpenSpec validation for this change and its dependency.
- [ ] 7.2 Run focused Rust/Swift integration, security, and network-condition
  tests for the selected implementation slices.
- [ ] 7.3 Complete a real Mac-to-iPhone dogfood pass with product-facing copy and
  diagnostics reviewed separately.
- [ ] 7.4 After merge, synchronize `alan-anywhere` and archive this change.
