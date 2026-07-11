## Context

Alan already has pure-text workspace memory, turn-end promotion, recall bundles,
and pre-compaction flushes. The accepted architecture now separates memory kind
(working, episodic, semantic, procedural) from authority (Personal,
System-Continuity, App, Workspace Memory Stores). Memory Stores are file-server
trees posted through `/srv/mem`, mounted or descriptor-passed under `/mnt/mem`,
and backed by `alan-memfs`/content-addressed storage.

The current engine already writes channel-scoped pure-text memory. This change
moves durable write authority behind the Workspace Memory Store so every client
uses the same namespace-visible transaction and audit surface.

## Goals / Non-Goals

**Goals:**

- Proactively propose stable facts from direct statements, repeated behavior,
  and external evidence.
- Keep every durable mutation inspectable, redacted, source-linked, and
  reversible through the owning Memory Store tree.
- Preserve pure-text memory documents and the current channel-scoped workspace
  layout as the store backing tree.
- Keep normal turns low-disturbance while making review available to any
  authorized file client.
- Make store authority and namespace access explicit.

**Non-Goals:**

- Add vector search, graph storage, SQLite, or provider-owned memory.
- Let the model write stable memory directly.
- Put memory semantics in Alan Kernel.
- Add a second memory control plane outside the mounted store.
- Define the final Alan for macOS memory-review UI.
- Finish every Personal/System/App store layout in this slice.

## Decisions

### 1. Planning and commit are separate processes

Agent Execution Engine may decide when to ask for semantic candidate planning
and may validate the bounded response schema. It writes a proposal document into
the selected writable Memory Store. The Memory Store validates the target,
rights, current content, dedupe, redaction, and transaction preconditions before
committing the target document and ledger record atomically.

The engine therefore remains responsible for cognition timing, not storage
authority. A model response is a suggestion; the mounted store decides whether
it becomes durable state.

### 2. The Memory Store exposes transactions as files

Each writable store exposes a service-owned surface equivalent to:

```text
/mnt/mem/<store>/
├── memory/                 # store-owned pure-text documents
├── writes/
│   ├── events              # ordered proposal/commit/revert stream
│   └── <write-id>/
│       ├── proposal        # whole document, commit on clunk
│       ├── status
│       ├── result
│       └── ctl             # cancel/retry before commit; revert after commit
└── ledger/
    └── YYYY/MM/<write-id>.md
```

The exact store segment is selected by the spawner's mount layout; the contract
does not require globally visible store ids. `proposal` carries observation,
target namespace path, evidence class and references, confidence, disposition,
and rationale. Successful commit creates or updates the target and ledger as one
store operation. The store retains `writes/<write-id>/` for the write lifecycle;
after commit its `ctl` accepts `revert`, while the dated ledger record remains a
read-only audit document.

Alternative considered: make a CLI-specific write API authoritative. Rejected:
that makes the namespace a secondary projection.

### 3. Store authority follows ownership, not memory kind

Personal, System-Continuity, App, and Workspace stores may each contain working,
episodic, semantic, or procedural material. A proposal targets one mounted store
and a path inside it. The engine cannot redirect a write to an unmounted store,
and a store cannot infer authority from filenames such as `USER.md`.

The current `.alan/runtime/<channel>/memory/` layout is the Workspace Memory
Store backing tree. Its adapter projects files into the workspace store mount;
raw host paths are debug metadata, never the agent-facing reference.

### 4. Ledger and revert belong to the store

Every committed stable mutation receives a write id and a Markdown ledger record
containing target namespace path, inserted anchor/range, normalized observation,
confidence, bounded evidence references, rationale, timestamps, redaction
summary, and revert state. `/mnt/mem/<store>/writes/<write-id>/ctl` accepts
`revert` after commit; the store verifies the recorded anchor and commits target
plus ledger state atomically. The dated
`ledger/YYYY/MM/<write-id>.md` path is read-only and points back to the retained
write transaction id.

`alan memory recent|show|revert` merely walks, reads, or writes these files. A
future UI does the same. Mount visibility and access rights authorize the
operation.

### 5. Sensitive data is rejected or marked before persistence

The store enforces a last mandatory redaction boundary before any durable target,
staging, daily note, proposal result, or ledger evidence is committed. It may
record a bounded redacted fact, but never plaintext secrets. Redaction markers
and reason classes remain distinct from truncation.

### 6. Reverted facts disappear from prompt-facing views

Successful revert removes or safely reverses the target block. If concurrent
manual edits prevent precise revert, the transaction becomes
`manual_resolution_required` and does not apply a risky patch. Runtime recall and
handoff readers consume the current store tree and exclude any store-marked
reverted or tombstoned content.

### 7. Disabled memory means no writable store capability

When memory is disabled, Agent Execution Engine does not schedule proactive
planning and its namespace receives no writable proactive-memory transaction
surface. Read-only memory may be mounted only if separately configured. This is
stronger and more inspectable than allowing a writer and asking it not to write.

## Risks / Trade-offs

- [Risk] Store transaction semantics are more work than direct filesystem writes
  → Mitigation: one transaction surface provides atomic target+ledger updates,
  revert, and events for all clients.
- [Risk] Workspace backing files and mounted paths drift → Mitigation: the
  adapter owns translation and tests path containment plus namespace resolution.
- [Risk] Model proposals over-promote facts → Mitigation: bounded schema,
  evidence classes, confidence rules, staging, and store-side validation.
- [Risk] Manual edits prevent precise revert → Mitigation: anchor/hash checks and
  explicit `manual_resolution_required` status.
- [Risk] Redaction misses a secret → Mitigation: validate proposal content and
  referenced evidence again at the durable store boundary.

## Migration Plan

1. Add the store-owned proposal, result, ledger, event, and revert surfaces to
   `alan-memfs` without changing current promotion behavior.
2. Project the current workspace `.alan/runtime/<channel>/memory/` layout
   through the Workspace Memory Store adapter.
3. Route existing runtime promotion and flush candidates through store proposal
   files; remove direct durable writes from engine code.
4. Add file-client CLI inspection and revert.
5. Enable proactive direct-statement, repeated-behavior, and external-evidence
   candidates after redaction and revert tests pass.
6. Add explicit Personal/System/App store layouts in focused follow-up changes.

## Open Questions

- Whether `writes/<id>/proposal` ids are allocated by create or a clone-via-open
  helper; both must remain ordinary aP operations.
- Which Personal and System-Continuity store trees are mounted by default into
  the Root Agent Process; this change does not grant them implicitly.
