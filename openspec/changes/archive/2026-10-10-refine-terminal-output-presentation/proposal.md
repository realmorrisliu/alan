## Why

Alan Shell currently exposes renderer-generated role labels such as `tool>`,
`plan>`, `input>`, `error>` and expanded `thinking>`. Repeated titles, paths and
detail hints make routine work visually compete with answers and decisions.
The user confirmed a complete removal of this label style and a calmer,
action-oriented presentation on 2026-10-09.

## What Changes

- Remove every renderer-generated `xxx>` role label from terminal presentation,
  including live state, details and re-rendered retained history. Preserve
  literal content, structured identity and the existing `:` / `!` input routes.
- Use concrete actions and authoritative outcomes, bounded summaries, readable
  errors and contextual detail discovery instead of fixed repeated scaffolding.
- Group adjacent successful, explicitly known read-only operations while keeping
  every Action and its retained details individually inspectable. Writes,
  failures, requests and unknown operations remain distinct.
- Retain a short permanent record for each plan change and access to its full
  snapshot, rather than repeatedly printing the complete plan.
- Validate the whole output inventory against real terminal workflows, including
  stable input, narrow widths, reconnect and usable scrollback.
- Record the decision in ADR-0059 and the interview in `decision-record.md`.
  Follow the archived `refine-terminal-readability` delivery without reopening it.

## Capabilities

### New Capabilities

None. Reuse existing presentation and renderer owners.

### Modified Capabilities

- `rust-inline-tui`: label-free semantic presentation and compact, retained plan
  updates across terminal surfaces.
- `tool-result-presentation`: compact action-oriented summaries, read-only
  grouping eligibility and truthful detail disclosure.
- `alan-renderer-host-contract`: per-Action identity and update isolation within
  a grouped presentation, including hydration and history reconciliation.

## Impact

Primarily `crates/tui`, its fixtures and native terminal acceptance. Runtime-owned
presentation metadata in `crates/agent-engine` and `crates/agent-protocol` may need
a narrowly scoped additive change if existing metadata cannot prove grouping
eligibility. The TUI must not interpret Tool arguments or guess from titles.
No new dependency, execution path or presentation state service is planned.

Linux toolchain support and broader repeated development qualification are
separate follow-up deliveries, ordered in `design.md`; this change uses real
workflows only to accept its UI. Automatic input routing remains disabled.
