# TypeSafe Connection profile delivery — 2026-10-07

This slice follows the authenticated adapter probe in `df35138e` and publishes
TypeSafe evaluation through the existing profile, Host credential and llmfs owners.
It adds no alternate credential store, router or Process lifecycle.

## Contract

- `typesafe` profiles require a pinned `jev-X.Y.Z` model and a Host-owned secret
  reference. Endpoint/header/generation-control overrides are unavailable. Config
  transports resolved credentials only in serde-skipped runtime fields; saved
  Agent configuration and Connection metadata never contain the new secret.
- LlmClient, Host LiveSecretProvider and ConnectionLlmProvider preserve the
  underlying callable's generation/evaluation capabilities and forward evaluation.
  Host resolves the current credential at every request; an already-captured
  client fails after secret revocation, while in-flight requests retain their
  original authorization.
- Connection publication records pinned model/provider/credential-reference
  provenance and keeps evaluation independent from generation binding. Provider
  introspection does not advertise generation features for TypeSafe.
- Evaluation-only profiles cannot become the generation default, a selected
  Process generation profile or a captured generation binding. Adding the first
  evaluation profile does not silently make it the generation default.
  Both public select and file-control selection share one State check; replacing
  a selected profile with an evaluation-only profile retires that stale selection.
- Existing generation providers retain their current profile and request paths.
  No Machine consumer, effect dispatch or automatic classification is activated.

## Evidence

`cargo check --workspace` passed. The full service-manager and os-host library
suites passed, including evaluation-only default/selection rejection and
Host-wrapper credential revocation/secret non-serialization. The opt-in
`live_typesafe_profile_through_mounted_connection` test passed through the real
ProductLlmClientFactory, ConnectionService and llmfs file operations with an
isolated test System Store and Host credential store. It returned Selected(rust),
provider typesafe, model jev-1.13.0, 394 input tokens and 44 output tokens. No user
channel profile/default was changed. The test's ephemeral stores were removed.

The user-provided ignored `.env` credential was passed only to the opted-in child
process; no secret value was logged or committed. This single fixed-language
capability case is not one of the frozen routing observations and proves neither
routing accuracy nor latency/cost qualification.

Logs under `~/Library/Caches/Alan/`: `typesafe-profile-check.log`,
`typesafe-profile-boundary-tests.log`, `typesafe-live-profile.log`.
Full quality passed; 67 focused engine configuration tests also passed. Spec review
found a public-select and same-ID replacement bypass; the shared State selection
check and expanded regression close both. Spec and Standards re-review passed on implementation patch SHA-256
`44f9915693cb100b82c80e3a5e4d3a79a5836d4b8a572e36e23dd867d36f5d2c`.
Final adapter/CLI library tests passed (208 LLM, 20 CLI, 4 llmfs local tests);
the mandatory final commit hook remains required before delivery.

## Remaining work

Machine-owned shadow admission, durable outcomes and interrupted recovery;
interactive and redirected surface collection; all 324 frozen observations and
baselines; verified billing and latency gates; current-head CI after stack merge.
Automatic execution remains disabled and still needs an explicit user decision.
