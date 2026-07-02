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
- [ ] 2.2 Post a handle under `/srv` (`/srv/llm`) and serve the tree at `/mnt/llm`.
  Partial: the tree is served and mounted at `/mnt/llm`; `/srv/llm` posting is
  still open.

## 3. Provider and Connection surfaces

- [ ] 3.1 Serve `/mnt/llm/providers/<provider>` introspect-only (models, capabilities,
  status) from `alan-llm` adapters; not callable.
- [ ] 3.2 Serve `/mnt/llm/connections/<connection>` callable endpoints from connection
  profiles (provider + model + credential); credentials resolved from the secret
  store, never agent-visible plaintext.
- [ ] 3.3 Reflect connection-profile add/remove as endpoint appear/disappear.

## 4. Generation as a connection directory

- [x] 4.1 Implement clone-via-open: `open clone` allocates
  `/mnt/llm/connections/<connection>/<n>/` with `data`, `events`, `ctl`, `status`.
- [x] 4.2 Accept the request across multiple `data` writes; commit on `data`
  clunk (no start command); reject a truncated/malformed document at commit.
  Drive `alan-llm` streaming.
- [ ] 4.3 Stream typed events to `events` (retained, offset-resumable); `status`
  exposes progress, tokens, cost. Partial in #578: retained event streaming and
  terminal records shipped; token/cost exposure deferred with metering (§6).
- [ ] 4.4 Implement `ctl` abort; reap finished directories per retention policy.
  Partial in #578: `ctl` abort shipped (atomic terminal transition, drain-task
  notify); reaping/retention still open.

## 5. Wire DTO and mapping

- [ ] 5.1 Define versioned request DTO and stream-event DTO owned by `alan-llmfs`.
- [ ] 5.2 Map DTO ↔ `alan-llm` `GenerationRequest` / `StreamChunk`.
- [ ] 5.3 Add tests that an `alan-llm` internal change does not move the wire
  format unless the DTO version changes.

## 6. Metering and errors

- [ ] 6.1 Enforce cost/metering/rate-limiting in `alan-llmfs` per Connection.
- [ ] 6.2 Dial-time failures → `open` error code; mid-generation failures →
  terminal error record in `events`.

## 7. Verification

- [ ] 7.1 Integration test: open a Connection, write a request, read a streamed
  response end-to-end (mock provider via `alan-llm` `mock` feature).
- [ ] 7.2 Run `just verify`.
- [ ] 7.3 Run `openspec validate add-llm-file-server --strict`.
