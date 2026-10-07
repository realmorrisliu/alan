# Delivery — 2026-10-07

Status: local implementation, native acceptance and independent review passed;
remote current-head CI remains separate. PRs #1032 and #1033 are merged; this
UI branch is based on main `43700bb1`.

- 335 terminal library tests passed, including continuous 16–80-column queue
  and model-control states, streamed fence labels, literal code, Unicode,
  repeated drain, reconciliation, completion anchors and accessibility policy.
- Final-tree `just quality` and strict OpenSpec validation passed.
- Independent Spec and Standards review passed on frozen patch SHA-256
  `d65644f740bba16883ec9b24a9ec46d049b3c85a9b818d91abfb13f3741cad25`.
  Review caught queue metadata consuming model space and bare ellipses at
  critical widths. Both root causes and the missing boundary tests were fixed.
- Native evidence: `~/Library/Caches/Alan/ui3/`, pinned binary/source in
  `build.json`, artifact hashes in `evidence-sha256.json`. PID `80519`, boot
  `1ca9beff-f2d3-4242-8bfd-080650df0a2c`, Root `8`, owned named Herdr session
  `alan-ui-readability-20261007`, pane `w1:p1`.
- Real Sol/medium output verified Rust and diff boundary labels, four-space
  code indentation, diff signs and answer separation at 94 and 47 columns.
  The narrow prompt retained `next gpt-6.1-sol`, ready and queue state.
- Typing `/project` character by character kept the input at visible row 28
  throughout candidate filtering. Existing native-backend tests also verify
  actual cursor coordinates and no candidate-driven history drain.
- A second real 40-line answer forced host scrollback. Both earlier code blocks,
  literal code and every numbered answer line remained available exactly once;
  widening back to 94 columns preserved the same result.
- `/quit` exited normally with code 0 after the queue settled; native PID gone.
  Test-only pane/session were closed. No runtime or profile defaults changed.

## Complete terminal test follow-up

The rebased CI exposed stale public integration assertions for the newly added
answer-leading blank row and decorated fences. The earlier 335-test command
covered only library tests. Updated both integration suites to assert the new
row positions, cyan language labels and dim boundaries while retaining literal
body, ANSI, late-closing Markdown, Unicode and semantic scrollback cutoff checks.
`cargo test -p alan-terminal-ui --no-fail-fast` now passes all 335 library and
11 integration tests. CI must still pass on the follow-up commit itself.

## Active-admission review correction — 2026-10-07

PR #1034 review found that the narrow header removed `active` even when the queue
contained admitted work and execution was idle or paused. Preserve that label in
the compact queue string and in the existing minimum-width fallback, after more
urgent uncertainty/deferred/pause cues. Admission remains distinct from execution;
this correction does not label that work `working` or change queue behavior.

The existing lifecycle regression now covers Idle/Paused at every width from 16
through 80. It failed before the fix at 16 columns (`gpt…-sol · ready`) and now
passes with an active cue and bounded width. All 335 terminal library tests and
11 integration tests pass. Spec and Standards reviews pass. Logs are retained at
`~/Library/Caches/Alan/ui-active-header-{red,tests}.log`. Prior native acceptance
remains tied to its recorded source; this targeted presentation fix is validated
by the updated renderer regression and requires new current-head CI.

## Awaiting-action priority correction — 2026-10-08

A subsequent PR #1034 review reproduced queue metadata replacing `approval`
at minimum widths. The same fallback also affected project selection. Preserve
these two current user actions when the combined state and queue cue cannot fit;
queue details remain available through `/queue`. Other queue urgency and model
control priority are unchanged.

The existing model/status regression now combines confirmation and project
selection with unknown, uncertain, deferred, paused and active queues at every
width from 16 through 80. Its pre-fix failure was `gpt-6-luna · q ?` at width 16.
All 335 terminal library tests and 11 integration tests pass. Evidence logs:
`~/Library/Caches/Alan/ui-approval-priority-{red,tests}.log`. This targeted change
requires independent review, the complete quality gate and fresh current-head CI;
the earlier native visual acceptance is not attributed to this source.

## Pending-admission compact cue — 2026-10-08

The next PR review identified the remaining known queue case: pending admissions
without other flags fell through to the activity status alone. Add `queued` to
the existing cue selection after uncertainty/deferred/pause/active, preserving
the awaiting-action priority above. This completes the snapshot field cases;
counts and simultaneous details remain in `/queue` when space is insufficient.

A narrow-header regression covers pending-only queues while Idle, Running and
Paused over widths 16–80. It failed before the fix at width 16 with
`gpt…-sol · ready`. The awaiting-action matrix also includes a valid pending-only
snapshot. All 336 terminal library tests and 11 integration tests pass. Evidence:
`~/Library/Caches/Alan/ui-pending-header-{red,tests}.log`. Independent review,
the complete quality gate and current-head CI remain required for this correction.
