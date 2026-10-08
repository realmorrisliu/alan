# Design

## Context

The existing Markdown projector tracks source-byte and expanded-character cuts.
The inline renderer uses that same projection for drawing and scrollback drain.
Prompt metadata already has authoritative model and queue observations.

## Goals / Non-Goals

Preserve those owners and stable input geometry. No new parser dependency,
syntax-highlighting engine, transcript state owner or runtime control changes.

## Decisions

- Transform fence marker atoms into compact boundary glyphs and style the existing
  info text as a language label. Keep source keys, literal body text and diff colors.
- Give one leading answer spacing row a stable cut before the first source byte,
  so the normal drain mechanism removes it once rather than regenerating it.
- Fit model and state first on narrow lines; omit lower-priority fields before
  truncating identity. Reuse existing detailed model/queue commands.

## Risks / Trade-offs

Streaming labels can grow after a partial cut; retain source keys and test
append/drain/resize/reconcile together. Very small widths cannot show every
field; preserve state and an explicit shortened model rather than an empty slot.

## Migration Plan

Renderer-only change. Rebuild the CLI and verify the actual Herdr pane; reverting
this slice restores presentation without changing persisted Agent state.
