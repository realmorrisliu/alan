# Planning delivery — 2026-10-14

Current status: planning merged in PR #1060 at `a4ad5992`; canonical adoption was
rechecked on 2026-10-23 against fetched main `9313b5a0`. This delivers the planning
and earlier canonical closure, not the new autonomy controls. The original
pre-merge verification record follows; see the post-merge verification below.

Q1–Q16 are confirmed. ADR-0060, four independent changes, glossary/current-guide
updates and the four merged-change closures are prepared on
`codex/plan-personal-autonomy-20261014`, based on main `ea929481`.
This record describes local documentation, not a merged PR or new runtime delivery.

## Verification

- CI-pinned `@fission-ai/openspec@1.4.1 validate --all --strict`:70 passed,0 failed.
- `scripts/check-openspec-current-surfaces.sh`:pass.
- `git diff --check`:pass.
- Relative Markdown file links in changed/new Markdown:0 missing targets.
- Independent governance/authority and daily-use/continuity reviews:pass after
  correcting missing owner deltas, aggregate reviewer fallback and task-denominator
  accounting. Exact source evidence was used for older merge receipts.
- No tracked files in pre-existing historical archives changed. Newly archived
  changes retain their earlier evidence, original outcomes and qualification limits.
- No production code, personal store migration, website edits, model task runs or
  new two-week qualification was performed by this documentation delivery.

Canonical synchronization reflects earlier merged code. New delegation, trusted
posture, budget and continuity deltas remain under active changes and must not be
mistaken for shipped controls. Review/CI/merge of this documentation branch remains
separate from local validation, as does implementation of the new plan.

## Planning PR review follow-up

PR #1060 review found two remaining canonical synchronization issues. Remove the
old proposal/adoption qualifier from delivered read-only dependency projection and
its cumulative trust delta, and align composer-history scope with the shipped
product-store binding. The existing foreground_history_is_product_store_owned_and_restarts_typed
test already verifies same-store recall and isolated-store separation. No runtime
behavior or prior historical archive changes are involved. New-head CI/review
remain separate from the original head’s passing checks.

## Retained storage-contract audit

The next review found the same obsolete channel identity in retained active input
deltas and accepted package ADRs. The follow-up also audits the Connection/durable
state/attachment ADRs, the remaining input delta and current handoff/task text.
Active requirements now use explicitly bound product stores. The standalone delta
reuses the adopted canonical installation requirements and drops an already-applied
rename, so later synchronization cannot restore `alan-dev` or channel ownership.
Original dated test receipts and pre-existing archives remain historical evidence.

The proactive-memory plan remains parked. Its disposition explicitly rejects the
retained channel/Host-directory backing premises and forbids synchronizing them
without replacement; this does not reactivate its tasks. Package, Connection and
File-Server Service ownership and authorization boundaries remain unchanged.

## Post-merge verification — 2026-10-23

Fetched main `9313b5a0943cac3a1251b9418bef7d8e24213cc4` contains the planning merge
`a4ad5992c80aaa8706bf4d8df7f5f614a3a14e4f`. Canonical specs and historical archives
have no changes between those commits or between the planning merge and the
published budget/evidence branch `224f2b8a`.

The four archived changes in the document map contain 53 requirement entries:
47 match canonical text after whitespace normalization and three removed entries
remain absent. The other three differences are accounted for:

- The earlier repeated-development prompt block is superseded by the later
  composer block, which matches the canonical contextual two-line prompt.
- Read-only dependency projection has lost its obsolete proposed/adoption
  qualifier, as required by the planning PR review; its behavior remains adopted.
- The legacy inspection scenario has a corrected title; its body still rejects
  retired channel selection instead of deriving new authority from it.

All twelve direct relative Markdown links in the four archives' 45 Markdown files
resolve. External URLs and anchor existence were not retested. The current-surface
guard passes and CI-pinned OpenSpec 1.4.1 strict validation passes 70/70. Existing
archive bytes were not edited. These are closure checks, not reruns of historical
model tasks or evidence of the new delegation/continuity implementations.

Audit details are retained with the two-task pilot evidence under
`~/Library/Caches/Alan/daily-use-pilot/20261023/evidence/` in
`canonical-closure-audit.json` and `archive-relative-links.json`.
