# Testing Strategy: Prevent Client-Server Protocol Drift

> Status: current testing guide. Normative Rust test placement and reusable
> harness behavior live in OpenSpec:
> [`rust-test-placement-contract`](../openspec/specs/rust-test-placement-contract/spec.md)
> and
> [`runtime-harness-contract`](../openspec/specs/runtime-harness-contract/spec.md).

## Goals

alan's event stream is a cross-client contract. The testing strategy aims to ensure:

1. Events emitted by the server are always consumable and renderable by clients.
2. Protocol evolution has explicit compatibility boundaries instead of implicit assumptions.
3. CI catches protocol changes before clients drift out of sync.

---

## Current Protocol Baseline (2026-02)

Source of truth: `alan_protocol::Event`, `alan_protocol::Op`, and `alan_runtime::tape`.

- Events: `turn_started`, `turn_completed`, `text_delta`, `thinking_delta`, `tool_call_started`, `tool_call_completed`, `yield`, `error`
- Ops: `turn`, `input`, `resume`, `interrupt`, `register_dynamic_tools`, `compact`, `rollback`
- Tape model: `Message::User/Assistant/Tool/System/Context`, `ToolRequest`, `ToolResponse`

Reference implementation:

- `crates/protocol/src/event.rs`
- `crates/protocol/src/op.rs`
- `crates/runtime/src/tape.rs`

---

## Test Layers

### 1) Contract Tests

File: `crates/alan/tests/event_contract_test.rs`

Purpose:

- Verify minimal user-visible client contracts (for example, text responses must produce displayable content).
- Verify tool-call events are recognizable by frontend clients.
- Verify fallback behavior when the model returns empty content.

This layer does not test runtime internals; it tests whether users get the expected visible behavior.

### 2) Event Sequence Validation

File: `crates/alan/tests/event_sequence_validation_test.rs`

Purpose:

- Verify relative ordering and required event presence under key scenarios.
- Cover text responses, tool-call flows, and empty-response fallbacks.

Current example pattern:

```rust
let expected_sequence = vec![
    EventPattern::new("turn_started").required(),
    EventPattern::new("thinking_delta").required(),
    EventPattern::new("text_delta").required(),
    EventPattern::new("turn_completed").required(),
];
```

### 3) Integration Event-Flow Tests

File: `crates/alan/tests/integration_event_flow_test.rs`

Purpose:

- Verify base `EventEnvelope` properties (such as timestamp monotonicity).
- Verify transport-level stability for streaming events.

### 4) Live Provider Protocol Harness

Files:

- `crates/llm/tests/live_provider_harness.rs`
- `scripts/live-provider-harness.sh`
- `docs/live_provider_harness.md`

Purpose:

- Verify real upstream protocol paths against live providers.
- Catch auth drift, upstream request-contract changes, and stateful continuation
  regressions that mocked tests cannot see.

Scope:

- non-streaming generation,
- streaming completion,
- Responses-style continuation for providers that declare support.

Operational model:

- tests are `#[ignore]`,
- the runner requires `ALAN_LIVE_PROVIDER_TESTS=1`,
- provider credentials are injected explicitly through harness-specific
  environment variables.

### 5) Live Runtime Smoke

Files:

- `crates/alan/tests/live_runtime_smoke_test.rs`
- `scripts/live-runtime-smoke.sh`
- `docs/live_runtime_smoke.md`

Purpose:

- Verify the real runtime turn path against a live provider.
- Catch runtime-level request-shaping regressions that provider-only adapter
  tests cannot see.

Current scope:

- managed `chatgpt` runtime startup,
- real turn submission,
- event-stream completion to `turn_completed`,
- cross-session stable-memory persistence and recall,
- cross-session handoff continuity after restart,
- text output and no-provider-error assertions.

Operational model:

- tests are `#[ignore]`,
- the runner requires `ALAN_LIVE_PROVIDER_TESTS=1`,
- provider credentials are injected explicitly through runtime-smoke
  environment variables.

---

## Protocol Sharing and Compatibility

Rust protocol crates are the source of truth for shipped daemon clients.
The Rust TUI consumes `alan_protocol` events directly and uses the daemon API
contract helpers for route construction. Client compatibility checks live in
Rust contract tests instead of generated TypeScript files.

---

## Recommended Change Workflow

### When adding or changing an Event

1. Update `crates/protocol/src/event.rs` first.
2. Update contract tests: `crates/alan/tests/event_contract_test.rs`.
3. Update sequence tests: `crates/alan/tests/event_sequence_validation_test.rs`.
4. Update client handlers (Rust TUI / Apple).
5. Run tests: `cargo test --workspace`.

### When adding or changing an Op

1. Update `crates/protocol/src/op.rs` first.
2. Update daemon routing/submission tests.
3. Update client submission payloads.
4. Run full tests.

### When changing a provider adapter or provider capability declaration

1. Update adapter/unit tests in `crates/llm/src/*`.
2. Update runtime/provider branching tests if capability behavior changed.
3. Compile and run:
   `cargo test -p alan-llm`
   `cargo test -p alan-runtime`
4. For risky wire-level changes, run the live provider harness for the affected
   providers before merging.
5. For runtime-visible provider changes, also run the live runtime smoke for
   the affected providers before merging.

---

## CI Recommendations

```yaml
- name: Run contract tests
  run: cargo test -p alan --test event_contract_test

- name: Run event sequence tests
  run: cargo test -p alan --test event_sequence_validation_test

- name: Run Rust TUI tests
  run: cargo test -p alan-terminal-ui
```

---

## Summary

To avoid protocol mismatch, follow "contract first, compatibility second":

1. Protocol source of truth: `alan_protocol + alan_runtime::tape`
2. Behavior source of truth: contract tests + sequence tests
3. Frontend sync mechanism: Rust protocol types + CI verification
