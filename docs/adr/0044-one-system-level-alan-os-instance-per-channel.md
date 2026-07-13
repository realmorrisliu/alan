# One system-level Alan OS instance per channel

Status: accepted

Each user and device has at most one active Alan OS instance per install
channel, owned by a dedicated Alan OS Host process. Alan for macOS, Alan CLI,
and future renderer hosts attach through aP and do not own system lifetime, so
their exit cannot terminate Agent Processes. Platform per-user process
management enforces singleton ownership without restoring daemon-era Session,
HTTP, WebSocket, or relay contracts. Explicit ephemeral Hosts remain available
only for development and tests.
