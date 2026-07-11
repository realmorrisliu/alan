## Context

`remote-access-service` defines how a remote principal receives a Remote Entry
Process through aP file operations, handoff, and an attachment lease. Alan
Anywhere owns the user-facing product above that boundary.

## Goals / Non-Goals

**Goals:**

- one account-backed list of enrolled Macs;
- explicit device authorization and revocation;
- short-lived entry authorization bound to user, source device, destination,
  intent, and expiry;
- destination-host Process and governance authority;
- continuity through leases and stream offsets;
- an iPhone experience that exposes product concepts, not infrastructure.

**Non-Goals:**

- defining a new Alan OS entry primitive;
- choosing the final byte-delivery transport;
- preselecting a workspace, app, or Agent Process before remote entry;
- moving filesystem, Process, Tool, or policy authority into Alan Cloud;
- designing the general Alan for macOS-to-Alan OS attachment.

## Decisions

### 1. Device enrollment is explicit

A Mac becomes remotely available only after account sign-in, device naming,
key establishment, and an explicit enrollment confirmation. Revocation removes
future entry authority and terminates active remote lineage through the Remote
Access Service contract.

### 2. Entry tickets are short-lived and intent-bound

Alan Cloud may issue a ticket only after validating the account, source device,
destination device, requested entry intent, and expiry. The destination host
validates the ticket and remains free to refuse entry under local policy.

### 3. Product discovery happens after entry

Before handoff the iPhone sees only account-owned device availability and entry
progress. After handoff it discovers the granted namespace through files. This
prevents cloud catalogs from becoming a second source of workspace, app, or
Process truth.

### 4. Continuity is lease reattachment

A temporary network loss does not recreate execution. The client reattaches an
active lease, rereads current files, and resumes streams from caller-held
offsets. Expiry or revocation terminates the remote lineage.

### 5. Cloud coordination has bounded authority

Alan Cloud may own accounts, enrolled-device metadata, coarse availability,
ticket issuance, and transport coordination. It cannot author destination
files, Process state, Agent Machine state, policy decisions, or Tool results.

### 6. Transport is replaceable

The product contract requires authenticated encrypted byte delivery and useful
interactive latency, but does not select direct connectivity, a broker, LAN,
or another mechanism. A separate implementation change must measure and choose
the transport.

## Risks / Trade-offs

- Full user-namespace entry is powerful: product copy must state authority and
  show active device/lease state plainly.
- Mobile churn may make interaction feel unreliable: verify offset continuity
  and lease reattachment under realistic networks before launch.
- Cloud compromise may expose account and routing metadata: minimize retained
  metadata and keep content/end-state authority on the destination host.
- Device loss creates urgent revocation needs: make revocation immediate,
  auditable, and available from another signed-in device.

## Migration Plan

1. Validate the accepted Remote Access Service contract with a local headless
   client.
2. Specify account, enrollment, device key, presence, and ticket storage.
3. Run a transport feasibility spike and record the selected mechanism in a
   separate OpenSpec change.
4. Implement Mac enrollment/availability and iPhone device selection.
5. Implement ticket-backed entry, handoff, lease reattachment, and revocation.
6. Run security, network-churn, multi-device, and product UX verification.

## Open Questions

- Which byte-delivery implementation meets the latency and reachability target?
- What presence freshness is honest enough for product-facing availability?
- Which account recovery path can revoke a lost last device safely?
