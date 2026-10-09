# Development qualification and terminal presentation goal — 2026-10-09

The user authorized implementation of the confirmed terminal design, common
Linux tooling and repeated real development acceptance. The execution goal is
active. This roadmap links deliveries; it does not expand the UI change's
normative scope or activate the independent installation/cache change.

## Ordered deliveries and completion gates

| Order | Owner | Required result | Completion evidence |
| --- | --- | --- | --- |
| 1 | `refine-terminal-output-presentation` | Label-free output across every semantic surface; compact Actions; proven read-only groups; compact plan records with historical snapshots; stable input and retained details | Its 17 tasks, focused regressions, native PTY/Herdr acceptance at 48/80/120 columns, quality, reviewed PR and current-head CI; canonical sync after merge |
| 2 | Separate Linux toolchain change, proposed after the UI slice | Preserve selected shell/git/Rust executables, PATH order and required runtime files in the existing namespace backend; provide isolated writable caches and explicit project dependency access | Real local dependency build/test and git diff under supported and unsupported PATHs; absence/escape/read-only/revocation/descendant cancellation tests; no widened home access or silent executable replacement |
| 3 | Separate repeated-development qualification change | Repeat each of five frozen task families at least three times on supported macOS and Linux entry paths; refine observed failures | Per-run source/binary/model/environment identity, first-attempt success, intervention count, duration, exact artifact/effect assertions, failure cause and retained transcript; rerun affected cases after fixes |

Before each follow-up starts, create its own proposal, design, normative deltas
and task list, using existing owners. Node/package-manager or Python/venv support
requires an actual recorded task that needs it; it is not an unsolicited tool
installation program.

## Frozen task families for delivery 3

1. Small RED-to-GREEN fix: reproduce one failing test, change production code,
   verify the targeted test and adjacent public behavior.
2. Cross-file change: update an API and its callers, test observable behavior,
   and verify the exact artifact diff without unrelated edits.
3. Failure correction: encounter a real compiler/test/tool error, inspect its
   retained diagnostic and fix the cause, without accepting a false pass.
4. Cancel/revoke/recover: interrupt bounded work, revoke authorization, verify
   blocked later effects and restore only through explicit recovery/approval;
   completed effects must not replay.
5. Long-output follow-up: inspect stdout/stderr or a diff exceeding inline
   bounds, ask a dependent question or make a bounded follow-up edit, and verify
   referenced evidence, result correlation and once-only effects.

Use disposable small projects and explicit grants. Initial repetitions cover
macOS/Herdr and Linux/ordinary PTY, with cross-host interaction smoke coverage;
freeze the exact matrix before running it. Do not count unsupported or skipped
slots as passes. Report first-run outcomes separately from corrected reruns.
Scripted fixture qualification, real-model task execution, and autonomous
code authorship are separate evidence categories. Codex implements this goal;
Alan is exercised as the product under test, not assigned this implementation.

The native revoke/remount failure found during UI acceptance has the independent
`refresh-model-process-directory` repair. Its targeted native rerun and captured
requests are recorded in that change's `acceptance.md`; review/CI/merge remain
separate gates. This is an observed reliability repair within delivery 1's
qualification work, not an extra completed delivery or a replacement for grouping.

## Measurement and delivery discipline

- UI progress is checked tasks out of 17, including separate local, native,
  review, CI and merge gates. Do not present planning-artifact completion as UI
  implementation progress.
- Broader progress is completed deliveries out of three; quantify repeated
  qualification using passed/failed/unsupported runs and human interventions.
- No automatic routing activation or general autonomous self-development
  qualification follows from this goal.
- Prepare reviewable PRs; retain the user's existing choice to merge them.
  Merge waits do not authorize replacing unverified later phases with claims.
- Record discovered UI defects in the owning change and fix the shared cause;
  preserve earlier terminal scrollback, original Tool content and evidence.

## Initial implementation baseline

Source: main `aac0e6117c3e2a629267113e3de6a418022c3b83`.
Worktree: `output-presentation-20261009`; branch:
`codex/alan-output-presentation-20261009`.
Existing UI design tasks were 0/17 complete at goal creation. The first
renderer slice has 3/17 checked implementation tasks (trace, generated-label
removal and compact summaries), with 341 library and 12 integration tests
passing. Baseline native captures, grouping, plan-detail refinement, complete
quality/review/CI and fresh native acceptance remain pending. No delivery is
complete (0/3). See `implementation-notes.md` for evidence and limitations. The unrelated installation/cache worktree and main's untracked
proposal are preserved.
