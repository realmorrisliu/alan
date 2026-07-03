> **Reconciled 2026-07-02 against shipped code.** The minimal Generation slice
> landed as `alan-llmfs` in PR #578 (per the llmfs-enters-the-core decision:
> Generation only, everything else deferred so the core does not absorb a
> quota service). Shipped: clone-via-open, multi-write `data` commit-on-clunk,
> retained `events` stream with terminal records, `ctl` abort, `status`
> transitions. Still open: `/srv/llm` posting (tree is currently mounted
> directly), provider introspection (§3.1), connection-profile reflection
> (§3.3), directory reaping (§4.4), versioned wire DTO (§5), metering (§6).
>
> **§5 (wire DTO) + §3.1 (capability introspection) are now on the critical
> path of `refactor-engine-namespace-native` Slice A**: the engine's provider
> projection (`detect_provider` / `capabilities()` / `project_messages()`) must
> move behind the neutral request document (ADR-0024 D2) before `LlmClient` can
> leave the engine. They are no longer optional polish.

## 1. Prerequisites

- [x] 1.1 aP protocol available (`define-plan9-kernel-substrate §5`): clone-via-
  open, retained byte/offset streams, three-phase errors (dial/commit/mid).
  Shipped as `alan-ap` (#573).

## 2. Crate skeleton

- [x] 2.1 Add `alan-llmfs` depending on `alan-ap` and `alan-llm`; it is a file
  server, not a backend, and `alan-kernel` does not depend on it. Shipped in
  #578 as `crates/llmfs`.
- [x] 2.2 Post a handle under `/srv` (`/srv/llm`) and serve the tree at `/mnt/llm`.
  Done 2026-07-02: root namespace assembly now posts the llmfs transport as the
  `llm` handle in `alan-kernel::SrvFs`, mounts `/srv` as the rendezvous tree,
  looks the handle back up, and mounts that same tree at `/mnt/llm`. Regression
  coverage reads the final namespace through `MountFs` + `alan-shell`: `/srv`
  lists `llm`, `/srv/llm` reads as the handle identity rather than a state
  directory, and `/mnt/llm` exposes the llmfs `providers`/`connections` tree.

## 3. Provider and Connection surfaces

- [x] 3.1 Serve `/mnt/llm/providers/<provider>` introspect-only (models, capabilities,
  status) from `alan-llm` adapters; not callable.
  Done 2026-07-02: `alan-llmfs` exposes read-only
  `providers/<provider>/models`, `providers/<provider>/capabilities`, and
  `providers/<provider>/status` for the known provider families. `models` now
  includes a bundled provider model catalog with default model, source, family,
  context window, and reasoning support metadata; `status` includes explicit
  `callable: false` and catalog availability. Regression coverage proves
  `providers/openai_responses/clone` is not walkable, so Generations remain
  connection-only.
- [x] 3.2 Serve `/mnt/llm/connections/<connection>` callable endpoints from connection
  profiles (provider + model + credential); credentials resolved from the secret
  store, never agent-visible plaintext.
  Done 2026-07-02: llmfs now supports profile-aware connection registration via
  `ConnectionProfile { provider, model, credential_ref }` while keeping
  plaintext credentials outside the namespace. `/connections/<connection>`
  lists `clone`, `provider`, `profile`, `capabilities`, and Generation
  directories; `profile` is a JSON document with provider/model/credential_ref
  metadata and no `credential`/`api_key` plaintext fields. Generations still
  start only through the callable Connection `clone`.
- [x] 3.3 Reflect connection-profile add/remove as endpoint appear/disappear.
  Done 2026-07-02: llmfs now exposes `unregister_connection`; connection
  profile add/remove updates the `connections/` listing qid and removes the
  endpoint and its Generations. Regression coverage proves a registered profile
  appears under `/connections`, then disappears and becomes unwalkable after
  unregister.

## 4. Generation as a connection directory

- [x] 4.1 Implement clone-via-open: `open clone` allocates
  `/mnt/llm/connections/<connection>/<n>/` with `data`, `events`, `ctl`, `status`.
- [x] 4.2 Accept the request across multiple `data` writes; commit on `data`
  clunk (no start command); reject a truncated/malformed document at commit.
  Drive `alan-llm` streaming.
- [x] 4.3 Stream typed events to `events` (retained, offset-resumable); `status`
  exposes progress, tokens, cost.
  Done 2026-07-02: `status` is now a versioned JSON file containing lifecycle
  status, progress terminality, latest provider token usage, and a cost object
  (`metered: false` until §6.1 supplies real metering). The event stream remains
  retained and offset-resumable; when a stream chunk carries usage metadata,
  llmfs records it and bumps the status qid version. Regression coverage asserts
  `status` exposes progress/tokens/cost and that stat length matches the
  readable status document.
- [x] 4.4 Implement `ctl` abort; reap finished directories per retention policy.
  Done 2026-07-02: retained `ctl` abort behavior is covered by lifecycle tests,
  and llmfs now applies a per-Connection count-based retention policy on
  clone-open, reaping the oldest terminal Generation directories while keeping
  open/running Generations and the newest terminal post-mortems reachable.
  Regression coverage proves the reaped Generation disappears from both the
  connection listing and subsequent walks.

## 5. Wire DTO and mapping

- [x] 5.1 Define versioned request DTO and stream-event DTO owned by `alan-llmfs`.
- [x] 5.2 Map DTO ↔ `alan-llm` `GenerationRequest` / `StreamChunk`.
- [x] 5.3 Add tests that an `alan-llm` internal change does not move the wire
  format unless the DTO version changes.

## 6. Metering and errors

- [x] 6.1 Enforce cost/metering/rate-limiting in `alan-llmfs` per Connection.
  Done 2026-07-02: Connections now expose a `meter` file with per-Connection
  limits and cumulative `generation_starts`, `total_tokens`, and
  `total_cost_microusd` counters. Clone-open reserves against the Connection's
  generation limit before allocating a Generation; exhaustion fails without
  creating a directory. Provider usage chunks update both the Generation status
  token fields and the Connection meter. Pricing remains zero until a model
  price catalog is introduced, so cost is metered structurally without non-zero
  charge calculation.
- [x] 6.2 Dial-time failures → `open` error code; mid-generation failures →
  terminal error record in `events`.
  Done 2026-07-02: rate-limit exhaustion is enforced as a dial-time
  `open clone` `NoAccess` error, while startup failures, early stream close, and
  provider `stream_error` finish reasons leave the Generation terminal and
  append an `error` record to `events`. Regression coverage includes
  `connection_generation_limit_is_enforced_at_clone_open`,
  `a_startup_failure_is_terminal`,
  `an_early_closed_stream_ends_with_a_terminal_error`, and
  `a_stream_error_finish_reason_is_terminal_error_not_done`.

## 7. Verification

- [x] 7.1 Integration test: open a Connection, write a request, read a streamed
  response end-to-end (mock provider via `alan-llm` `mock` feature).
  Done 2026-07-02: existing `crates/llmfs/tests/generation.rs`
  `writing_the_request_streams_tokens_to_events` covers the mock-backed
  end-to-end path: open `connections/default/clone`, write the request to
  `data`, commit on clunk, and drain `events` until the streamed response and
  terminal `done` record are observed. Re-run directly with
  `cargo test -p alan-llmfs writing_the_request_streams_tokens_to_events -- --nocapture`.
- [x] 7.2 Run `just verify`.
  Done 2026-07-02: `just verify` passed after the llmfs metering/rate-limit
  slice, including `cargo fmt --all`, workspace clippy with `-D warnings`,
  `cargo test --workspace`, doctests, and the explicit `alan` smoke suite.
- [x] 7.3 Run `openspec validate add-llm-file-server --strict`.
