# Remote Attachment Model (Consolidated)

Status: Accepted. Extends [ADR-0024](0024-plan9-kernel-model.md) (kernel model)
and [ADR-0026](0026-plan9-application-ideas-for-agents.md) D1 (9P network
transparency) to cross-host access. This record consolidates the full remote
attachment decision set — originally drafted as forty single-decision records —
into one anchor, following the ADR-0024 precedent. The normative contract with
scenarios lives in OpenSpec (`remote-access-service`); this ADR records the
decisions and their rationale.

## Context

Alan Anywhere (continue your Alan OS from another owned device) forces the
question of what "remote access" means in the Plan 9 model. The daemon-era
answer — session APIs, relay-proxied HTTP routes, reconnect snapshots — is the
wrong architectural center: clients integrate against session APIs instead of
Alan OS files. With the aP wire transport landed, remote access can be what the
model says it should be: importing a real process's namespace across hosts.

## Decisions

### D1. Remote attachment targets namespaces, not daemon APIs

Remote features attach to a remote Alan OS through mountable file trees and
executable surfaces over aP, never through daemon session APIs. In the current
single-user phase, the default authority is a **User Namespace Attachment** —
the signed-in user's full namespace, not a session/workspace/app projection.
Finer-grained delegated authority is explicit later work, not smuggled in as a
compatibility token matrix.

### D2. Remote clients enter through a Remote Entry Process

Cross-host access creates a real `Process` on the destination host before any
client interaction; the client works through that process's namespace,
descriptors, and executables. Authority, mounts, cwd, and audit belong to the
process boundary, not to an out-of-band transport session. The entry process
starts as a **general shell entry, not an Agent Process** — agents are spawned
beneath it only when the caller chooses. A fresh attach creates a new entry
process by default and lands in a **neutral shell** (no implicit app/workspace
resurrection); resuming an existing lineage requires explicit reattach intent.

### D3. Fresh entry clones the Login Namespace Template

A fresh Remote Entry Process clones the owning user's `Login Namespace
Template` rather than sharing the namespace of an already-running shell or
agent — per-process namespace semantics hold for remote entry too.

### D4. Entry is owned by a dedicated, OS-generic Remote Access Service

A Service-Manager-started file server owns remote entry: it terminates
transports, authenticates the remote principal, manages leases, and creates or
reattaches entry processes. Handle at `/srv/remote-access`; optional local
inspection at `/mnt/remote-access`. It is OS-generic — Alan Anywhere, direct
attach, LAN attach, and tests all enter through it — and it never owns runtime
truth.

### D5. Bootstrap, then hand off

The service exports a minimal aP **Remote Bootstrap Tree**; there is no
separate login RPC. Fresh entry (`new/`) and lease reattach (browsable
`leases/`, minimal neutral metadata only) are distinct file-surface views.
Fresh entry allocates a bootstrap instance via clone-via-open (`new/clone`)
with a standard file set (`request`, `status`, `handoff`, `ctl`). `handoff` is
a **blocking, one-shot capability delivery**: `status` observes progress;
`handoff` waits for readiness and yields the entry process's namespace root
itself — not an endpoint bundle. A successful handoff **consumes** the
bootstrap instance. Pre-handoff failures are cleaned up by the bootstrap
instance (including partially created entry processes); post-handoff lifecycle
belongs to the process tree and lease. Steady-state operations never proxy
through the entry service.

### D6. Leases bound continuity; recovery never re-drives execution

The entry process survives transport loss within a bounded **Remote Attachment
Lease**, reattachable by the same remote client identity. The lease starts
atomically when the root handle becomes handoff-ready (a transport failure
between readiness and receipt is recovered by explicit reattach, not by
garbage-collecting a valid process). Reconnect recovery = lease reattach +
saved stream offsets + ordinary file reads. Execution is never re-driven
because a client reconnected; there is no daemon reconnect snapshot as source
of truth.

### D7. Revocation terminates the lineage

Device revocation or lease expiry tears down the whole remote-attached process
lineage by default. No local takeover escape hatch is defined in this phase;
any future exception is a separate explicit design.

### D8. Remote context is files, lineage-local, inherited

Remote-only attachment facts (device identity, transport mode, lease state,
reattachment history) are exposed as a small **Remote Context Tree** mounted at
`/mnt/remote` — inside the attached lineage only, never host-global — and
inherited by descendant processes through normal namespace mechanics. Service
discovery (`/srv/remote-access`) and per-lineage provenance (`/mnt/remote`)
use distinct paths because they are different kinds of objects.

### D9. Entry runs under the user Credential; device identity is provenance

In the single-user phase the entry process runs under the target user's normal
`Credential`; the remote device is tracked separately as `Remote Device
Identity` for provenance, audit, and lease control. Device-as-principal would
be a premature parallel authority model.

### D10. The product control plane stays outside Alan OS

Account login, device directory, presence, relay brokerage, and **Remote Entry
Ticket** issuance belong to the product control plane (Alan Anywhere), outside
Alan OS. Tickets authorize *entry attempts* — account + client device + target
device + intent + expiry — never daemon-session operations. Product discovery
stops at device availability; work discovery happens after attach by reading
the returned namespace. Alan Anywhere is a product experience over this model,
not a remote-control API.

### D11. Transports vary; attach semantics do not. No compatibility gateway

Direct, relay, LAN, and future brokers are byte-delivery choices only — they
vary in reachability, encryption, latency, ticketing, and reconnect behavior,
never in attach semantics. There is **no HTTP/WS/daemon-session compatibility
gateway**: old daemon-backed remote APIs are deletion targets, not migration
surfaces. Even a thin gateway would preserve the wrong architectural center.

### D12. Imported remote trees are ordinary mounts under /mnt/peer

A tree reached over aP import is an ordinary mountable file server. Default
prefix `/mnt/peer/<remote-id>`, where `<remote-id>` names the **exported entry
tree** (one device may host several lineages; device provenance lives in the
Remote Context Tree, not the mount path). Mutating operations and executable
effects execute on the remote host, attributed to the exporting lineage.
Cross-host cooperation composes through files and processes — no agent-to-agent
RPC protocol. Visibility is directional: importing a remote tree does not
implicitly expose the local namespace back; reverse sharing uses the same
ordinary export-and-mount primitives.

### D13. Post-attach control is namespace operations

Messages, interrupts, request answers, and app actions are reads and writes on
the returned tree (`io/`, `requests/<id>/response`, app files,
`/proc/<pid>/ctl`). Transport delivers bytes; Alan OS files define the
semantics.

## Risks / Trade-offs

- **Full-namespace default (D1) on the highest-risk client.** A phone on a
  mobile network, with relay in the path, holds the user's whole world;
  revocation (D7) and short-lived tickets (D10) are the levers. The
  `remote-access-service` spec must state this threat model explicitly and
  keep scoped attachment as named follow-up work — this is a deliberate
  single-user-phase trade, not an oversight.
- **Lease lifetime tuning (D6).** Too short breaks mobile churn; too long
  leaves orphan authority. Bounded, configurable, revocable.
- **Relay latency for interactive streams (D11).** The model assumes aP file
  reads over relay are acceptable for streaming and interrupts on mobile
  networks — unproven; must be spiked before product build-out
  (`add-alan-anywhere-mvp` sequencing).

## References

- Contract: `remote-access-service` (OpenSpec,
  `define-remote-access-service`); product: `alan-anywhere`
  (`add-alan-anywhere-mvp`).
- Daemon-era remote control: `remote-control-contract` — frozen as legacy by
  this model (D11); see its delta under `define-remote-access-service`.
- Vocabulary: `CONTEXT.md` Remote* entries (names and one-liners only; this
  ADR and the OpenSpec contract carry the semantics).
