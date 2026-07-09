## Why

ADR-0026 D1 records aP network transparency as the next Ring 3 North Star slice
after content-addressed knowledge and routefs. The in-process aP contract is now
usable enough that we can add the first import/export transport without changing
clients or turning distributed agents into an RPC mesh.

## What Changes

- Add a small aP wire transport that serializes aP operations and responses over
  an async byte stream.
- Provide an exported-server loop that serves any `alan_ap::FileServer` through
  the wire protocol.
- Provide an imported-client adapter that implements `alan_ap::FileServer` while
  forwarding operations to the exported remote tree.
- Preserve local semantics: clients use the same walk/open/read/write/clunk API
  whether the tree is local or imported.
- Keep this slice transport-focused; discovery, auth, encryption, multi-host
  topology, and distributed-agent product UX remain later work.

## Capabilities

### New Capabilities

- `ap-wire-transport`: Defines import/export of aP file trees over a byte
  transport while preserving normal aP operation semantics for clients.

### Modified Capabilities

- None.

## Impact

- Extends the `alan-ap` transport layer; `alan-kernel` remains
  dependency-isolated and transport-agnostic.
- Adds focused integration tests using in-memory async streams and existing aP
  file servers.
- Updates workspace wiring and OpenSpec coverage for the network-transparency
  slice referenced by ADR-0026 and ADR-0027.
