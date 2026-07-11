---
name: memory
description: |
  Manage persistent Memory Store files across Agent Processes.

  Use this when:
  - Continuing work started by an earlier Agent Process
  - The user references prior decisions, constraints, or history
  - Stable, user-confirmed information should be retained
  - Durable handoff or topic notes need inspection or maintenance

metadata:
  short-description: Persistent memory across Agent Processes
  tags: [memory, persistence, continuity, context-management]
capabilities:
  required_tools: [read_file, write_file, edit_file, bash]
---

# Memory

Alan's current memory surface is pure text under the active workspace's
channel-scoped runtime directory:

```text
.alan/runtime/<channel>/memory/
├── USER.md
├── MEMORY.md
├── handoffs/LATEST.md
├── daily/YYYY-MM-DD.md
├── episodic/YYYY/MM/DD/process-<hash>.md
├── working/process-<hash>.md
├── topics/<slug>.md
└── inbox/<candidate>.md
```

Resolve the exact active memory directory from runtime context. Do not infer an
unscoped fallback path.

## Ownership

- `USER.md`: stable, user-confirmed identity and preferences only.
- `MEMORY.md`: curated workspace facts and durable decisions.
- `handoffs/LATEST.md`: compact continuation state for a later Agent Process.
- `daily/`: chronological work notes and automatic pre-compaction flushes.
- `episodic/`: durable Process-scoped execution summaries.
- `working/`: current Agent Process working continuity.
- `topics/`: reusable subject-specific knowledge.
- `inbox/`: candidates awaiting deliberate promotion.

## Start of work

Prefer the memory bootstrap already injected into the prompt. Read files only
when exact on-disk verification, editing, or deeper history is needed. Start
narrowly with `USER.md`, `MEMORY.md`, `handoffs/LATEST.md`, the newest daily
note, or a relevant topic page.

## Persisting memory

Persist only information that is useful beyond the current Process:

- explicit decisions and durable constraints;
- verified workspace conventions and stable references;
- unfinished work that must survive a Process boundary;
- user-confirmed preferences the user asked Alan to remember.

Do not persist inferred personality, speculative summaries, transient chatter,
secrets, credentials, or large raw tool output.

When significant work changes durable project state, refresh
`handoffs/LATEST.md` with the current goal, completed work, open loops, next
steps, and concrete rollout or file references. Use topic pages when a subject
outgrows the curated index.

Automatic pre-compaction flushes may append a structured daily entry and an
inbox candidate. Review candidates before promoting them to `MEMORY.md` or a
topic page.
