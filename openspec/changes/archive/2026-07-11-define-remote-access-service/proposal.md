> **SUPERSEDED.** The accepted file-native `remote-access-service` requirements
> are folded into `remove-daemon-era-contracts`; its old remote-control freeze
> delta is intentionally discarded. Archive this change without synchronizing
> its independent deltas.

## Why

Remote entry is an Alan OS capability, not an Alan Anywhere feature. The
remote-attachment design (ADR-0028) defines a general OS entry model — a
Service-Manager-started file server that authenticates remote principals and
hands off a `Remote Entry Process` namespace root — that Alan Anywhere, direct
attach, LAN attach, and tests all enter through. That contract was drafted
inside the `add-alan-anywhere-mvp` product change and as prose in the
`CONTEXT.md` glossary; a product change must not own an OS contract, and a
glossary must not carry normative semantics that `openspec validate` never
sees.

## What Changes

- Add the `remote-access-service` capability: the OS-level contract for the
  Remote Access Service (`/srv/remote-access`), the Remote Bootstrap Tree
  (clone-via-open fresh entry, browsable lease reattach), one-shot handoff of
  the `Remote Entry Process` root, attachment leases, lineage revocation, the
  `/mnt/remote` context tree, `/mnt/peer/<remote-id>` imports, and the
  no-compatibility-gateway rule.
- Freeze `remote-control-contract` as legacy: the daemon-era remote surface
  (relay HTTP routes, reconnect snapshots, session scopes) is a deletion
  target per ADR-0028 D11 and MUST NOT be extended; durable invariants
  (host-authoritative execution, transports never bypass governance) transfer
  to `remote-access-service`.
- Slim the `CONTEXT.md` Remote* glossary entries to names, one-line meanings,
  and Avoid lists; semantics live in ADR-0028 and this capability.
- `add-alan-anywhere-mvp` is re-scoped to the product plane (accounts, device
  enrollment, presence, tickets, iPhone experience) and consumes this
  capability.

## Capabilities

### New Capabilities

- `remote-access-service`: the OS remote-entry contract (bootstrap, handoff,
  entry process, lease, revocation, remote context, peer imports, gateway
  prohibition).

### Modified Capabilities

- `remote-control-contract`: frozen as legacy (no new requirements, routes,
  scopes, or metadata); reduced to the daemon-era record plus a pointer to
  `remote-access-service` as the successor for all new remote work.

## Impact

- ADR-0028 (Remote Attachment Model, consolidated) anchors the decisions; this
  change carries the validatable contract.
- `add-alan-anywhere-mvp` depends on this capability; its spec keeps only
  product-plane requirements.
- Implementation lands in follow-on changes (a `remote-accessfs` file server
  above the aP wire transport, wired through Service Manager). The relay
  latency spike gating product build-out is owned by `add-alan-anywhere-mvp`.
- No code changes in this change.
