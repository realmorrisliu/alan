# Guardian shadow configuration slice — 2026-10-23

This implements evidence-only dedicated Connection and policy selection toward
tasks 2.1 and 3.1. Active authorization still uses the admitted main Connection and
built-in policy. Tasks 2.1–2.2 and reviewer qualification remain open. This branch
is not merged or installed behavior.

## Explicit interface

The candidate CLI accepts `--guardian-shadow <profile>` on bare `alan`, optionally
with `--guardian-policy /absolute/path/policy.md`. This is distinct from
`--shadow-evaluator`, which evaluates input routing. No project file, Agent
Definition, recovered observation or saved default activates reviewer comparison.
Omitting the flag performs no candidate generation. Child launch does not inherit
the Root-only selection.

The Host reads the explicit UTF-8 policy once, rejects empty content or more than
64 KiB, and passes an owned snapshot through Service Manager. Without a policy
flag, the candidate uses the built-in policy. Connection Service captures the
explicit generation-capable profile through its existing immutable callable
boundary, retaining its credential reference and Host-owned credential resolution.
It changes neither the selected main profile nor the default. Evaluation-only
Connections are not supported by this generation-based comparison slice.

## Execution and evidence

Only an existing reviewer-routed action is compared. Deterministic denials,
always-human requests and already-authorized ordinary actions keep their existing
paths. Both reviewers receive the same frozen untrusted transcript and normalized
action; their authoritative policies remain separate system prompts, without
Tools. A candidate allow, deny or malformed response never replaces the main
decision. Candidate outage does not grant rights or invoke an alternate profile.

Machine-owned `guardian_shadow_v1` events acknowledge a start before candidate
admission and a finish after settlement. Identity includes the task and Tool call,
normalized action, full model-request and policy digests, the main callable identity when available, and
the exact candidate profile revision. Missing main identity remains unknown.
Recovered starts and completed observations do not automatically retry the
candidate; changed identity is rejected. These observations are comparison
evidence, never permission or Tool replay authority.

Candidate retries and cleanup share the original task's time and model ledger.
They also have the existing aggregate review deadline and four-attempt bound.
Budget exhaustion, cancellation and unconfirmed cleanup preserve their stop paths
and incomplete evidence. Comparison therefore consumes real task allowance and
can pause the task. Qualification must account for that overhead. Existing main
review continuation across those pauses still needs the owning 2.2/2.4 acceptance;
candidate deduplication does not establish whole-runtime no-repeat qualification.

Finished observations retain the two assessments, candidate evaluation/retirement
latency and available final-response token usage. This latency excludes boot and
start-record acknowledgement; final-response usage is not aggregate retry usage.
Monetary cost remains explicitly unknown. Failed evidence acknowledgement blocks
further shadow admission instead of silently losing or retrying a paid comparison.

## Verification and remaining gates

Deterministic namespace and Service Manager tests verify opposite main/candidate
decisions, unchanged real Tool effects and main selection, an inactive default,
shared accounting, policy isolation, original-frame recovery and failed durable
acknowledgement. CLI tests cover explicit selection and bounded policy snapshots.
The three-crate regression passed 1,862 tests with three existing ignored tests.
Private logs are under `~/Library/Caches/Alan/guardian-shadow/20261023/`.

These fixtures prove wiring and isolation, not real-model judgment quality. Still
required: frozen labeled corpus and thresholds; bounded real Connection runs;
independent reviewer comparison; configurable active selection and aggregate
candidate/main fallback; stale grant/profile/policy invalidation; native acceptance;
quality, current-head review/CI and user merge. No candidate is eligible for active
authorization yet. Formal daily-use qualification remains zero tasks / zero days.
