## Context

After workspace removal, the CLI still owns an in-process Kernel/runtime and
terminates it with the renderer. The existing aP wire framing and generic
FileServer export/import already provide the transport seam needed for a
dedicated local system Host.

## Goals / Non-Goals

**Goals:**

- One Alan OS authority per user/device/install channel.
- Keep system execution alive when CLI or macOS clients exit.
- Export a ready namespace through native aP over a protected local endpoint.
- Make Host boot composition a deep module instead of CLI/engine plumbing.

**Non-Goals:**

- Service Manager, dynamic boot units, remote transport, macOS rendering, or
  Process restoration after Host restart.
- HTTP, WebSocket, Session, relay, or app-owned product fallback paths.

## Decisions

### 1. Dedicated Host process is the only product owner

Platform per-user process management enforces one stable and one dev instance.
CLI and apps attach. A separate explicit ephemeral executable/config exists for
tests only. Embedding one instance per renderer was rejected because it creates
competing `/proc`, `/srv`, and `/agent/root` authorities.

### 2. Host exposes only attachment and whole-system lifecycle

The Host owns Kernel lifetime, boot identity, System Store adapters, temporary
fixed boot composition, readiness, and shutdown. It does not expose engine
handles, PIDs, or typed management calls. The later Service Manager takes all
internal boot ownership and the temporary composition is deleted.

### 3. Local attachment is aP over Unix domain socket

Use existing aP frames and export/import loops. Each channel has a socket in a
platform runtime directory with current-user permissions and peer UID checks.
Each connection owns fids, not Session identity. Disconnect clunks fids only.

### 4. Boot identity guards Process References

Each Host boot publishes a fresh boot ID. A Process Reference is boot ID plus
PID; clients reject it on mismatch. Host restart restores durable stores but
creates a new Process table and Root Agent.

### 5. Ready means stable paths are readable

The Host accepts product attachments only after the Standard Namespace,
required fixed services, and `/agent/root` are readable. Partial boot fails as
a whole.

## Risks / Trade-offs

- [Socket process races] → Let the platform singleton owner create the endpoint
  atomically; clients retry boot/attach rather than unlinking foreign sockets.
- [Fixed composition becomes permanent] → Make the Service Manager change a
  hard successor and guard the temporary builder for deletion.
- [Long blocking reads consume connection capacity] → Preserve concurrent aP
  request handling and test multiple stream readers.
- [Stable/dev cross-attachment] → Include channel identity in endpoint,
  System Store, diagnostics, and client validation.

## Migration Plan

1. Extract Host boot composition behind a small lifecycle interface.
2. Add boot ID, readiness tree, wire root export, and channel endpoint.
3. Add dedicated process packaging and platform singleton lifecycle.
4. Convert `alan` into boot/attach + Shell client.
5. Delete the linked renderer-owned runtime product path.

## Open Questions

None. Internal service ownership transfers in
`implement-minimal-service-manager`.
