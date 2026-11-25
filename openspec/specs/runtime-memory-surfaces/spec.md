# runtime-memory-surfaces Specification

## Purpose
Define runtime memory and handoff surface behavior: agent-authored semantic
continuation summaries, substantive fallback goal selection, coherent fallback
truncation, terminal turn-state refresh, and rollout references when compact
surfaces omit detail.
## Requirements
### Requirement: Rollout Remains Source Of Truth
Memory surfaces SHALL stay compact continuation aids and point to rollouts when detail is omitted.

#### Scenario: Detail exceeds memory budget
- **WHEN** recent conversation detail exceeds the memory surface budget
- **THEN** the memory surface keeps a coherent summary and identifies where to inspect the raw rollout for full detail

### Requirement: Runtime memory surfaces use Agent Process evidence
Current-goal, semantic-memory, fallback, handoff, and daily-note surfaces SHALL derive from Agent
Machine plan state, substantive turns, Agent Process activity, and rollout/checkpoint or file
evidence.

#### Scenario: Memory surfaces refresh after terminal state
- **WHEN** a turn or Agent Process changes durable project state and reaches a known terminal state
- **THEN** generated memory surfaces refresh from that state and its evidence
- **AND** truncation provenance names the source Process, rollout/checkpoint, or file

### Requirement: Project continuity summaries are scoped and user-controlled
Alan SHALL offer bounded project summaries of goals, completed/pending work and verification linked to retained evidence. Users SHALL be able to inspect, correct and delete them. Summary deletion SHALL remove the derived material from recall without silently regenerating it from the same old sources. Durable execution evidence SHALL remain distinct and its retention SHALL be explained. Summaries MUST NOT grant authority or automatically propagate between projects.

#### Scenario: User corrects summary
- **WHEN** a user corrects a generated project summary
- **THEN** the new summary retains user/source provenance and older generated material cannot silently overwrite it

#### Scenario: User deletes summary
- **WHEN** the user deletes derived project continuity material
- **THEN** it is excluded from recall while source evidence is retained separately and explained

#### Scenario: Another project is selected
- **WHEN** the user starts work in a different project
- **THEN** the first project’s summary is not automatically inserted or converted into authorization

### Requirement: Shared project memory enforces read-only child access and Root-owned proposal commits
Child workstreams SHALL mount the bound project's `alan-memfs` Store read-only for BM25 and CJK recall (`recall/<query>`) and SHALL NOT mutate shared project memory directly. Child handoffs MAY include structured `memory_proposals` carrying rollout provenance, which Root SHALL secret-scrub, deduplicate, and commit atomically to `alan-memfs` while preserving user `/summary` inspect, edit, delete, and revision rollback controls.

#### Scenario: Child reads shared memory without direct write access
- **WHEN** a child workstream queries `/mnt/mem/recall/<query>`
- **THEN** it receives ranked shared project memory hits
- **AND** any direct write attempt by the child against shared project `memfs` is denied

#### Scenario: Root commits child memory proposals atomically with provenance
- **WHEN** a child workstream completes with `memory_proposals` referencing its rollout
- **THEN** Root scrubs secrets, deduplicates against existing entries, and commits accepted proposals as a new `memfs` revision with provenance
- **AND** the user can inspect, correct, delete, or roll back the committed summary via `/summary`

### Requirement: Project Inbox surfaces unread scheduled workflow digests
Completed project-bound `alan workflow` runs SHALL append bounded, secret-scrubbed `InboxDigestEntry` records to the project's System Store `inbox/` directory. Root SHALL present unread inbox summaries upon opening the bound project and mark entries digested once acknowledged or dispatched into a workstream.

#### Scenario: Root presents and digests unread workflow inbox entries
- **WHEN** a user starts `alan` in a bound project with unread entries in `inbox/`
- **THEN** Root displays a concise summary of unread workflow digests on startup
- **AND** once the user dispatches or acknowledges an entry, its `digested_at` timestamp is recorded so it is not repeated as unread
