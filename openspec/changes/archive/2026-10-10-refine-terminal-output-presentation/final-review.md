# Final implementation review — 2026-10-09

Reviewed PR #1044 at `3f915876e4dab8b730737685278c584a84207586` against
base `997ade6a0e27f10df4a2a54e19daf8b52d929f5a`, repository guidance and all
three normative deltas. The complete diff has 89 files; the candidate Rust tree
is unchanged from the native-qualified `f2a9222d` binary. No remaining blocking
finding was identified in this review. This is the implementer's code review,
not an independent GitHub approval or permission to merge.

## Contract and boundary checks

| Owner | Reviewed implementation | Evidence and outcome |
| --- | --- | --- |
| `rust-inline-tui` | Typed history, Action summaries, notices, pending requests, thinking and detail layout | Generated role-prefix paths are removed without a global content rewrite. Literal lookalikes and authoritative failure/unknown states remain. Plans retain revision-specific records; bounded detail reads report gaps and never substitute the newest snapshot. |
| `tool-result-presentation` | Runtime execution receipts, native implementation classification, authority fingerprints and Action projection | Eligibility requires a successful matching native Process receipt, a bounded concrete owner/submission and unchanged positive read-only authority. Missing metadata, another runner, approval, writes, failures and changed authority fail closed. Tool result text cannot confer eligibility. Commands, paths, stdout/stderr and diff remain inspectable. |
| `alan-renderer-host-contract` | Group projection, partial drain, attachment reset, history reconciliation and asynchronous member details | Groups preserve individual owner/Action identity. Committed rows are frozen; later updates are separate. Concrete historical references remain selectable, absent evidence is explicit and stale replies cannot replace a new selection. Grouping introduces no execution or recovery owner. |

Cross-checked the negative Runtime metadata/receipt cases, read-only boundaries,
all 27 drain/resize combinations, repeated identical later members, same-ID
cross-Process selection, historical evidence removal, unavailable current
catalog and delayed old-selection replies. Plan tests cover retention budgets,
invalid/incomplete event evidence and observed historical snapshots. The native
matrix covers all five UI workflows on six fresh invocations (30/30), with exact
source, diff and once-only effects after exit.

The installation compatibility repair remains narrow: only the known regular
legacy control lock is excluded from payload adoption; unknown entries, symlinks
and directories remain rejected. Source writer exclusion is retained. Shared,
exclusive and migration locks explicitly unlock at the owning scope boundary,
including duplicate-descriptor/error paths, with RED/GREEN regressions and the
real-consumer migration probe. It does not broaden migration policy.

## Verification and remaining delivery gates

All 16 GitHub checks for the reviewed head completed successfully, including
macOS/Ubuntu suites, quality, coverage, release builds, harnesses and CodeQL.
Receipt: `target/3f915876-ci-final.json`. The local full workspace result is
2,959 passed and 15 existing ignored; focused TUI results are 363 library and
12 integration tests. Full quality and strict OpenSpec validation passed.

This checklist/review-only commit must collect its own current-head CI before
the PR is made ready; it changes no candidate Rust or native fixtures. User merge
and post-merge canonical synchronization remain open tasks. No native
cross-Process replacement qualification, broad Linux toolchain support or
repeated autonomous development qualification is claimed by this review.

## PR review follow-up — 2026-10-10

Comment `4229713588` correctly identifies an overlap missed by the review above:
when retained plan history ends at its display bound but the UI captured later
snapshots, whole-collection concatenation duplicates the shared prefix and puts
the retained gap after newer captured snapshots. The real Kernel/AgentFS
regression reproduces seven entries instead of the expected five.

Plan detail now merges on concrete owner, revision and exact snapshot. A matching
retained snapshot appears once with retained provenance; unmatched captured
prefixes/tails remain exact, and the retained gap stays at its original boundary.
Without an exact shared identity, the two histories remain explicitly distinct;
matching revision numbers or content alone cannot substitute another snapshot.
The related error branch also discarded the retained read diagnostic whenever
captured snapshots existed. A second RED regression reproduces one entry instead
of the expected captured snapshot plus explicit read error; both are now retained.

All 366 TUI library and 12 integration tests pass after these repairs. New tests
cover cumulative display bounds, gap navigation, retained provenance, unmatched
captured prefixes, different owners/revisions/snapshots and a removed Process's
event history. The Action catalog merge already excludes the current owner from
observed references and retains its catalog failure warning; attachment/history
merges retain concrete identities and generation fences. No equivalent
whole-cache concatenation path requiring another repair was found there.

Receipts: `target/plan-history-merge-red.log`,
`target/plan-history-read-failure-red.log` and
`target/plan-history-review-green.log`. This is a narrow plan-detail follow-up;
the 30 native UI slots remain evidence for the earlier `f2a9222d` candidate, not
a fresh native run of this patch. The follow-up requires its own normal quality
gate and current-head CI before merge; no durable events or Tool effects change.

### Initial submission notice ownership

The automatic review of `8824362d` raised comment `4235779335`. It is valid:
the successful input writer installed an unowned local notice, so the queue's
guard for unrelated Runtime notices rejected subsequent admission updates.
It also left no queue-hint reference for a terminal receipt arriving before the
first queue event to retire. The RED regression reproduces this direct-completion
residue without relying on a later `turn_started` notice.

The production submission path now installs the initial notice and its queue
hint together through the existing typed notice representation. Both use the
actual submission ID; queue admission can update it and direct terminal receipts
can clear it. The guard for unrelated Runtime notices remains unchanged.
Regression cases cover queued/paused admission, direct completion/failure/cancel
before any queue event, newer drafts, older completions and a Runtime warning
with identical displayed text. All 368 TUI library and 12 integration tests pass;
receipts are `target/submission-notice-red.log` and
`target/submission-notice-green.log`. Other local notices belong to request,
model or project controls and must remain protected from queue updates; no other
ordinary input producer requiring this repair was found.

## Final delivery verification — 2026-10-10

Final head `2c30cf38b2bb064ff6262e777c11dd77cb4f93bc` passed all 16 PR checks;
automatic review completed without further findings. Both review threads are
resolved. The user merged PR #1044 as `faf747c2f5e8c389992144093250b4d5e538eeb5`,
whose 15 main checks also passed. Earlier pending-CI/merge paragraphs retain
their historical scope and are superseded by this receipt.

Fresh isolated ordinary PTY acceptance at 80×22 on the final repair source
confirmed queued admission updates the initial notice, cancellation preserves
the Runtime pause warning, explicit `/continue` clears the stale notice and
executes the queued effect exactly once. After the cancelled command delay,
`effects.log` is exactly `queued-once\n`; the cancelled tail is absent, README
is unchanged and `/quit` exits 0. No provider request was needed for these
explicit Tool commands. Receipt: `target/submission-notice-native-acceptance.json`.
The full 30-slot matrix remains evidence for its earlier frozen candidate.
