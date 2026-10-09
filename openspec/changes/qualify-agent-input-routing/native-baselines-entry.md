# Native shadow requalification entry — 2026-10-08

## Scope

Requalify the existing advice-only input consumer after #1039. Improve its finite
candidate descriptions around whole-input intent, quoted/data boundaries and
ambiguity; do not add a dispatcher, shell parser, special-case corpus allowlist or
activation control. The v1 corpus, labels, repeats and numeric budgets remain
unchanged. Freeze the new source, Connection selection and harness before real
candidate measurement. Prior v1/v2 evidence remains immutable and failed.

## Matched native baselines

The existing `shadow_client_fixture` and collector admit original bytes through
actual file-backed TUI or `run_stdio_task`, into the existing Root Machine. The
new baseline selections exist only in this example/harness:

- Typed: the pinned TypeSafe `jev-1.13.0` choice adapter and new captured criteria.
- Generation: a harness-only choice facade calls one captured dev ChatGPT
  `gpt-6.1-sol`, medium generation operation, with the same candidate criteria as
  instructions and no Tools. It reuses the component baseline's allocator,
  streaming validation and cleanup. This does not advertise a shipped ChatGPT
  finite-choice capability. Its real source profile, operation, events, usage,
  terminal state, abort/close receipts and unknown billing are retained.
- Prefix: shadow evaluation is absent. The observer reads the actual length-framed
  `io/input` record and correlates it with ordinary input completion. The shipped
  parser's intent and exact body must match; original unprefixed input stays Agent.

All three use fixed mock ordinary generation after admission, an empty Tool
registry and no Host project mount. They measure routing advice, never model-driven
Tool work. Explicit command overrides may complete with unavailable execution
because the harness intentionally lacks bash; those receipts are not successful
command execution. Neither classification nor prefix advice grants authority.

The collector uses one monotonic clock per attempt. Redirected input waits for a
Host-ready marker before writing stdin and EOF; interactive input waits for the
initialized native TUI before pasting. It stops timing when the correlated Machine
completion report is observed, before requesting terminal exit. Redirected fixtures wait for a timing acknowledgement
after writing their completion report, so an after-exit timestamp is not accepted. This includes
client parsing, queue admission, Machine processing, durable evidence, fixed mock
completion and polling overhead. Host boot, initial TUI rendering and Process exit
are excluded. Full-window latency is a conservative routing gate; model-only
elapsed time remains a separate diagnostic. Raw paired timings retain their
jitter; do not turn negative differences into free latency or compare old warm
component timing with this window.

Generation is bounded to 25 seconds, then existing abort and descriptor close
operations and terminal-status observation each have one-second bounds, inside the Machine's 30-second choice
window. A fixture-owned task retains the nested operation; dropping the outer
choice signals cancellation, and Host teardown joins its bounded abort/close and
final receipt work before shutting down the captured generation Connection.
No implicit retry is permitted; a receipt already attempted is not reused.
Missing usage or per-call subscription billing remains unknown. Public TypeSafe
list pricing can estimate recorded input tokens, but is not an invoice.

## Evidence and limits

Record source/binary digests and build identity before collection, and keep helper
source digests in every fixture export. Preserve failed attempts and unsupported
redirected pending responses in the denominator. The redirected client still
cannot answer a pending form: a pending request plus its actual `needs interactive
input` error is retained, never replaced by synthetic aP response submission.

The full matrices are typed 324, generation 324 and native prefix 300 attempts.
Each has three repeats and both original surfaces; prefix excludes the 24 pending
response slots because that baseline has no response-admission classifier.
Correctness uses the frozen expected classes. Valid NoMatch advice maps to
ambiguous, never command. Compare all non-bypass ordinary attempts across the three
baselines, including terminal model failures and their full-window times. A missing
native result, cost provenance, budget gate or parity receipt blocks qualification.

Local test/build/smoke evidence precedes candidate freeze. Independent review and
current-head CI remain delivery gates. No automatic routing activation is authorized.

## Pre-freeze review corrections

Independent review of `d67f9235` found two harness gaps: an after-exit redirected
timing fallback and a dropped outer choice future bypassing nested generation
cleanup. The correction removes that fallback and requires a completion timing
handshake before redirected shutdown, including the unsupported form path. The
collector regression rejects an already-exited report as matched timing. Native
prefix smoke passes on interactive and redirected clients with the handshake.

The fixture now owns a single bounded generation task. Dropping the outer future
signals that task; fixture shutdown joins cleanup before closing the captured
Connection host. A real llmfs operation with a waiting fake provider verifies
outer-future drop after generation commit and event-tail creation: abort is
acknowledged, the tail is closed, status is aborted, the failure receipt is
retained, billing stays unknown and provider dispatch count stays one. This is
lifecycle fixture evidence, not a real-model correctness or price measurement.
