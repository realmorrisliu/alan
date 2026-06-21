# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the
codebase.

## Before exploring, read these

- `CONTEXT.md` at the repo root if it exists.
- `docs/adr/` if it exists, reading ADRs that touch the area about to be changed.
- `openspec/specs/` for Alan's long-lived contracts and accepted behavior.
- `openspec/changes/` for active proposals, task lists, and spec deltas related to the current
  work.

If any of these files or directories do not exist, proceed silently. Do not flag their absence or
suggest creating them upfront. The domain-modeling workflows create domain docs lazily when terms
or decisions actually get resolved.

## File structure

This repo uses a single-context layout:

```text
/
├── CONTEXT.md
├── docs/adr/
└── openspec/
    ├── specs/
    └── changes/
```

Do not assume a `CONTEXT-MAP.md` multi-context layout unless that file is introduced later.

## Use the glossary's vocabulary

When output names a domain concept in an issue title, refactor proposal, hypothesis, or test name,
use the term as defined in `CONTEXT.md` when that file exists. Do not drift to synonyms the glossary
explicitly avoids.

If the concept needed is not in the glossary yet, either reconsider whether the project already uses
different language or note the gap for domain modeling.

## Flag decision conflicts

If output contradicts an existing ADR or OpenSpec contract, surface it explicitly instead of silently
overriding the prior decision.
