# runtime-memory-contract Specification

## Purpose
Defines runtime-memory contracts for workspace memory layout, recall and write
paths, compaction coordination, human-readable memory surfaces, provenance, and
truncation behavior.

## Requirements
### Requirement: Memory contracts live in OpenSpec
alan SHALL keep durable memory architecture, pure-text memory layout, recall,
write, compaction coordination, and memory-surface requirements in OpenSpec.

#### Scenario: Memory behavior changes
- **WHEN** a change modifies workspace memory layout, session summaries,
  working memory, handoff files, memory recall, memory write planning,
  promotion, or compaction coordination
- **THEN** the change updates `runtime-memory-surfaces`, `add-proactive-memory-v2`,
  this capability, or another named OpenSpec owner
- **AND** it does not create a new long-form memory contract in `docs/spec/`

#### Scenario: Legacy memory docs remain linked
- **WHEN** `docs/spec/memory_architecture.md` or
  `docs/spec/pure_text_memory_contract.md` is reached during migration
- **THEN** the file points to OpenSpec memory owners as a non-authoritative
  bridge

### Requirement: Memory surfaces remain human-readable and provenance-aware
alan SHALL keep memory and handoff surfaces readable, workspace-scoped, and
explicit about truncation or provenance when they summarize larger runtime
artifacts.

#### Scenario: Memory surface summarizes recent work
- **WHEN** alan writes current-goal, handoff, session, topic, or recall
  material
- **THEN** it preserves substantive user intent and relevant evidence
  references
- **AND** it avoids replacing the goal with low-information control messages

#### Scenario: Memory content is truncated
- **WHEN** memory or handoff content is shortened for readability or prompt
  safety
- **THEN** the truncation is coherent and points to the source rollout,
  evidence, or session context when available

### Requirement: Memory vocabulary and kinds are stable
alan SHALL model runtime memory as distinct working, episodic, semantic, and
procedural kinds rather than as one ever-growing memory file.

Stable terms:

- **Working memory**: runtime-owned, session-local state needed to continue the
  current task.
- **Episodic memory**: compact summaries of what happened in past sessions.
- **Semantic memory**: stable facts about the user, workspace, constraints,
  conventions, and reusable decisions.
- **Procedural memory**: behavior rules expressed through prompts, persona
  files, and skills rather than `.alan/memory/`.
- **Recall bundle**: a bounded source-labeled block injected into a turn after
  runtime retrieval.
- **Session summary**: curated markdown summary for one finished session.
- **Handoff**: the current cross-session continuation note optimized for what
  was happening most recently.
- **Topic page**: curated subject memory that accumulates durable facts and
  decisions for one recurring subject.
- **Inbox entry**: candidate memory observed but not yet fully promoted into
  stable memory.
- **Promotion**: moving a candidate or repeated observation into `USER.md`,
  `MEMORY.md`, or a topic page.
- **Write plan**: a structured model-produced proposal that runtime validates
  before any durable memory write.

Kind responsibilities:

1. L0 working memory preserves current goal, active subgoals, confirmed
   constraints, unresolved questions, pending verifications, recent findings,
   and active recall hits for the live session.
2. L1 episodic memory preserves handoffs, session summaries, and daily notes so
   alan can answer what happened, what was being done, and what remains.
3. L2 semantic memory preserves stable user facts, workspace facts, decisions,
   conventions, topic pages, and candidate inbox entries.
4. Procedural memory remains in prompts, persona, and skills and is not mixed
   into user identity or session history.

#### Scenario: Memory surface is classified
- **WHEN** runtime code, docs, or specs describe a memory asset
- **THEN** it classifies the asset using working, episodic, semantic, or
  procedural memory semantics
- **AND** stable identity/preferences are not mixed with chronological project
  status or behavior instructions

### Requirement: Memory kind is separate from memory authority
alan SHALL treat working, episodic, semantic, and procedural memory as
agent-cognitive memory kinds rather than ownership buckets. Ownership and access
authority SHALL be modeled separately by Memory Stores such as personal,
system-continuity, app, and workspace stores.

#### Scenario: Memory store is described
- **WHEN** runtime code, docs, or specs describe where memory is owned or who may
  authorize access
- **THEN** they use Memory Store language rather than redefining memory kinds as
  User Memory, System Memory, or App Memory
- **AND** Agent Runs access memory stores only through configured memory paths,
  Access Checks, Context Grants, or app-controlled memory surfaces

### Requirement: Pure-text memory layout is inspectable
alan SHALL keep the baseline memory system file-backed, pure-text,
workspace-scoped, and inspectable without vector databases, SQLite indexes, or
hidden provider memory.

This baseline layout is the current workspace-scoped compatibility Memory Store.
It may contain files such as `USER.md` for stable user-facing facts, but that
does not make every such file the future global Personal Memory Store. Future
Alan OS memory work must split store authority explicitly instead of inferring
authority from these compatibility filenames.

Target layout:

```text
.alan/
`-- memory/
    |-- USER.md
    |-- MEMORY.md
    |-- handoffs/
    |   `-- LATEST.md
    |-- working/
    |   `-- <session-id>.md
    |-- daily/
    |   `-- YYYY-MM-DD.md
    |-- sessions/
    |   `-- YYYY/MM/DD/<session-id>.md
    |-- topics/
    |   `-- <slug>.md
    `-- inbox/
        `-- YYYY/MM/DD/<entry-id>.md
```

Rules:

- `USER.md` stores stable user identity, preferences, and stable constraints
  only.
- `MEMORY.md` stores curated workspace-level semantic memory and topic indexes.
- `handoffs/LATEST.md` is rewritten to represent the most recent actionable
  continuation state.
- `working/<session-id>.md` is runtime-owned, session-local, and not stable
  cross-session memory.
- `daily/YYYY-MM-DD.md` is append-heavy chronology and compaction-preservation
  material, not a final stable memory index.
- `sessions/YYYY/MM/DD/<session-id>.md` is one curated summary per finished
  session.
- `topics/<slug>.md` holds dense subject-specific memory that would otherwise
  bloat `MEMORY.md`.
- `inbox/YYYY/MM/DD/<entry-id>.md` stages observed candidate memory with visible
  status.
- Raw rollout files remain the high-fidelity execution log and are not replaced
  by markdown memory files.

#### Scenario: Stable user preference is written
- **WHEN** alan promotes a durable user preference
- **THEN** it writes to `USER.md` with source context
- **AND** it does not store temporary project status or open TODOs in that file

#### Scenario: Topic material grows dense
- **WHEN** a recurring subject would make `MEMORY.md` long or hard to scan
- **THEN** alan moves the dense material into `topics/<slug>.md` and links it
  from `MEMORY.md`

### Requirement: Memory file contracts preserve required sections and provenance
alan SHALL keep memory markdown surfaces structured enough for deterministic
runtime use and human inspection.

Required `working/<session-id>.md` sections:

```markdown
# Working Memory

session_id: ...
updated_at: ...

## Current Goal
## Active Subgoals
## Confirmed Constraints
## Pending Verification
## Open Loops
## Recent Findings
## Active Recall
```

Required `handoffs/LATEST.md` sections:

```markdown
# Latest Handoff

session_id: ...
updated_at: ...

## Summary
## Current Direction
## Open Loops
## Important References
```

Required session-summary frontmatter and sections:

```markdown
---
session_id: ...
created_at: ...
workspace: ...
title: ...
tags: [ ... ]
entities: [ ... ]
source_rollout: ...
---

# Session Summary

## What Happened
## Key Decisions
## Durable Facts
## Open Loops
## References
```

Required topic frontmatter and sections:

```markdown
---
title: ...
aliases: [ ... ]
tags: [ ... ]
entities: [ ... ]
updated_at: ...
source_sessions: [ ... ]
---

# <Topic Title>

## Summary
## Stable Facts
## Key Decisions
## Open Questions
## References
```

Required inbox frontmatter and body:

```markdown
---
id: ...
kind: user_identity | user_preference | workspace_fact | topic_fact | workflow_rule
status: observed | confirmed | rejected | expired
target: USER.md | MEMORY.md | topics/<slug>.md
confidence: low | medium | high
created_at: ...
updated_at: ...
source_sessions: [ ... ]
---

## Observation
## Evidence
## Promotion Rationale
```

#### Scenario: Session summary is written
- **WHEN** a session finishes and alan writes a session summary
- **THEN** the summary includes title, tags, entities, source rollout, key
  decisions, durable facts, open loops, and references for lexical recall and
  audit

#### Scenario: Candidate memory is rejected
- **WHEN** a candidate inbox entry is rejected or expires
- **THEN** alan updates visible status instead of silently deleting evidence

### Requirement: Runtime owns session bootstrap and pre-turn recall
alan SHALL build memory bootstrap and recall bundles in runtime rather than
depending on model initiative or hidden provider memory.

Session bootstrap reads:

1. `USER.md`
2. `MEMORY.md`
3. `handoffs/LATEST.md`
4. newest `daily/YYYY-MM-DD.md` if present

Recall triggers include:

1. explicit references such as "remember", "last time", "previously", and
   equivalent user wording
2. identity or preference questions
3. references to previously discussed projects, people, or recurring topics
4. working-memory unresolved references likely answered by episodic or semantic
   memory

Recall routing order:

1. identity/preferences: `USER.md` and inbox entries targeting `USER.md`
2. current project or ongoing work: `MEMORY.md`, `handoffs/LATEST.md`, newest
   daily note
3. past-session recall: recent session summaries and daily notes
4. topic recall: `topics/*.md`, then session summaries mentioning matching
   aliases/entities
5. fallback: recent raw rollout grep only when curated markdown does not answer

Lexical search widening order:

1. exact file reads for known targets
2. exact phrase match
3. alias/entity/title match
4. tokenized OR-style lexical search
5. recency-biased fallback over recent summaries
6. raw rollout grep as last resort

Recall bundles are bounded, source-labeled, and informational background rather
than new user input.

#### Scenario: New session starts
- **WHEN** alan starts a new session with memory enabled
- **THEN** runtime injects a bounded bootstrap bundle from the required memory
  files without relying on the model to ask for them

#### Scenario: Recall bundle is injected
- **WHEN** runtime injects recalled memory into a turn
- **THEN** every recalled item is source-labeled and bounded
- **AND** the bundle is treated as background context, not as a new user
  instruction

### Requirement: Runtime validates model-mediated memory write plans
alan SHALL use model-mediated semantic judgment for automatic memory promotion
while keeping trigger timing, validation, provenance, and durable writes under
runtime authority.

Write-plan contract:

1. Runtime chooses when to invoke write planning and which active-turn messages
   are in scope.
2. The model returns bounded structured output with `kind`, canonical target,
   confidence, disposition, observation, evidence, and promotion rationale.
3. Runtime validates and canonicalizes the output before any file write.
4. Runtime remains the only component allowed to mutate memory files.
5. Invalid, mismatched, or over-broad candidates are dropped rather than
   written.
6. Low-confidence or ambiguous candidates fall back to inbox staging.

Direct stable writes require a validated `promote_now` disposition and at least
one of:

1. the user explicitly says to remember it
2. the user directly states the fact as stable identity, preference, or
   constraint
3. the user authorizes a source lookup that directly states the fact
4. the fact is already in stable memory and the new turn updates it

Observed captures that are useful but not stable enough become inbox entries or
daily notes rather than stable memory.

#### Scenario: Write plan is over-broad
- **WHEN** the model proposes a memory write that spans unrelated facts,
  mismatches its target, lacks evidence, or exceeds the bounded schema
- **THEN** runtime rejects or stages the candidate instead of mutating stable
  memory directly

#### Scenario: User asks alan to remember a stable preference
- **WHEN** a validated write plan marks the preference as `promote_now`
- **THEN** runtime writes the durable change and preserves source evidence

### Requirement: Session finalization and consolidation keep memory curated
alan SHALL write episodic memory at session end and periodically consolidate
stable memory so chronology does not overwhelm semantic surfaces.

At session end, runtime should:

1. finalize `working/<session-id>.md`
2. write `sessions/YYYY/MM/DD/<session-id>.md`
3. refresh `handoffs/LATEST.md`
4. append a concise entry to the current daily note
5. optionally stage inbox entries for unresolved but important observations

Consolidation responsibilities:

1. merge repeated daily/session facts into topic pages
2. promote confirmed inbox entries into `USER.md` or `MEMORY.md`
3. prune or resolve stale open loops in `LATEST.md`
4. keep `MEMORY.md` short by linking outward to topic pages
5. mark obsolete candidate entries as rejected or expired rather than silently
   overwriting history

Consolidation may run at explicit session end, during background maintenance,
before or after compaction, or on explicit user request.

#### Scenario: Session finalizes
- **WHEN** a session reaches finalization
- **THEN** alan preserves current working state into session summary, handoff,
  daily note, and optional inbox entries according to the memory-kind
  contracts

### Requirement: Memory and compaction remain adjacent but distinct
alan SHALL distinguish context-window compaction from durable memory
preservation.

Rules:

- Compaction protects the current session from context overflow.
- Memory preserves information beyond the current session.
- Automatic pre-compaction memory flush primarily targets daily-note
  preservation and candidate inbox staging.
- Pre-compaction flush must not be treated as the only memory write path.
- Pre-compaction flush must not write directly into `USER.md`.
- Confirmed-turn promotion targets stable memory only after runtime validates a
  write plan.
- Neither automatic memory path relies on brittle punctuation or phrase
  splitting as the source of truth for semantic memory decisions.

#### Scenario: Soft-threshold compaction approaches
- **WHEN** runtime performs a pre-compaction memory flush
- **THEN** the durable output lands in daily-note or candidate staging surfaces
  with provenance
- **AND** stable user memory is not mutated without validated promotion

### Requirement: Memory remains auditable, private, and rollout-linked
alan SHALL preserve textual source trails and avoid silently absorbing sensitive
or unsupported data into stable memory.

Audit and privacy rules:

1. Tape and rollout remain the high-fidelity execution record.
2. Session summaries, handoffs, and topic pages are curated projections derived
   from rollout and tape evidence.
3. Retrieval prefers curated text before raw rollout.
4. Raw rollout grep is a fallback, not the primary recall mechanism.
5. Stable memory does not silently absorb sensitive data.
6. User-facing identity memory favors explicit confirmation or user-authorized
   source verification.
7. Rewrites and deletions leave a visible trail through summaries, frontmatter
   status, or source references.

#### Scenario: Sensitive fact appears in a turn
- **WHEN** a turn contains potentially sensitive information without stable
  memory intent or confirmation
- **THEN** alan avoids silently promoting it into stable memory and preserves
  only appropriate provenance or candidate state
