## 1. Wire Frames

- [x] 1.1 Add public aP wire frame types for request frames and
  `Result<Response, ErrorCode>` response frames.
  Done 2026-07-02: added `WireRequestFrame`, `WireResponseFrame`, and
  `WireError` to `alan-ap`.
- [x] 1.2 Implement newline-delimited JSON encode/decode helpers over async byte
  streams.
  Done 2026-07-02: added request/response encode, decode, read, and write
  helpers for newline-delimited JSON frames.
- [x] 1.3 Add focused tests proving every request and response result survives
  byte framing.
  Done 2026-07-02: `crates/ap/tests/wire_transport.rs` covers every aP request
  and both successful/error response results.

## 2. Export Loop

- [x] 2.1 Add an export loop that reads framed requests, dispatches them through
  the same `FileServer` methods as `InProcessTransport`, and writes framed
  response results.
  Done 2026-07-02: added `export_file_server`, which dispatches through
  `InProcessTransport` and writes a framed operation result.
- [x] 2.2 Preserve typed `ErrorCode` values returned by the exported server.
  Done 2026-07-02: response frames carry typed `ErrorCode` values; tests verify
  `NoAccess` survives an exported/imported write failure.

## 3. Import Adapter

- [x] 3.1 Add an imported-tree adapter that implements `FileServer` and forwards
  each method over the wire transport.
  Done 2026-07-02: added `WireTransportClient` and `ImportedFileServer` with
  method-by-method forwarding to the exported remote tree.
- [x] 3.2 Serialize v1 calls to one in-flight request per connection and document
  that multiplexing is a later transport upgrade.
  Done 2026-07-02: `ImportedFileServer` guards one connection with a mutex, and
  the API docs/design document call out multiplexing as later work.
- [x] 3.3 Preserve blocking stream-read semantics through the imported adapter.
  Done 2026-07-02: added a remote `Stream` test proving imported reads stay
  pending until the exported server's stream produces data.

## 4. Boundaries And Verification

- [x] 4.1 Verify `alan-kernel` stays transport-agnostic and depends only on
  `alan-ap` among Alan crates.
  Done 2026-07-02: `cargo test -p alan-kernel --test dependency_boundary --
  --nocapture` passed.
- [x] 4.2 Run focused aP transport tests.
  Done 2026-07-02: `cargo test -p alan-ap -- --nocapture` and
  `cargo clippy -p alan-ap --all-targets --all-features -- -D warnings` passed.
- [x] 4.3 Run `just verify`.
  Done 2026-07-02: `just verify` passed, including workspace fmt, clippy,
  tests, doctests, and the `alan` smoke suite.
- [x] 4.4 Run `openspec validate add-ap-wire-transport --strict`.
  Done 2026-07-02: strict OpenSpec validation passed for
  `add-ap-wire-transport`.

## 5. PR Hygiene

- [x] 5.1 Commit this slice separately from routefs.
  Done 2026-07-03: committed as
  `feat(ap): add wire import export transport` on `feat/northstar-ap-wire`.
- [x] 5.2 Open a stacked draft PR on top of `feat/northstar-routefs`.
  Done 2026-07-03: opened #591 on top of `feat/northstar-routefs`, then marked
  it ready for review to match the current PR workflow.
