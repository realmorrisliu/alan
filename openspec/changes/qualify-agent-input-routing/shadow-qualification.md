# Shadow qualification v1 — 2026-10-07

Status: reviewed and frozen pre-measurement gates and labeled corpus. No candidate has been measured,
the finite-choice boundary is locally delivered in `623440ca`, and automatic
execution is disabled. A direct TypeSafe adapter capability probe has passed; no normal evaluator
profile has yet been published or used for routing qualification. The user
activated this bounded next phase after local lifecycle/model/UI acceptance;
predecessor PR merge and delivered Connection capability remain entry gates for
integrated qualification and canonical sync.

## Scope and labels

Use the existing read-only project task's input boundary as the evaluation point.
Classify the original input as command, Agent or ambiguous. `command` means the
original bytes are already intended shell input; natural-language requests to
run commands remain Agent work. Do not translate a natural-language instruction
into a new command string. Explicit `!`/`:` and pending responses bypass evaluation. Pending response data
containing either prefix remains response data, so response consumption precedes
route parsing.
The frozen corpus includes these bypasses as separate orchestration checks.

`shadow-corpus.v1.json` contains 54 hand-labeled cases: literal shell syntax,
Chinese/English requests, discussion, quotes, negation, injection, ambiguity and
explicit/pending-response precedence. It is a bounded regression corpus, not a
statistical claim about all user input. Run each case three times through both
interactive and redirected admission (324 observations). Preserve every attempt;
do not select the best repeat, discard timeout results or exclude failed requests
from latency/cost. Review labels before any candidate measurement. Changing labels,
budgets or adapter/configuration invalidates the comparison and requires a new
version plus rerun of all baselines and candidate.

## Numeric gates

The machine-readable budgets are `shadow-budgets.v1.json`.

- False execution classifications: 0 among all non-command labeled cases.
  Known discussion-to-command errors block activation regardless of aggregate accuracy.
- Overall routing accuracy >=95%; recall for each non-bypass class >=90%.
  Bypass decisions must be exact, invoke no evaluator and preserve original bytes.
- Successful valid evaluations >=98%. Malformed/unavailable/timeout/cancelled
  evaluations never select command; cancellation starts no fallback dispatch.
- End-to-end added routing time: p50 <=300 ms, p95 <=1000 ms, measured with a
  monotonic clock from admission to validated result/fallback, including failures.
  This bounds delay added before useful Agent work; fixture timings are not evidence.
- Mean measured/verified-priced cost <=1000 micro-USD ($0.001) per evaluated input.
  Missing usage, price provenance or billing information is unknown, never zero.
  llmfs unknown cost fields are null; they are not billing evidence.
- Classification-caused Tool spawns, command runs and project writes: exactly 0.
  Controlled negative fixtures must prove this at dispatch and filesystem boundaries;
  a model-generated claim of no effects is not evidence. Shadow collection starts
  no fallback dispatch; cancellation is recorded as cancelled, never Agent dispatch.

Compare (1) existing deterministic prefix/pending-response routing, (2) the
existing generation-only Connection used as an advice-only classifier with no
Tools, and (3) typed evaluation through the mounted Connection. Record correct
routing, latency and cost separately. The all-Agent fallback's zero routing cost
is not a successful command classifier. Candidate p95 latency and mean cost must
not exceed the generation-only baseline, and one must improve by at least 20%.
Unavailable baselines or cost evidence mean qualification is incomplete.

## Evidence and activation

Pin corpus/budget SHA-256 before collection, source commit, schema version,
Connection/profile publication and model/configuration (no secrets). Correlate
case ID, surface, repeat, original-input digest, operation ID, typed outcome,
validated route, elapsed time, usage and cost provenance. Keep runtime evidence
in the owning System Store; exported qualification reports may live in ignored
local build artifacts. The scorer never executes case text.

Passing this corpus is necessary but insufficient: require real-provider evidence,
interruption/recovery fixtures, both input surfaces, current-head review/CI and
an explicit user activation decision. No production hook or config defaults are
changed by this planning/scoring slice.

## Scoring exported observations

Run `python3 scripts/harness/score_routing_shadow.py --self-test` for the pure
scorer regression. Pass an exported JSON document instead of `--self-test` to
score measurements. The envelope has `version: 1`, corpus/budget SHA-256 and
`records`. Each record contains case_id, surface, zero-based repeat,
input_sha256, route, outcome, elapsed_ms, cost_microusd (nullable), cost_source,
evaluator_calls, fallback_calls and effects. The collector, not model output, owns timings,
cost provenance and dispatch observations. Missing, duplicate or changed-input
observations are rejected rather than silently removed from denominators.

`numeric_pass` grades this bounded corpus only. The report always says
`activation_authorized: false` and lists outstanding evidence/decision gates.
Self-test synthetic measurements cannot prove real-provider behavior or benefit.
`shadow-freeze.v1.json` pins pre-measurement corpus and budgets for review;
no candidate results have influenced these artifacts.

## Preparation review

Spec and Standards review passed after correcting cancellation/fallback accounting,
rejecting duplicate JSON keys and adding pending-response/prefix cross-cases.
The reviewed production scorer and corpus/budgets are pinned by patch SHA-256
`c30aa3037252a859d38512083cfd3e8150b52cdaa392bade52811d24139a785d`.
The scorer self-test passed; this is fixture validation only. Strict validation
passes, with an informational archive dependency on the predecessor's unmerged
renderer requirement. This change is not archive-ready.

Tasks 1.2, 2.1, 2.3 and 2.4 remain open: the generic typed operation boundary
now exists in `623440ca`, but its Machine consumer, real-provider publication and
baseline measurements remain unfinished. The TypeSafe transport passed local HTTP fixtures and one authenticated capability
probe (see the cognitive change's typesafe-adapter-entry.md). That does not complete
ordinary Connection profile publication or routing qualification. The separate activation decision remains pending.
