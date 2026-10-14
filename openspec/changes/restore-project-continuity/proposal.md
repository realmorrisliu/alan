# Proposal

## Why

A foreground Root should be able to continue project work across invocations without becoming a permanent global Agent. Existing durable evidence needs a bounded, inspectable project-oriented selection and summary experience.

## What Changes

- Discover authorized retained history and explicitly select a source for recovery.
- Keep project-scoped goal/progress/verification summaries inspectable, correctable and deletable, with source provenance.
- Revalidate current authority and remaining budgets; never replay completed effects or restore revoked grants from memory.
- Preserve independent foreground invocation lifetime and avoid automatic cross-project memory propagation.
- Replace only the bounded history-selection planning from the parked expose-agent-rollout-history draft; do not revive its clone/background launch path or proactive permission learning.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `evidence-retention-and-projection`: Authorized retained-history discovery and explicit selection.
- `runtime-memory-surfaces`: Inspectable project-scoped summaries and user control.
- `rust-inline-tui`: Explicit history/fresh-start selection and summary control presentation.
- `agent-namespace-runtime`: Recovery context remains distinct from authority and effects.

## Impact

Reuses Agent Runtime durable rollout/checkpoint evidence and Memory Stores. Depends on delegation for saved authority and budgets, not on full-trust mode. No new global Project runtime identity, background daemon or cross-device channel.
