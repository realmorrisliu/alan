# Design

## Context
See proposal.md. Existing Process-owned observations are sufficient. The renderer currently combines location, model binding qualifiers, execution state and queue details in one row, while a second activity row repeats running state.

## Goals / Non-Goals
Goals: a quiet default, explicit exceptions, stable completion and truthful active binding.
Non-goals: new authority, model selection changes, localization framework or Tool transcript redesign.

## Decisions
- Separate project/model context from conditional attention. Keep English UI wording consistent with the current product; use short actionable labels.
- Retained project receipt supplies the label/access; otherwise display observed namespace cwd without inventing a grant. /status includes cwd, receipt/access and existing detailed model/queue observations.
- Display selected-next while idle, active binding during running/paused work or a pending response. A differing next binding is an exception cue. Unknown observations stay explicit; known idle/zero queue disappears.
- One existing live-region layout owns blank row, rule, context, attention, notices and composer. Do not put separators into durable history. Keep existing Activity controls and queue ownership unchanged.
- Retain context-specific detail hints; do not add a permanent shortcut footer. Narrow widths sacrifice location detail before model; warnings wrap and remain reachable above the prompt.

## Risks / Trade-offs
Two boundary rows and conditional attention consume terminal space → include them in the same cursor/retention budget and exercise short/narrow panes. Unknown queue data must not look like known-empty → preserve an explicit warning for attached unavailable observations. Active/next divergence must not mislead → retain exceptional next-binding cue and full /status.

## Migration Plan
Renderer-only change; no stored-data migration. Revert the presentation patch to roll back. Native verification uses isolated stores and a fresh binary, never personal credential mutation.
