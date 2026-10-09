## 1. Baseline and implementation scope

- [ ] 1.1 Freeze fixtures and native captures for the complete output inventory and five UI workflows; record source, binary, model, terminal size and expected effects.
- [x] 1.2 Trace rendering callers, history/drain tests, detail selection, runtime metadata and retained plan snapshots; document the smallest owner-correct implementation and affected fixtures.

## 2. Unified output presentation

- [x] 2.1 Remove every typed renderer-generated role-prefix path, including details, plans, errors, pending input and thinking; preserve literal lookalikes and existing route markers with regressions.
- [x] 2.2 Compact Action summaries, remove repeated paths/commands and per-Action hint scaffolding, and keep primitive-specific status and retained evidence accessible.
- [x] 2.3 Align live activity, notices, failures and request hierarchy; verify waiting, rejection, cancellation and unknown outcomes remain explicit without duplicated state lines.
- [ ] 2.4 Verify the contextual detail hint and existing navigation from supported surfaces, including Host page-key aliases and restoration of the same draft/cursor.

## 3. Read-only grouping and plan history

- [x] 3.1 Reuse or minimally extend Runtime-owned metadata for positive read-only eligibility; verify older/unknown results and arbitrary shell commands remain standalone.
- [x] 3.2 Group eligible adjacent successful Actions while preserving member selection, status, chronological boundaries and authority context; test all group-ending cases.
- [ ] 3.3 Preserve per-Action updates, attachment fencing and history reconciliation across grouping, partial drains, repeated identical content and reconnect; never rewrite committed scrollback or replay work.
- [x] 3.4 Render each distinct plan change as a compact permanent record; retain and expose its corresponding full snapshot through existing evidence/detail owners, with truthful loss and no latest-plan substitution.

## 4. Verification and native acceptance

- [ ] 4.1 Run focused TUI and affected Runtime/protocol regressions for literal text, failure visibility, stdout/stderr, diff markers, raw detail, metadata fallback and plan retention.
- [ ] 4.2 Check deterministic layout cases at 48, 80 and 120 columns, including Chinese/emoji, long paths, multiline drafts, Markdown/code, long single-line output and completion below input.
- [ ] 4.3 Run the five workflows in ordinary PTY and Herdr on the fresh candidate; verify label removal, concise summaries, member/snapshot detail, stable input, usable scrollback and exact no-repeat effects.
- [ ] 4.4 Run `just quality`, applicable test/check workflows and OpenSpec strict validation; record remaining environment limits rather than treating skipped native checks as passes.

## 5. Review and delivery

- [ ] 5.1 Review the exact implementation diff against all three spec owners, including failure/retention behavior and changes to history identity; resolve findings and collect required current-head CI.
- [ ] 5.2 Prepare reviewable PRs and acceptance evidence; merge only with the user's applicable authorization, then verify the merged revision and required checks.
- [ ] 5.3 Sync delivered deltas into canonical specs after implementation merge, update ADR/disposition delivery evidence, and verify archive-readiness before archiving.

Linux tooling and broader repeated development qualification are intentionally
not implementation tasks in this change. Their continuation order is recorded
in `design.md`; completion here does not qualify those follow-up deliveries.
