# TypeSafe finite-choice adapter entry — 2026-10-07

The next bounded slice implements the real TypeSafe HTTP protocol behind the
existing provider operation contract. It does not publish a new Connection profile
kind, supply credentials, enable a Machine consumer or qualify live service behavior.
The public Rust constructor is for explicit qualification; ordinary provider factory
and operator CLI publication remain gated on a successful authenticated live probe.

Protocol sources checked on this date: [HTTP API](https://docs.typesafe.ai/api),
[Choice](https://docs.typesafe.ai/primitives/choice), and
[models](https://docs.typesafe.ai/models). The protocol accepts state and typed
questions, returns a choice/distribution/confidence and token usage, and identifies
the model used. Those transport facts are not evidence of accuracy or latency here.

## Adapter decisions

- Accept a credential from the caller's owning Host store and an explicit numeric
  `jev-X.Y.Z` model version. Read no environment or other provider's auth store.
- Use only the documented HTTPS endpoint. Disable redirects and transport retries;
  allow one request per evaluation. Bound the entire HTTP operation to 30 seconds
  and its response to 1 MiB; llmfs may enforce a shorter caller deadline.
- Preserve original input as state. Map caller candidates to distinct wire IDs,
  with a separate abstention option that cannot collide with caller IDs. Return
  caller IDs or NoMatch, never an executable body. Fixed question instructions and
  candidate descriptions are part of the adapter configuration pinned for all runs.
- Require exact returned model identity, one expected Choice answer, the complete
  finite distribution, finite bounded confidence/probabilities, normalized mass
  within 0.001, selected maximum within 0.000001, and nonnegative nonoverflowing
  usage. Reject duplicate answer/distribution keys. Unknown billing remains unknown.
- Fail without retry/fallback on authentication, rate limit, redirect, malformed
  result, timeout or oversized body. Do not expose credentials, state or response
  bodies through errors.
- Generation methods have an unavailable default; this evaluator explicitly
  advertises generation=false; its generation capability object is null instead
  of inheriting family defaults such as streaming_text=true. llmfs rejects generation allocation before quota
  reservation, so an evaluator need not implement fake chat/streaming methods.

## Live probe and remaining publication gate

The user supplied `TYPESAFE_API_KEY` in the ignored root `.env`. The opt-in test
loaded only that named value into its child environment; it did not source the
file as shell code, print the key or load unrelated credentials. The provider
adapter itself still reads no environment.

One authenticated `live_typesafe_choice_probe` passed with pinned `jev-1.13.0`,
Selected(rust), 402 input tokens and 44 output tokens. Monotonic elapsed time was
1659 ms. This was a fixed code-language capability probe, not a labeled routing
corpus case or a p50/p95 measurement; no routing prompts, labels or budgets were
changed in response. The 324-observation qualification remains unperformed.

Ordinary Connection profile publication and Machine-owned shadow consumption are
still unfinished. The successful authenticated probe satisfies the adapter's
entry gate for implementing that publication; it does not by itself register a
profile, select a generation binding or enable routing.

## Validation and review

- `cargo test -p alan-llm -p alan-llmfs`: 208 LLM unit tests and 53 llmfs tests,
  plus LLM integration tests, passed. The new opt-in live probe separately passed.
- Adapter HTTP fixtures verify original input and caller-ID mapping, generation
  unavailability, malformed/oversized responses, no retries or redirects, safe
  errors and release of a silent HTTP response body on cancellation.
- Full `just quality` passed. The final staged snapshot must pass the mandatory
  commit hook before delivery.
- Spec and Standards review passed after correcting the contradictory default
  generation feature projection. Frozen implementation patch SHA-256:
  `fe5fc7a2fcfacce0276d0ac67d82f9adbd1d0b979686db1ca0a4a27d219779ca`.
  The opt-in live test and this evidence were added after that implementation review;
  they do not alter adapter behavior.

Logs: `~/Library/Caches/Alan/typesafe-final-tests.log`,
`typesafe-live-probe.log`, `typesafe-adapter-quality.log`, and
`typesafe-adapter-commit.log`. Costs have not been scored or treated as zero.
