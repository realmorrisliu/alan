# Development qualification and terminal presentation goal — 2026-10-09

The user authorized implementation of the confirmed terminal design, common
Linux tooling and repeated real development acceptance. The execution goal is
active. This roadmap links deliveries; it does not expand the UI change's
normative scope or expand the independently delivered installation/cache change.

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

## Current checkpoint

UI tasks are 14/17 checked; completed goal deliveries remain 0/3. The fresh
canonical-installation candidate has completed all 30 native workflow slots:
ordinary PTY and Herdr, each at actual 48/80/120 columns. Six independent fixtures
have exact source/diff/once-only-ledger assertions after the invocations ended.
Member and historical-plan details, Chinese/emoji draft/caret return, literal
content, command stdout/stderr, edit diff, authorization controls and native
scrollback are exercised. The owned Herdr pane was closed, its caller's original
layout restored and no candidate Alan process remains. See the final matrix in
`acceptance-matrix.md`; older candidates and harness corrections remain separate.

Head `82647a75cb321526a4a06555d4f81d432ada3ce7` has all 16 CI checks passing.
Its Rust tree is unchanged from the built `f2a9222d` candidate. Final exact-diff
review, required CI for the next evidence/checklist commit, user merge and
canonical synchronization remain open. PR #1044 remains draft while those
review/CI gates are collected. Native five-workflow closure does not claim native
cross-Process replacement, general self-development or Linux qualification;
real Kernel/AgentFS tests remain the evidence for replacement/attachment fencing.

### Earlier checkpoint evidence (historical)

UI tasks remain 12/17 checked; goal deliveries remain 0/3. Local tracing,
label removal, compact Actions/plans, notices, positive read-only grouping,
member/history lifecycle, contextual detail and regression/layout/quality gates
are closed. Complete native acceptance, exact final-head review/CI, merge and
canonical synchronization remain open. Grouped lifecycle tests include concrete
old-Process references, same-ID fencing, unavailable evidence and 27 drain/resize
combinations. The separate Process-directory repair #1043 is merged; its canonical
synchronization remains a separate closure gate.

The resolved-result title repair and revocation guidance have actual 48- and
120-column five-workflow acceptance: 10/30 workflow slots on that pre-installation
candidate, with supported read/search corrections recorded separately from first
attempts. Exact edits, once-only effects, unknown cancellation outcomes, distinct
member/plan evidence and Chinese/emoji drafts/cursors were checked. The next
80-column request returned correct reads after high observed latency, but its
remaining interactions were not exercised; it is a partial attempt. Earlier
failed attempts and targeted Herdr smoke receipts remain in `acceptance-matrix.md`
and `native-acceptance.md`. They do not qualify a newly integrated candidate.

Main advanced through installation/cache PR #1042 to `997ade6a`; the UI branch
integrated it as `4378a77a`. CI for earlier repair head `55cd37fd` passed 15/16
checks and exposed a real installation guard-release failure in coverage's test
execution. Shared/exclusive/source guards now explicitly unlock when their owning
scope ends, including duplicate descriptors and error paths. Deterministic
RED/GREEN and live exclusion tests cover the repair. New-base full workspace
passes 2,957 tests (15 existing ignored); the separate opt-in real-process
migration/rollback probe, full quality and strict OpenSpec 68/68 also pass.
Fresh-head CI remains required; draft PR #1044 is not merge-ready.

CI on `f29a0d57` passed coverage but failed the Ubuntu test suite: a manually
acquired test probe also relied on descriptor close instead of explicit unlock.
The macOS suite was cancelled by fail-fast, not passed. The test now releases its
probe explicitly. Three further RED/GREEN cases qualify known legacy control
metadata, exclusion of active connection migration and the legacy owner's lock
release. The current compatibility candidate passes 2,959 workspace tests with
15 existing ignored, full quality and strict OpenSpec 68/68; new-head CI is pending.

Both real-store read-only dry runs now pass: `dev` reports three payload
components and `stable` one. The exact known regular migration lock is retained
as source control metadata, excluded from copied payload and covered by writer
exclusion; unknown names, directories and symlinks remain rejected. No user data
migration was executed. Native qualification needs the user's explicit choice
of source before adoption into the canonical installation. These previews do
not constitute successful UI or Linux qualification.

Head `c473c24e26807e0847c6eb4446537559fbc9dac7` subsequently passed all 16 CI
checks, including macOS/Ubuntu tests, coverage, both release builds, quality,
harnesses and CodeQL. PR #1044 remains draft with complete native acceptance
and final review open. Personal-store adoption still awaits explicit source
selection; that is an operational prerequisite, not a missing UI feature.

The user explicitly selected `dev` on 2026-10-09. Fresh read-only validation
passed, then the installation transaction committed all three components.
Independent byte inventories verified all 2,171 service files, two credential
files and the auth file unchanged in the old source and identical in canonical
payload immediately after adoption. The source control lock remains in dev and
was not copied. `connection current` resolves `chatgpt-main`; the new native
invocation displays `gpt-6.1-sol` medium. No source cleanup or cross-source merge
was performed. Goal execution resumed; source selection is no longer a blocker.

Documentation head `f2a9222d` also passed all 16 CI checks. Its newly built binary
has SHA-256 `eaf18e8264d939b2a7043ef6e4cb3554489c9ee7eefef0b163b29132ad9e182d`.
The first fresh ordinary PTY 80×22 invocation completed all five frozen workflows
with source/ledger assertions after normal exit: 5/30 candidate slots at that
checkpoint, separate from the 10/30 older-candidate receipts. The current matrix
above supersedes that partial count. See `acceptance-matrix.md` for the operational
receipts and later native closure.

## Read-only preparation for the Linux follow-up

This inventory was collected before the dev source selection and prepares
delivery 2. It neither activates delivery 2 nor changes this UI change's three
normative capability owners. No Linux packages, mounts or toolchains were
installed or modified by these probes.

The existing `reified_namespace` plan/runner and Host Mount authority remain
the implementation owners. The runner clears inherited environment and uses a
fixed system command PATH; its user-PATH readiness check rejects executable
directories outside the read-only execution substrate or a changed search
order. This conservative fallback must remain until a real toolchain projection
passes the existing confinement and authority gates. Installing Rust alone
does not solve that product gap.

On 2026-10-09, the already-running OrbStack `ubuntu` machine is aarch64, kernel
`7.0.14-orbstack-00380-ga7e0a2dc9535`, ordinary UID 501. Shell and C compiler
exist, but git is absent. Cargo/rustc resolve through `/home/morris/.cargo/bin`
to rustup and report 1.96.0, while this checkout pins 1.97.0. Rustup already has
1.97.0 installed; the follow-up must select and project it rather than claim a
new installation. Its PATH also
contains OrbStack executable directories outside the default substrate. The
unprivileged combined user/mount/PID/network namespace probe exits 0; this
proves namespace creation only, not bind/remount, seccomp, full backend selection
or developer-task qualification. Raw receipts are
`target/linux-follow-up-environment.json` and
`target/linux-follow-up-namespace-probe.json`.

The subsequent independent change therefore needs both an explicit reproducible
Linux fixture with git and the required Rust version, and owner-correct support
for selected shell/git/Rust paths, runtime data and isolated writable dependency
caches. Preserve executable choice and PATH order; do not expose the whole home
directory or silently replace tools. Test real local dependency build/test,
git diff, absence, unsafe paths, read-only grants, revocation and descendant
cancellation. Existing runner tests can return early when capability probes
fail; qualification must inspect readiness and skip diagnostics, rather than
count such an early return as a native pass. Node/Python stay outside this
delivery until a recorded task requires them.
