## Context

Alan Anywhere is the product experience over Alan OS remote attachment. The
user should experience Alan as available across their own devices: an iPhone can
enter the user's Mac-hosted Alan OS and work through the returned namespace. The
system can still use outbound relay byte transport under the hood, but the
product must not ask the user to understand tunnels, daemon URLs, VPNs, public
IPs, SSH, router configuration, port forwarding, or relay nodes.

The canonical architecture is `Remote Product Control Plane` plus
`Remote Access Service`. Product account/device infrastructure handles owned
device discovery, presence, relay brokerage, and short-lived entry tickets.
Alan OS handles entry: `Remote Access Service` authenticates the ticket,
creates or reattaches a `Remote Entry Process`, and hands off that process
namespace root. After handoff, clients read files, tail streams, write `ctl`,
answer request files, and spawn executables. There is no HTTP/WebSocket
compatibility gateway.

The OS entry model itself — bootstrap tree, one-shot handoff, entry process,
leases, revocation, `/mnt/remote`, `/mnt/peer` — is anchored by ADR-0028 and
owned as a contract by `define-remote-access-service`
(`remote-access-service` capability). This change designs only the product
plane above it and MUST NOT restate or fork those OS semantics.

## Goals / Non-Goals

**Goals:**

- Make signed-in Alan for macOS automatically become remotely reachable from
  the user's own iPhone without inbound network exposure.
- Let the iPhone app discover the user's online Macs and enter the selected
  device through `Remote Access Service`.
- Preserve Mac-authoritative namespace state, process execution, agent
  execution, tool execution, governance, and stream ordering.
- Support realtime stream reads, process control writes, request responses,
  executable invocation, and reconnect recovery over the remote path.
- Bind remote access to account identity, device identity, short-lived remote
  entry tickets, revocation, and encrypted transport.
- Keep direct/relay as byte-delivery transport modes while replacing
  operator-facing setup with product-managed enrollment and presence.

**Non-Goals:**

- Remote desktop, screen sharing, terminal screen mirroring, or arbitrary
  desktop control.
- P2P hole punching, LAN discovery, router configuration, user-managed VPNs,
  SSH setup, or Cloudflare-style user-managed tunnels.
- Multi-user collaboration or shared workspaces.
- Complex enterprise networking, MDM, organization policy, or delegated
  account administration.
- Moving agent execution, workspace reads, tool execution, or governance
  authority to Alan Cloud.
- Building a full push-notification system as a blocker for foreground iPhone
  realtime use.

## Decisions

1. Use Alan Cloud as an account/device directory plus relay broker, not as a
   runtime or OS authority.

   Alan Cloud owns user authentication, device enrollment, presence, relay
   routing, short-lived token issuance, revocation, and audit metadata. It does
   not execute tools, read namespace files, decide governance outcomes, author
   stream records, spawn processes, or advance runtime state.

   Alternative considered: expose a daemon endpoint directly from the Mac. That
   conflicts with zero configuration and would require public IP, router, or
   tunnel knowledge for many users.

2. Treat the Mac as the authoritative execution device.

   The Mac owns Alan OS namespace state, process state, stream ordering, lease
   state, tool execution, and policy decisions. Relay and iPhone bytes are
   remote access to that host, not remote execution contexts.

   Alternative considered: proxy workspace state through Alan Cloud and allow
   cloud-side execution for continuity. That would break the security and
   product requirement that user tasks execute on the user's own device.

3. Add product-managed device enrollment above the existing relay tunnel.

   Signed-in Alan for macOS creates or refreshes a stable local device identity,
   stores device credentials in Keychain, requests short-lived remote entry
   tickets, and starts outbound relay byte transport automatically when needed.
   The user sees device availability, not relay configuration.

   Alternative considered: continue using `ALAN_RELAY_URL`,
   `ALAN_RELAY_NODE_ID`, and `ALAN_RELAY_NODE_TOKEN` as the primary path.
   Those remain useful for development/operator modes but cannot be the MVP
   product path.

4. Model the pre-attachment surface as device availability.

   The iPhone app should list the user's online Macs and connectability. It
   should not display relay node IDs, daemon base URLs, tunnel status, raw
   routing headers, session catalogs, or workspace catalogs unless a debug
   surface is explicitly opened. After entry, work discovery happens by reading
   the returned remote namespace.

   Alternative considered: reuse the existing relay node list directly in the
   mobile UI. That leaks implementation details and makes the product feel like
   remote infrastructure instead of workspace continuation.

5. Provide realtime stream delivery with offset recovery.

   Alan Anywhere needs realtime streamed output while preserving
   reconnect-safe recovery. The transport should carry reads from remote stream
   files, and clients recover through lease reattachment, saved stream offsets,
   and ordinary file reads after reconnect or gap detection.

   Alternative considered: use high-frequency polling only. Polling is a useful
   fallback, but it does not satisfy the product expectation of streamed output
   and responsive interrupt/approval flows.

6. Use device-bound, short-lived remote entry tickets.

   Account login proves user identity. Device enrollment binds a Mac/iPhone app
   installation to that account. Each remote entry attempt uses a short-lived
   `Remote Entry Ticket` scoped to the account, client device, target Mac,
   entry intent, expiry, and revocation state. In the current single-user phase,
   the default ticket is not a workspace-, session-, or operation-scope matrix.

   Alternative considered: one long-lived bearer token per node. That matches
   the current technical MVP but is too hard to revoke safely and too weak for
   an account-driven consumer product.

## Risks / Trade-offs

- Relay compromise exposes routing metadata -> Keep relay non-authoritative,
  issue short-lived remote entry tickets, minimize stored metadata, and require
  Mac-side validation before entry.
- Mac goes offline during iPhone use -> Keep process and namespace state on the
  Mac, mark the device offline/stale in presence, and avoid pretending
  execution can continue elsewhere.
- Realtime relay stream drops under mobile network churn -> Require cursor
  recovery through lease reattachment, stream offsets, and file reads; never
  re-drive execution due to reconnect.
- Device list becomes noisy or confusing -> Show only user-owned devices with
  product labels, last activity, connectability, and clear offline state; keep
  relay diagnostics debug-only.
- Workspace path disclosure on mobile -> Avoid pre-attachment workspace
  catalogs; after attachment, workspace visibility comes from the remote
  namespace and descriptor rights.
- Existing environment-configured relay paths diverge from product-managed
  Alan Anywhere -> Keep environment configuration as development/operator
  compatibility for non-Anywhere local development only; Alan Anywhere must not
  use a daemon compatibility gateway.

## Migration Plan

1. Add the OpenSpec requirements and GitHub tracking issue for Alan Anywhere
   MVP; mark the old architecture issue as superseded by this product contract.
2. Introduce account/device data models and local device identity storage.
3. Implement Alan for macOS device enrollment and automatic outbound relay connection
   behind a feature flag or development cloud endpoint.
4. Add Cloud device/presence endpoints and short-lived remote entry ticket
   issuance.
5. Add realtime remote stream delivery over aP/file-surface reads with lease
   reattachment and stream-offset recovery.
6. Update iPhone to use account device discovery and Remote Access Service entry
   instead of manual daemon connection.
7. Harden revocation, audit, and offline/reconnect behavior before making the
   feature default.
8. Keep rollback simple: Alan for macOS can stop advertising remote availability and
   iPhone can hide Alan Anywhere entry during development.

## Open Questions

- Which Alan account provider is authoritative for MVP login: Alan-hosted auth,
  Sign in with Apple, GitHub, or an existing managed account surface?
- Should remote stream bytes be end-to-end encrypted between iPhone and Mac in
  MVP, or is transport encryption plus host-authoritative execution acceptable
  for the first product slice?
- What is the minimum device availability metadata that iPhone may display
  before attachment?
- Should APNs pending-approval notifications be included in this MVP or tracked
  as a follow-up after foreground realtime Alan Anywhere works?
