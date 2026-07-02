## Context

ADR-0025 makes `alan-ap` the owner of Alan's file-service protocol and its
transport shape. The crate already has serializable `Request`/`Response` values,
an in-process fast path, and tests proving each operation is wire-shaped. ADR-0026
D1 records the missing Ring 3 capability: importing/exporting aP file trees across
machines so distributed agents mount remote resources instead of calling a new
RPC mesh.

This slice turns the existing wire-shaped contract into a real byte transport
without changing any `FileServer` implementation or kernel API.

## Goals / Non-Goals

**Goals:**

- Carry every existing aP `Request`/`Response` across an async byte stream.
- Export any `FileServer` through a server loop that receives requests and sends
  responses.
- Import a remote tree as a `FileServer` adapter so local clients keep using the
  same walk/open/read/write/stat/create/remove/clunk semantics.
- Preserve typed `ErrorCode` failures across the transport.
- Keep the kernel unaware of the transport.

**Non-Goals:**

- Network discovery, peer identity, auth, encryption, reconnect, or multiplexing.
- Public CLI/daemon commands for remote hosts.
- Cross-host Service Manager policy or distributed-agent product UX.
- Literal 9P compatibility.

## Decisions

1. **Implement the first wire transport inside `alan-ap`.**

   `alan-ap` already owns protocol messages and `InProcessTransport`; putting the
   byte transport beside them keeps protocol ownership coherent and avoids a
   second crate that must mirror every future operation.

   Alternative considered: add `alan-ap-wire` as a separate crate. That keeps
   optional transport dependencies out of `alan-ap`, but the current dependencies
   already include Tokio and Serde, and ADR-0025 names the in-process and future
   wire transports as part of aP.

2. **Use newline-delimited JSON frames for v1.**

   A frame is one JSON object plus `\n`. The format is debuggable, works with
   existing Serde types, and is enough to prove import/export semantics.

   Alternative considered: length-prefixed binary frames. That is more compact
   and handles arbitrary payload bytes without escaping overhead, but it makes
   the first slice less inspectable. We can add a binary codec later without
   changing `Request`/`Response`.

3. **Wrap responses in an envelope that can carry typed errors.**

   `Response` only represents successful operation results. The transport must
   preserve `Result<Response, ErrorCode>`, so the wire response is an explicit
   success/error envelope.

4. **Keep v1 strictly one request in flight per connection.**

   The imported adapter serializes calls with a mutex around the stream. That
   avoids request IDs and response reordering while still proving that a remote
   tree can be used like a local one. Multiplexing can be added later as a
   compatible transport upgrade.

## Risks / Trade-offs

- [Risk] JSON framing is inefficient for high-rate streams. -> Mitigation: this
  is a correctness slice; the protocol messages stay codec-independent.
- [Risk] A single in-flight request can head-of-line block on a blocking stream
  read. -> Mitigation: v1 makes that limitation explicit; later multiplexing can
  add request IDs without changing `FileServer`.
- [Risk] Transport IO failures do not map perfectly onto file-server errors. ->
  Mitigation: v1 maps malformed frames and IO shutdown to `ErrorCode::Io` or
  `ErrorCode::BadRequest` at the adapter boundary, keeping clients in typed aP
  error space.
- [Risk] Adding transport code to `alan-ap` could tempt kernel coupling. ->
  Mitigation: dependency-boundary tests continue to require `alan-kernel` to
  depend only on `alan-ap`, and the kernel does not call transport constructors.
