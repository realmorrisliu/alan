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
Task 1.1 stays open. The first renderer slice deliberately leaves full plan
lists readable until historical snapshot selection is implemented; it must not
be counted as compact-plan completion.

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
