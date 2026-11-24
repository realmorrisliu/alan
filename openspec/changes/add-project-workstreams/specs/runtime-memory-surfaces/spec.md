# Spec Delta

## ADDED Requirements

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
