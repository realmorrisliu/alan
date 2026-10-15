# Design

## Context

Explicit --resume and durable no-repeat controls already ship. This delivery adds
bounded project-oriented discovery and continuity, not a second recovery system.
The older expose-agent-rollout-history draft is parked and includes rejected
renderer launch exceptions; only its user need for inspectable history is retained.

## Goals / Non-Goals

Make project continuation understandable through ordinary terminal use. Root stays
instance-local. Background tasks, proactive cross-project memory, universal fork/
replay and new durable Process identity are excluded.

## Decisions

An explicitly selected project context references existing resource bindings and
Memory Store paths. It is not a Kernel object, runtime registry or authority source.
Use an explicitly selected opaque continuity reference. Different worktrees or
checkouts do not share summaries merely because their path or Git remote looks
similar; the user explicitly chooses reuse, with fresh independent Host authority.
This reference names stored context, not a globally executable Project object.
Agent Runtime Service presents authorized durable records through files; the TUI
selects and displays references. Do not scan raw Host backing in the renderer or
let cwd imply historical authority. Discovery is bounded/paged and isolates bad
records so one malformed rollout cannot hide all valid history.

Selecting a source shows goal, source provenance, known completion/wait/failure and
verification context. Existing --resume remains an explicit user selection; new
history selection extends its owning recovery path. Start fresh remains available.
A new Root validates current grants and budgets. Completed effects remain evidence,
unknown prior effects retain explicit resolution, and pending work is restored
paused rather than blindly replayed. Selecting a summary alone is not resume.

Keep a bounded generated summary per selected project context: goal, completed
work, open work, verification and evidence references. Runtime owns validated
publication into the existing Memory Store; generated content cannot modify grants,
reviewer policy or trust posture. Do not silently promote a task observation into
a global preference. Root replacement does not transfer data to another project.

User controls inspect, correct or delete the derived summary. Corrections carry
user provenance; deleted summaries are excluded from recall and are not silently
regenerated from the same old evidence until explicit regeneration or a new task
produces fresh material. Deleting a summary does not rewrite durable execution
records or claim secure deletion of every historical source; explain that distinction.
Conflicting user corrections outrank older generated summaries, without editing the
original rollout. Secrets are excluded from summaries and native-path projection
rules remain in force. No model-authored credential or permission persistence.

## Risks / Trade-offs

- History confused with live state → display provenance and recreate Process-local state.
- Summary becomes authority → resolve grants from current owners, never summary text.
- Corrupt/large records block selection → bounded reads and per-record diagnostics.
- Deletion appears ineffective → honor summary deletion in recall and explain retained evidence.
- Concurrent summaries conflict → retain source revision and detect conflicting edits.

## Migration Plan

Do not reactivate parked background-launch tasks. Extend existing explicit recovery
with backward-readable records and explicit missing-evidence states. No migration
of old user installations is required for this personal-development delivery.
