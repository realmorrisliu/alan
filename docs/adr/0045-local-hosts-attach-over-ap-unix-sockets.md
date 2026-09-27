# Local hosts attach over aP Unix sockets

Status: accepted for aP transport and fid semantics; endpoint ownership revised
by [ADR-0056](0056-bare-alan-attaches-to-root-agent.md) on 2026-09-27.

An Alan OS instance exports the Standard Namespace through the existing aP wire
protocol on a Unix domain socket under its runtime directory. Each foreground
invocation owns its endpoint; `ALAN_INSTANCE_RUNTIME_DIR` selects that exact
endpoint when set, and only one invocation may own it. The install channel
selects persistent configuration and stores, not a singleton live runtime.
Platform runtime directories and peer UID checks protect discovery and access;
the System Store does not own the endpoint. Each connection has independent fids
over the same instance authority, disconnect only clunks those fids, and clients
reconnect by walking stable paths and resuming from caller-held offsets. Local
attachment introduces no HTTP, WebSocket, Session token, health endpoint, or
server-side attachment identity.
