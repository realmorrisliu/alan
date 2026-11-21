# Tasks

## 1. Streaming, usage and ephemeral invocation
- [x] 1.1 Stream assistant text and thinking incrementally and remove the generation-read yield loop; verify tape reconciliation, guardrails and cancellation with focused engine tests.
- [x] 1.2 Publish the usage snapshot through `machine/ui/usage`; verify backward-compatible readers and restart behavior.
- [x] 1.3 Add ephemeral invocation to Host boot and the CLI; verify no root-rollout read or write and rejection with explicit recovery.

## 2. Providers
- [x] 2.1 Deliver `magpie` with live model discovery, optional empty key and stable session affinity; verify with wire mocks.
- [x] 2.2 Fix OpenAI Responses/Codex cache keys, continuation, compaction, WebSocket fallback and OAuth refresh; verify with wire mocks.
- [x] 2.3 Fix Vertex AI endpoint, dual authentication, thought signatures, streaming and multimodal input; verify with wire mocks.
- [x] 2.4 Complete OpenRouter catalog, image input and cost fields; demote Anthropic from first-party defaults while keeping its adapter and adding prompt caching.

## 3. Terminal and CLI
- [x] 3.1 Fix the verified TUI/CLI defects listed in the proposal with a regression test each.
- [x] 3.2 Add real-PTY end-to-end tests for startup, modal exit, idle Esc, Ctrl+C, resize and CJK text.
- [x] 3.3 Verify the four-provider selection in `alan connection`, secret entry without echo and connection testing.

## 4. Documentation and OpenSpec hygiene
- [x] 4.1 Record ADR-0061 and make guides, ADR-0054/0056 and canonical wording terminal-neutral.
- [x] 4.2 Sync and archive `embed-alan-runtime` and `unify-agent-command-input`; archive the six retired desktop-era plans; freeze cognitive and input-routing work as deferred research.
- [ ] 4.3 Run quality, strict OpenSpec with the CI-pinned validator, independent review and current-head CI; report limits before user merge.
- [ ] 4.4 After user merge, sync these deltas into canonical specs and archive.
