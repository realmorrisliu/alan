# Local hosts attach over aP Unix sockets

Status: accepted

Alan OS Host exports the Standard Namespace root through the existing aP wire
protocol on a per-user, per-install-channel Unix domain socket. Platform runtime
directories and peer UID checks protect discovery and access; the System Store
does not own the endpoint. Each connection has independent fids over the same
system authority, disconnect only clunks those fids, and clients reconnect by
walking stable paths and resuming from caller-held offsets. Local attachment
introduces no HTTP, WebSocket, Session token, health endpoint, or server-side
attachment identity.
