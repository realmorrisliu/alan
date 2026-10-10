## Context

Baseline: main `aac0e611`. `history.rs` emits role labels for Tool, plan, pending
input, error and expanded thinking; `history/action_summary.rs` fixes routine
Actions at three rows with a repeated detail hint. Plan updates append a whole
snapshot. The live region separately displays running Tools and notices.
Legacy label recognition in `file_backed/history_merge.rs` supports history
matching and must not become a global text-removal pass.

Existing owners already provide five Tool presentation primitives, Action
identity, retained output, plan snapshots, details, recovery and physical-row
layout. This work refines those owners. See `decision-record.md` for the five
user-confirmed choices and ADR-0059 for their rationale.

## Goals / Non-Goals

Goals: remove all renderer-generated `xxx>` role labels, make routine work
compact, preserve important outcomes and requests, and provide truthful detail
without sacrificing input stability or scrollback.

Non-goals: new execution/routing behavior, model-based summarization, automatic
retry, a new renderer state service, terminal/pane management, a desktop UI,
Linux toolchain implementation or general self-development qualification.

## Decisions

### Semantic presentation replaces role prefixes

Change typed rendering paths, not user or Tool text. The ban covers all
Alan-generated role labels of this form, not just a list of five known strings.
It applies to normal, expanded, failure, live and re-rendered history surfaces.
Original content, source Markdown, structured roles, Action IDs and durable
records remain intact. Existing `:` / `!` routes and pending-response behavior
are not role labels and keep their current semantics.

Use concrete operation titles and authoritative states with restrained markers
and indentation. Normal activity is visually subordinate to answers, errors and
requests. Status cannot rely solely on color. Do not replace role labels with
equally repetitive category scaffolding. Icon-only output was rejected because
it makes operation and outcome harder to identify.

### Complete output inventory

| Surface | Default presentation | Detail or preservation rule |
| --- | --- | --- |
| User input / explicit command | Existing literal input with `:` / `!` | Preserve text and route; responses remain correlated to their request. |
| Agent answer / Markdown / code / diff | Answer body with existing readable spacing and code boundaries | Literal `xxx>`, quotes and operators are content, not generated labels. |
| Thinking | Compact completed summary and existing live status | Existing expansion remains available without `thinking>`; no inferred reasoning. |
| Plan update | Completion count and current step, or a neutral update summary when unavailable | Each change retains its own complete snapshot; no latest-plan substitution. |
| Running activity | Concrete current action and known state | Ephemeral; do not append spinner ticks or repeated status notices to history. |
| File reads | Path once and useful bounded metadata | Contents remain inspectable; do not replace content with counts alone. |
| Search / listing | Query or target and available result summary | Eligible adjacent successful read-only operations can share a summary. |
| Write / edit / diff | Path and actual change counts | Standalone; full diff retains textual addition/removal markers. |
| Native command | Command once and actual exit state, with a bounded useful excerpt | stdout and stderr remain distinct; arbitrary shell commands are not grouped as reads. |
| Dynamic / MCP / unknown Tool | Runtime title or Tool name and bounded fallback | Unknown classification remains standalone; raw diagnostics stay accessible. |
| Approval / structured input | Requested operation, scope, choices and keyboard behavior | Always distinct; display changes grant no authority or new default answer. |
| Failure / rejection / cancellation / unknown outcome | Actual state and cause; next action only when known | Never collapse into success or hide in a successful group. |
| Notice / recovery / retention gap | Severity-appropriate concise notice | Preserve unavailable, truncated and unknown distinctions. |
| Context / queue / model / completion / detail UI | Existing contextual input and interaction surfaces | Preserve effective model, candidate placement below input, draft, cursor and detail return. |

Routine Action titles, command/path summaries and detail hints must not repeat
the same information. Reuse bounded physical-row rendering; a compact ordinary
success should normally need one or two rows. Failures may need a diagnostic
excerpt. Put detail discovery in the relevant shared/contextual hint, so users
can still find it without a third row on every Action. Keep the existing detail
navigation and Host-compatible page aliases.

### Group only proven read-only work

The Runtime supplies trustworthy grouping eligibility for known read-only
operations. First inspect existing metadata; add a minimal optional generic
field only if needed. A title, arbitrary Tool output, `Listing` presentation,
Tool-supplied claim or shell string does not prove absence of side effects.
Missing metadata remains compatible and renders a standalone result.

Native read-only opt-in is a compiled implementation guarantee, defaulting to
false; capability or package claims alone cannot provide it. The actual native
Tool runner supplies a matching Process/parent/Tool receipt under unchanged live
authority. Namespace Runtime consumes it once and exports optional concrete
Process, accepted submission and authority-digest correlation only for a successful
Action with no human-approval boundary. The digest includes selected directory
and the Host Mount Service's live grant identities, projected paths/access and
existing generation. It contains no raw Host paths. A different execution runner
or missing/changed context leaves the result standalone. The runner's bounded
transient receipts bridge native execution to the existing durable Action owner;
they are not an execution log, lifecycle owner or source of replay authority.

Group only adjacent successful eligible Actions within the same correlated turn
and authority context. User/assistant messages, explicit commands, plan changes,
requests, writes, failures, cancellation, unknown outcomes and authorization
changes end a group. Do not collect distant operations into a synthetic block.
If turn/order/authority correlation is unavailable, do not group.

A group is a renderer projection over ordered Action references. Its members
retain separate statuses, results and detail selection. Updating one member
must not change another member's outcome or duplicate historical results.
Do not defer displaying a running operation while waiting for a possible group.
Only the uncommitted inline region can be recomposed; rows already in native
scrollback remain immutable. Further eligible work can form a new group.

Detail references include the concrete Process path and Action ID. A later
attachment can inspect an earlier observed member through its original AgentFS
files, with the current view generation fencing the asynchronous reply. Two
Processes with the same Action ID remain different selections. The detail header
keeps the selected Process visible while paging; absent files report unavailable
instead of substituting the current Process's result or an old cached body.

### Plan changes keep compact history and full snapshots

Each distinct plan update commits a small history record with available progress
and current-step information. Deduplicate identical observations, not genuinely
different snapshots. Retain the corresponding full snapshot through the existing
AgentFS UI/history evidence owner and extend existing detail navigation for plan
selection. A plan is not a fabricated Tool Action. Inspect available historical
snapshot evidence before extending any metadata; do not create a parallel log.

Opening an old plan shows that snapshot, not the latest plan. If unavailable,
show retention loss truthfully. Returning from details restores the same draft.
Keeping only a mutable latest plan was rejected because it loses the history
needed to understand progress and changed intent.

### Preserve ownership and history reconciliation

Runtime owns title, semantic eligibility, outcome and durable evidence;
AgentFS owns its exported files; Process owns lifecycle. The TUI owns hierarchy,
group projection and interaction. It must not parse Tool argument schemas,
execute work, infer success, or make provider calls to produce nicer summaries.

Hydration, partial drains, resizing and reconnection use existing correlation
and semantic history boundaries. Old structured records can be rendered in the
new style; opaque old text and already committed terminal scrollback are left
literal. Neither requires rewriting stored evidence or rerunning an operation.

## Risks / Trade-offs

- Less default detail can hide useful diagnostics → retain actual status,
  bounded failure evidence and discoverable per-item details.
- Grouping can obscure order or authority → group only eligible correlated
  neighbors and retain individual identities; missing evidence disables grouping.
- New row counts can break partial-history matching → verify drain, repeated
  identical input, resize and reconnect against semantic identities and literals.
- Plan snapshots may exceed current detail retention → use existing evidence
  ownership, retain immutable references where available and report gaps.
- Removing repeated hints can reduce discoverability → show the applicable
  shared hint and verify navigation from every supported detail entry point.

## Migration Plan

1. Freeze current behavior in representative fixtures and native terminal
   captures. Record source/binary/model/terminal identities and expected effects.
2. Implement shared label-free rendering, compact summaries and state hierarchy.
3. Add conservative read-only grouping and compact plan history with details.
4. Run regression and native acceptance, review the exact diff and required CI.
   Keep implementation tasks open until evidence exists.
5. After reviewed implementation merges, sync the deltas into canonical specs
   and archive only when all acceptance and delivery tasks are complete.

No durable data migration is intended. Optional metadata must be additive with
standalone fallback for older records. Reverting renderer changes must not
require restoring rewritten evidence or undoing Tool effects.

## Acceptance and later delivery order

For this UI, use five workflows: read/search bursts; successful and failing
commands; edit/diff; approval/cancel/resume; long output followed by a question.
Cover ordinary PTY and Herdr at 48, 80 and 120 columns, including Chinese text,
long paths, literal `xxx>` data, streaming, details and reconnect. Record both
correct output and effect counts; a screenshot alone cannot prove no replay.
Use fixtures for deterministic layout cases and native paths for interaction.

Then deliver Linux shell/git/Rust tooling as a separate change, preserving real
PATH order, selected version, required runtime files, isolated caches and grants.
Validate actual builds/tests with isolation and cancellation, not just version
output. Keep the existing fail-closed checks until a supported environment is
proved; never mount an entire home or silently substitute system tools.

After that, expand development qualification with five task families repeated
three times: small RED-to-GREEN bug fix, cross-file change, failure correction,
cancel/revoke/recover, and long-output follow-up. Freeze inputs and measure first
attempt success, human interventions, duration, correct artifacts and exact
effects. Cover supported macOS/Linux and both terminal hosts without counting
unsupported slots as passes. Add Node/package-manager and Python/venv cases
according to actual demand. This is a linked roadmap, not implementation scope
or new autonomous self-development evidence for this UI change.

## Open Questions

None at the product-decision level. Implementation must inspect the existing
metadata and snapshot retention paths before selecting the smallest compatible
extension. That engineering choice cannot relax the confirmed contracts.
