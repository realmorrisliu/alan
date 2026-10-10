# Implementation trace — 2026-10-09

Baseline source is main `aac0e611`; the implementation worktree contains the
confirmed ADR and OpenSpec artifacts. This trace is engineering evidence, not
native acceptance or merged delivery.

## Touched paths and callers

- `history.rs` owns typed plain/styled Tool, plan, pending-yield, error and
  completed-thinking projection. `history/action_summary.rs` is shared by
  file-backed inline rendering and pruning. Both must agree on physical rows.
- `file_backed/app.rs`, `app/history.rs` and `layout.rs` own individual Action
  indices, partial drain and live contextual detail discovery. Preserve indexed
  Action identity and semantic input/assistant tails.
- `file_surface.rs` converts Runtime result metadata to presentation and retains
  completed Actions once. Unknown-position hydration intentionally omits them.
- `action_detail_io.rs`, `app/action_modal.rs` and asynchronous detail events own
  fresh, pinned member reads, generation fencing and unchanged composer drafts.
- `UiPlanSnapshot` includes explanation and ordered items. AgentFS UI events are
  the existing history evidence; historical detail must use that evidence or the
  corresponding already-hydrated snapshot, never substitute `ui/plan` latest.
- Runtime `write_action_metadata` is used by Process-backed Tool execution,
  explicit commands, directory control and rejected commands. Existing result
  metadata has no positive grouping eligibility plus turn/authority correlation.
  Tool capability alone can be a package claim; inspect actual Runtime binding
  before implementing grouping. For this first slice, use existing Action identity
  and bounded result primitives, with standalone fallback for every Action.
  Group eligibility must be added at the Runtime execution boundary, not inferred
  in the TUI.
- `history_merge.rs` legacy prefix matching remains compatibility logic, not a
  role-label generator. Literal `xxx>` fixtures must remain unchanged.

## Initial verification slice

Public rendering regressions cover normal/expanded thinking, plan, error,
flat Tool content and styled diff at 48/80/120 columns. Routine-summary tests
cover physical-row bounds, command/path deduplication, textual failure and
truncation. Existing semantic-drain, retained detail, Markdown, completion and
accessibility suites remain the regression authority.

Native captures for the full five-workflow matrix have not been taken yet.
Task 1.1 stays open. The first renderer slice left full plan lists readable;
the subsequent historical-plan slice below supplies compact-plan behavior.

## First renderer slice: verified local results

The public no-role-label regression failed on the original code with
`tool> ✓ Read src/main.rs`; it passes after removing shared generated-prefix
paths. Literal `server> ready`, `a > b`, `tool> literal`, quotations and opaque
legacy prefix matching remain content/compatibility, not removal targets.

Routine summaries now occupy one or two physical rows, with status text reserved
in the title row even for long Unicode paths. Commands retain actual exit state;
file/diff summaries avoid repeating a path already in the title. A shared
`Ctrl+O details` hint uses the existing reserved row below the composer; completion
and pending forms retain their input/key ownership. Rejected and cancelled
Action statuses retain their own words rather than being folded into failed.

`cargo test --locked -p alan-terminal-ui --no-fail-fast` passed 341 library and
12 integration tests. Updated assertions account for compact summary rows and
two-space diff framing while still verifying literal indentation, diff colors,
semantic drains, per-Action details, draft/cursor return and generation fencing.
Logs are local ignored artifacts in `target/tui-tests.log`.

Strict OpenSpec validation passed 66/66 items in this isolated worktree; the
independent installation/cache proposal is intentionally absent here. The
current-surface guard and `git diff --check` passed. The initial full `just quality` gate passed, including workspace Clippy,
Rustdoc, standalone CLI and distribution checks. Its receipt is
`target/quality.log`; later implementation changes require a fresh gate. No fresh native acceptance, read-only
grouping, compact historical plan detail, Linux toolchain expansion, real-model
repeat matrix, current-head CI or merged delivery is claimed by this slice.

## Historical-plan slice: verified local results

Each distinct plan change, including explanation-only changes and clearing the
plan, now appends one bounded summary row. Its semantic cell retains the exact
snapshot and concrete Process owner. Ctrl+O opens the existing detail surface;
`p` switches between individual Actions and historical plans. Historical reads
use the pinned Process's existing `machine/ui/events`, never the latest
`machine/ui/plan` projection. Exact captured snapshots remain distinguishable
when retained history is unavailable or cannot be correlated.

Reads have a five-second deadline and bounded chunks/snapshots. Invalid,
incomplete or oversized records leave earlier complete snapshots inspectable
and expose the gap explicitly; they never substitute another snapshot. New
authorization/input requests close the detail surface and invalidate late
responses so a modal cannot hide a pending request. Returning from details
preserves the draft, cursor and intent.

The focused suite passed 348 library and 12 integration tests, including plan
changes, drained scrollback, Unicode chunk boundaries, historical owner and
generation fencing, source gaps, exact captured fallback and request priority.
`git diff --check`, Rust source-size and architecture checks passed. A fresh
full `just quality` passed; receipts are `target/plan-tests.log` and
`target/plan-quality.log`. Native PTY/Herdr acceptance, read-only grouping,
Linux qualification and real-model repeats remain open.

## Wide-character scrollback repair

Partial real-model Herdr acceptance found a native scrollback problem missed by
buffer-only tests: the installed Ratatui insertion path emits all cells, so a
wide character's covered cell emitted an extra space and shifted the rest of
the row. The native adapter now clears the covered cell's emitted symbol after
rendering. No dependency, terminal implementation or feature flag was added.
The regression sends the complete cell grid through the actual Crossterm backend
and VT parser at 48/80/120 columns, checking Chinese, emoji, literal lookalikes
and the next row. See `native-acceptance.md` for the original failure and fresh
Herdr reacceptance. All 349 library and 12 integration tests, the fresh full
`just quality` gate and strict OpenSpec validation (66/66) passed. Other
qualification gaps remain open.

## Notice ownership and hierarchy

The prior string-only notice slot discarded Runtime severity and used matching
text to decide whether the queue could replace a notice. Two regressions failed
on that source: settled input left a duplicated queue line, and a Runtime warning
with identical text was mistaken for a queue-owned hint (`target/notice-red.log`).

The existing slot now retains the Runtime kind and an optional exact local-input
submission ID. Queue refresh/admission cannot overwrite an unrelated notice;
terminal settlement removes only its own hint. The header remains the queue
projection, including unknown, uncertain and paused states. Error and Warning
notices have textual severity as well as color; routine notices are subdued.
Local validation, model uncertainty/rejection/cancellation and recovery gaps keep
their known severity. The immediately paired failure Notice is suppressed only
when existing correlated-completion evidence already retained that failure in
permanent history; independent or repeated errors remain visible.

All 352 TUI library tests and 12 integration tests passed. The suite covers queue
event order, cancellation/rejection/unknown outcomes, late acknowledgements,
Root changes, unrelated notices with identical text, severity, literal `xxx>`
content and unchanged Chinese/emoji draft/cursor at 48/80/120 columns. Full quality
and strict OpenSpec (67/67) passed for the core notice slice. The final commit gate
also validates subsequent local severity refinements. Fresh native evidence
is a partial workflow sample; tasks for the complete matrix remain open.

## Positive native read-only grouping evidence

Only compiled ReadFile/Grep/Glob/ListDir implementations opt in; the default is
false, and the actual invocation must also have Read capability. Tool text,
package claims and arbitrary Bash commands cannot enable eligibility. The actual
shared native runner records a bounded one-use Tool Process/parent/name receipt
under unchanged live authority. A different runner, even with the same known
registered Tool, supplies no receipt. Missing/changed bindings, adapters,
authority, accepted submission or an explicit human-approval boundary suppress
eligibility. Namespace Runtime consumes the matching receipt and exports an
optional `read_only_context` only after the authoritative Action succeeds.

Context retains a concrete Process owner, accepted Machine submission and an
opaque SHA-256 identity of selected directory plus live Host Mount projection
identities/paths/access and the existing service generation. It exports no Host
paths and grants no authority. The transient native bridge retains at most 128
late receipts; eviction merely leaves an Action standalone. Durable results still
belong to the existing AgentFS Action and rollout/checkpoint owners. No new
execution service, replay log, dependency or provider request was added.

The actual Process execution fixture passes 12 eligibility cases, including
forged Tool claims, a different actual runner, missing adapter/authority,
wrong parent, oversized context, approval, missing submission, Write capability,
unknown native classification, execution failure and changed live scope. It
also checks one execution per Action, exact one-use receipt identity, bounded
late receipts and a changed selected directory. Protocol validation checks
concrete/bounded correlation; builtin tests keep Bash/edit/write unclassified;
Host Mount tests check revoked/replaced grants and absence of native backing paths.

Full affected suites passed: Runtime 1,405 library tests plus 20 integration tests
(one existing ignored), protocol 57 library plus seven integration tests,
Service Manager 146 library plus two integration tests, tools 138 library tests,
TUI 352 library plus 12 integration tests. Receipt: `target/group-tests.log`.
Full `just quality` passed (`target/group-quality.log`). This qualifies metadata,
not rendered groups or the complete native matrix. Tasks 3.2/3.3 remain open.

## Rendered read-only groups and useful command excerpts

Runtime-qualified neighboring Actions now share one bounded completion header
and one concrete operation/excerpt row per member. Their original cells retain
concrete Process/Action identity, context and result. The existing detail catalog
still selects each Action individually. Missing/invalid eligibility, different
Process/submission/authority, messages, explicit commands, plan records and
non-successful/unknown-classified Actions end the group.

A physical prefix drain freezes the involved group rows before pruning any
member. Frozen cells retain source identity for reconciliation; content equality
cannot substitute a different Process/Action. The existing observation map also
records committed eligible presentations. Repeated observations are suppressed;
changed evidence becomes a standalone update, preserving both old rows and a
new failure if reported. No operation is executed by this projection.

Native acceptance found that `/agent/root` was being compared with the concrete
metadata owner, disabling groups in the shipped entry path. A RED regression
reproduced it. The renderer now reuses the existing attachment's concrete queue
owner, including known-owner/unknown-queue state, and ignores stale Action file
refreshes for another owner. Detached/mismatched metadata remains standalone.
The existing project lost-ack fixture now supplies its actual attached owner.

Command summaries preserve bounded successful stdout, falling back to stderr
when stdout is absent; failures prioritize stderr. Actual exit/status and full
separate streams remain authoritative and available in retained details.

Current focused TUI results: 358 library and 12 integration tests passed.
Full `just quality`, including workspace Clippy/Rustdoc, source-size/architecture
checks and standalone distribution, passed. Tests cover 48/80/120-column group
rows, invalid eligibility, distinct and repeated identities, partial drains,
late updates, reconnect and the Root alias. See `native-acceptance.md` for the
fresh targeted run and its earlier failed candidate. Older-Process detail
navigation, comprehensive resize/drain/reattachment qualification and the whole
native workflow matrix remain open; task 3.3 is not claimed complete.

## Concrete Process member details and grouped resize/reconnect closure

The detail catalog now uses the existing projected Action references together
with fresh current AgentFS catalog reads. Each selectable reference contains a
concrete Process path and Action ID; it carries no evidence body or authority.
Historical references remain selectable after attachment replacement and physical
drains. A fresh selected read uses that reference's original files, while reply
application still requires the current view path, generation and complete
selection identity. Missing files stay unavailable. A current catalog failure
is visible even when observed references remain inspectable.

Detail layout uses two fixed header rows: selected Process and navigation.
The Process remains visible while paging at 48/80/120 columns. This reuses the
existing detail modal, source observation map and pager; it adds neither a new
history owner nor a cached output fallback. Existing callers, events, fixtures
and attachment reset use the same concrete reference shape.

Real Kernel/AgentFS regressions exercise two Processes with the same `a0`, old
Process removal, current catalog failure and a paused cross-Process late read.
Twenty-seven combinations of initial width, resized width and partial drain
verify frozen group suffixes, identity reconciliation and a later distinct member
with identical text. Native PTY evidence covers 48-column paging, distinct member
selection and draft/cursor return; it does not qualify native Process replacement.
Task 3.3 is now locally closed; the five-workflow native matrix remains task 4.3.

The final suite adds explicit 48-column coverage to existing completion,
model-header, detail and streaming/Markdown resize fixtures. Receipts are
`target/detail-process-final-tests.log`, `target/group-resize-tests.log` and
`target/detail-process-quality.log`; the last quality receipt precedes the final
resize fixture and is not the final-head gate. See `native-acceptance.md` for the
fresh candidate identity and exact native scope.

Final local result for this slice: 363 TUI library and 12 integration tests
passed, including the expanded width fixtures. The already-recorded affected
Runtime/protocol/Service Manager/Tool regression receipts remain applicable;
this slice changes no Runtime code. `just quality` passed after the final resize
fixtures (`target/detail-final-quality.log`), and strict OpenSpec validation
passed all 67 current items (`target/detail-final-openspec.log`). Tasks 4.1 and
4.2 are local gates, distinct from the still-open native matrix and current-head
PR review/CI/merge. The ordinary PTY test invocation has exited; no development
Agent was left running by this slice.

The branch integrated merged main `105903159ae0bd4243e6226aabdfdc8a76315f22`.
The Tool registry conflict retained both the PID-only binding lookup and positive
read-only presentation receipts; the final Rust tree was identical to the
already-tested slice. The independent repair's acceptance file uses the merged
main version, keeping its document ownership out of the UI diff.
`cargo test --workspace` then passed 2,893 tests, with zero failures and 14 existing
ignored tests (`target/ui-main-workspace-tests.log`); strict validation passed
67/67 (`target/ui-main-openspec.log`). Existing full-viewport tests cover both
PageUp/PageDown and Space/b, generation/selection changes and exact draft/cursor
return; targeted native PTY and Herdr receipts qualify Space/b interaction.
Tasks 2.4 and 4.4 are locally closed. Full native matrix, final-head CI/review,
merge and canonical synchronization remain separate gates.

## Native matrix findings: result-path deduplication and revocation guidance

The first frozen 80-column slot found that a relative edit argument produced
`Edit sample.rs` while the structured diff contained the resolved
`/mnt/project-request-2/sample.rs`. Literal title/path deduplication therefore
correctly declined to guess equivalence, but repeated the same file inline.
The shared Runtime mapper now uses the successful result's nonempty string path
for existing read/write/edit/list titles; missing, empty or invalid paths retain
argument fallback. Bash and unknown Tools retain their existing metadata.
Original arguments, results, execution authority and side effects are unchanged;
the TUI does not gain argument interpretation or path-resolution policy.

All four mapper callers were traced: explicit commands, directory control,
Agent file metadata and namespace Tool execution. A new table regression failed
before the production change and passed afterward, covering the four known file
Tools, invalid-result fallbacks and unaffected Bash/dynamic Tools. The existing
48/80/120-column summary test now checks a resolved diff path occurs once with
its change counts. A fresh actual 48-column model edit confirmed the inline
summary contains one path plus `+1 -1`, while retained original diff stays
inspectable. The earlier 80-column failure remains recorded in
`acceptance-matrix.md` rather than reclassified as a pass.

The same slot distinguished unrecovered Root dispatch from explicit durable
recovery: an unauthorized ordinary `/continue` dispatches the queued task, which
fails at Tool authorization without effects. Durable recovery separately checks
authority before resuming and retains paused tasks. Existing queue policy is
unchanged; the revocation notice now says to use `/project` before `/continue`
for project work. Native reapproval alone left the queue paused; explicit
continue executed its successor once. A separate negative control failed before
effects under revoked authority.

Fresh local gates passed: Engine 1,406 library tests (one existing ignored),
TUI 363 library plus 12 integration tests, full `just quality`, strict OpenSpec
67/67 and `git diff --check`. Receipts are `target/resolved-path-*-tests.log`,
`resolved-path-quality.log` and `resolved-path-openspec.log`. Tasks 2.2 and 4.4
were reopened when the native counterexample was found and are closed again
after the repair, tests and targeted native reacceptance. The complete native
matrix, exact final-head review/CI, merge and canonical sync remain open.

After this repair, the full workspace run passed 2,894 tests, zero failures,
14 existing ignored tests, across 98 suites including doctests
(`target/resolved-path-workspace-tests.log`). Ignored provider probes are not
native acceptance passes. The previous committed head `a76aa15d` had all 16 CI
checks successful; those checks do not qualify a subsequent repair head.

## New-base CI finding: installation guard completion

Main advanced to `997ade6a` through independent installation/cache PR #1042.
CI for UI repair head `55cd37fd` evaluated that new merged base: 15/16 checks
passed, while coverage's test execution failed at migration test
`changed_extended_metadata_prevents_rollback_without_losing_canonical_data`.
It could not reacquire shared installation access after rollback had ended.
Receipt: `target/resolved-path-coverage-failure.log`, job `113720672706`.
This is a failed test gate, not a coverage-upload failure or a local pass.

The new installation guards relied on closing a File to release advisory locks.
A duplicate open-file description, including a concurrently forked child's
pre-exec descriptor, can keep that lock alive beyond guard completion. Existing
Host singleton and Package Store guards already explicitly unlock on Drop.
Three deterministic regressions reproduced this class without timing sleeps:
shared access, exclusive migration and retained legacy-source locks each kept
a duplicate descriptor alive through guard completion. All failed before the
repair (`target/installation-lock-red.log`, `source-lock-red.log`).

Those existing guard owners now explicitly unlock on completion. Acquired locks
are immediately wrapped, so journal-validation and partial source-acquisition
errors also release through the same Drop path. Active guards still exclude
conflicting users, and closing an old duplicate cannot release a newer guard's
lock. The SourceLocks test replaces its inherited-descriptor retry sleep with
the deterministic duplicate-description check. No lock API, retry policy,
migration authority, data operation or installation planning scope was added.

Focused green checks passed eight installation-access cases, 33 migration cases
and six installation CLI boundary cases, including the original metadata
rollback test, malformed/incomplete journals, symlinks and live exclusion.
The fresh full workspace passed 2,957 tests, zero failures, 15 existing ignored
tests (`target/ui-install-workspace-tests.log`). With no other Alan invocation
present, the opt-in real-process quiescence and source-independent rollback probe
also passed separately (`target/installation-lock-native-process.log`). Fresh
`just quality` passed (`target/installation-lock-quality.log`); strict OpenSpec
passed 68/68 (`target/ui-install-openspec.log`). Task 4.4 is locally closed again;
fresh-head CI remains required. Native UI receipts above use the older candidate
and do not qualify the newly integrated installation layout.

The real local `connection current` probe correctly refused pre-adoption legacy
stores. Read-only migration dry runs for both explicit `dev` and `stable` sources
then rejected the built-in `legacy-connections-migration.lock` at the System
Store root. No data migration was executed. This source-layout compatibility
finding must be resolved before preparing a concrete source selection and
resuming native UI qualification; neither refusal is a native UI pass.

## Legacy control metadata compatibility and fresh qualification

CI on `f29a0d57` passed coverage but failed Ubuntu job `113730282335` at
`retains_existing_native_lock_and_never_creates_missing_lock_files`. Its manually
held raw probe still used descriptor close; a concurrent fork could retain that
description. The test now explicitly unlocks the probe, matching production
guard semantics. The fail-fast macOS cancellation is not a passing test result.

Three deterministic regressions failed before this slice: a known regular
`legacy-connections-migration.lock` was rejected, an active legacy connection
migration did not exclude adoption/rollback, and its own guard retained a lock
through a duplicate descriptor (`target/legacy-layout-red.log`,
`legacy-exclusion-red.log`, `legacy-lock-red.log`). The existing validator now
accepts only that exact regular control file; directories, symlinks and unknown
names remain rejected. SourceLocks retains its native lock for adoption and
rollback; the existing LegacyMigrationLock owner explicitly unlocks on Drop.
The transaction fixture verifies dry-run/apply/retry/rollback preserve source
marker bytes and do not copy the marker into canonical payload. No dependency,
new migration option, unknown-file exemption or retry policy was introduced.

Green checks pass 49 Host tests (two existing ignored), 35 migration tests and
2,959 workspace tests (zero failed, 15 existing ignored, 99 suites). The opt-in
real-process exclusion/rollback probe passes separately with no other Alan
invocation running. `just quality` and strict OpenSpec 68/68 pass. Receipts are
`target/legacy-compat-{host-tests,migration-tests,workspace-tests,native-process,
quality,openspec}.log`. Review and CI must qualify the next committed head.

Real-store read-only dry runs now validate `dev` (three payload components) and
`stable` (one), recorded in `target/legacy-compat-{dev,stable}-dry-run.json`.
An initial concurrent stable probe was refused because the dev CLI was still
running; the sequential retry passed. This refusal is retained as harness
interference, not reclassified as a source failure or successful migration.
No user data migration was executed. The canonical startup requires explicit
source selection, which remains the user's choice before native qualification
can resume. Older UI captures still do not qualify this installation candidate.
