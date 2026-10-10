# Repeated real-development acceptance matrix — 2026-10-10

The layout, thirty fixture inputs/prompts, bounds and measurement fields are
frozen at five families x two platforms x three repetitions. Runtime readiness
is frozen separately before each Host's generation. Three macOS F1 first attempts
have executed: two PASS and one FAIL for incomplete prospective tool identity.
A fresh linked retry qualifies that failed slot without replacing its first
outcome. The first F2 attempt also executed and remains FAIL after native preflight rejected
the independent binary-check operand. A fresh linked F2 retry and original F2 r2/r3 now pass the complete independent
checks on the repaired candidate. All three macOS F3 compiler-correction tasks
also qualify. Nine slots qualify through eleven real model attempts: seven first
PASS, two first FAIL and twenty-one NOT_RUN. See `native-runs.md`
for retained receipts.
No earlier UI, Sandbox, Tool or CI receipt fills a slot.

## Entry, identity and bounds

- macOS: fresh Alan Shell invocation in Herdr; Linux: fresh invocation in an
  ordinary PTY with actual required enforcing-backend readiness. Initial terminal
  size is 80x24. Narrow/resize/cross-host smoke is additional coverage.
- Effective source revision and production-source tree, native target/binary
  digest, definition/config identity, Connection profile/provider/model/effort,
  platform/kernel, selected git/Rust/PATH and grant IDs/access are required.
- Target selection is gpt-6.1-sol medium, subject to real supported profile
  inventory on both Hosts. Absence remains blocked; no substitute model or copied
  credentials. No automatic routing; use explicit supported input semantics.
- A repetition has a fresh fixture/Process identity. Recovery records every
  continuation Process/boot identity and explicit selected rollout/checkpoint.
  Record memory exposure without clearing user stores or claiming independent
  statistical samples. Fixtures must not expose expected answers/verifier state.
- Default wall limit is ten minutes per attempt, including waits. A time-limit
  change is a pre-run matrix revision, never retrospective. Actual owned writers
  must be cancelled/exited before reuse, and failure attempts are retained.
- Runtime durable files stay with their System Store owners. A unique owned Host
  qualification cache holds receipts/captures with source/effect inventories and
  durable references. No personal credential contents or copied private stores.

## Family checks

| Family | Frozen fixture direction | Required model behavior and independent proof |
| --- | --- | --- |
| F1 small repair | Three small Rust expression defects with immutable failing tests and fresh symbol/input variants | Observe actual RED; model edits only the allowed production file; targeted and adjacent behavior GREEN; immutable tests/config and exact authorized diff checked |
| F2 cross-file | Three small API/caller changes across two or three Rust files | Model updates definition and every declared caller; protected verifier exercises observable behavior; exact allowed files and no unrelated edits |
| F3 failure correction | Three seeded real compiler/test/tool failures; no fabricated diagnostic | Actual Alan Tool result records failure before correction; model inspects its correlated diagnostic and fixes the cause; independent behavior passes without deleting/weakening tests or accepting exit-only success |
| F4 cancel/revoke/recover | Each repetition has an append-only completed-effect ledger and observed delayed writer in a disposable grant | Observe writer live, cancel before scheduled effect, revoke and verify next unauthorized effect blocked; wait past scheduled time; explicitly select durable recovery and reapprove before continuation; completed ledger entry appears once, no late/duplicate write, truthful lifecycle/terminal states |
| F5 long-output follow-up | Repetitions exercise long stdout, long stderr and a long initial diff, with a useful item beyond inline bounds | Inspect retained detail or truthful loss and original Action correlation; model obtains evidence through available supported paths and performs a bounded dependent answer/edit; protected verifier checks the referenced item, source diff and once-only effects; draft/cursor/detail return/scrollback remain usable |

Host fixture setup is recorded separately from model authorship. For a long
initial diff, baseline changes are Host setup and the model's additional diff is
checked against that baseline. No operator may write the solution and label it a
model coding success. If unavailable retained evidence prevents the dependent
check, the task fails or is blocked; a truthful loss notice alone is not success.
Tool literal text such as `server> ready`, comparisons and Markdown quotes remains
content; generated role-prefix regression checks follow ADR-0059.

## Thirty initial slots

| Slot | Platform / Host | Family | Repetition | First outcome | Final qualified evidence |
| --- | --- | --- | --- | --- | --- |
| macos-f1-r1 | macos / Herdr | F1 | 1 | FAIL: missing pre-run exact tool identity | native-runs.md; fresh a2 PASS `71e2e625`; a1 retained |
| macos-f1-r2 | macos / Herdr | F1 | 2 | PASS | native-runs.md; a1 receipt `02cf4ac6` |
| macos-f1-r3 | macos / Herdr | F1 | 3 | PASS | native-runs.md; a1 receipt `9c9a1a6f` |
| macos-f2-r1 | macos / Herdr | F2 | 1 | FAIL: independent binary-check operand rejected | native-runs.md; fresh a2 PASS `c9dc9be0`; a1 `6c0336f1` retained |
| macos-f2-r2 | macos / Herdr | F2 | 2 | PASS | native-runs.md; a1 receipt `e3313a3a` |
| macos-f2-r3 | macos / Herdr | F2 | 3 | PASS | native-runs.md; a1 receipt `42c7ae59` |
| macos-f3-r1 | macos / Herdr | F3 | 1 | PASS | native-runs.md; a1 receipt `41bc46fa` |
| macos-f3-r2 | macos / Herdr | F3 | 2 | PASS | native-runs.md; a1 receipt `a6e6c051` |
| macos-f3-r3 | macos / Herdr | F3 | 3 | PASS | native-runs.md; a1 receipt `0ef06fc9` |
| macos-f4-r1 | macos / Herdr | F4 | 1 | FAIL: external grant revoke UI / wall bound | native-runs.md; a1 `a7d875ea`, a2 `2730d877` FAIL retained; fresh a3 PASS `bd740877` |
| macos-f4-r2 | macos / Herdr | F4 | 2 | NOT_RUN | absent |
| macos-f4-r3 | macos / Herdr | F4 | 3 | NOT_RUN | absent |
| macos-f5-r1 | macos / Herdr | F5 | 1 | NOT_RUN | absent |
| macos-f5-r2 | macos / Herdr | F5 | 2 | NOT_RUN | absent |
| macos-f5-r3 | macos / Herdr | F5 | 3 | NOT_RUN | absent |
| linux-f1-r1 | linux / ordinary PTY | F1 | 1 | NOT_RUN | absent |
| linux-f1-r2 | linux / ordinary PTY | F1 | 2 | NOT_RUN | absent |
| linux-f1-r3 | linux / ordinary PTY | F1 | 3 | NOT_RUN | absent |
| linux-f2-r1 | linux / ordinary PTY | F2 | 1 | NOT_RUN | absent |
| linux-f2-r2 | linux / ordinary PTY | F2 | 2 | NOT_RUN | absent |
| linux-f2-r3 | linux / ordinary PTY | F2 | 3 | NOT_RUN | absent |
| linux-f3-r1 | linux / ordinary PTY | F3 | 1 | NOT_RUN | absent |
| linux-f3-r2 | linux / ordinary PTY | F3 | 2 | NOT_RUN | absent |
| linux-f3-r3 | linux / ordinary PTY | F3 | 3 | NOT_RUN | absent |
| linux-f4-r1 | linux / ordinary PTY | F4 | 1 | NOT_RUN | absent |
| linux-f4-r2 | linux / ordinary PTY | F4 | 2 | NOT_RUN | absent |
| linux-f4-r3 | linux / ordinary PTY | F4 | 3 | NOT_RUN | absent |
| linux-f5-r1 | linux / ordinary PTY | F5 | 1 | NOT_RUN | absent |
| linux-f5-r2 | linux / ordinary PTY | F5 | 2 | NOT_RUN | absent |
| linux-f5-r3 | linux / ordinary PTY | F5 | 3 | NOT_RUN | absent |

## Per-attempt receipt and independent assertions

Each receipt requires a unique slot/attempt ID, frozen prompt/fixture hashes,
submission/request/action correlation, Process/boot and durable evidence references,
source/binary/model identity, effective tools/backend/grants, start/end/elapsed
measurement, raw terminal capture and verification report. The report records
baseline/final source hashes, allowed diff, behavior checks, artifact locations,
unauthorized-source/credential/runtime/network canary results where exercised,
effect counts and owned-process/scratch cleanup. Do not infer backend confinement
from command exit or an unconfigured native-test count.

Interventions have timestamps, reason, actor and affected attempt: planned
approval, planned lifecycle control, unplanned advice, operator source edit,
environment repair or product repair. Separate approval-wait time when available.
Final answers/captures are supporting evidence, not effect verification. Usage and
cost fields remain unknown if provider evidence is absent; never derive pricing
from guessed token counts.

## Honest aggregation and correction

First outcomes are PASS, FAIL, ENVIRONMENT_BLOCKED, UNSUPPORTED or NOT_RUN with a
specific cause. Every initial slot stays in the thirty-slot denominator. Report
executed attempts and qualified completions separately by family/platform;
blocked/unsupported slots do not satisfy completed repetitions. Additional attempts
link to the original slot with their source, changed inputs and intervention log.
Report first-attempt success, assisted completion and zero-unplanned-intervention
completion separately. Planned approval/lifecycle counts are visible even when
there was no corrective help; this metric is not approval-free autonomy.

A qualification conclusion requires at least three proven real-model completions
in each platform/family, exact behavior/effects and the required lifecycle/terminal
evidence. Original unsuccessful attempts remain part of the report. A production
repair requires affected native reruns; if unaffected-slot applicability cannot be
proved, rerun those slots too. Doc-only changes may retain native evidence when the
production source and binary identity are demonstrably unchanged. No general
self-bootstrap or auto-routing eligibility follows from this bounded sample.
