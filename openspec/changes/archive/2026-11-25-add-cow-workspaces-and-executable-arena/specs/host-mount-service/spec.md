# Spec Delta

## ADDED Requirements

### Requirement: Host Mount Service clones and syncs CoW workspaces safely
Host Mount Service SHALL create isolated full-tree workspaces using native OS copy-on-write (`clonefile` on macOS APFS, `FICLONE` on Linux, or `git worktree` fallback) under Host-private staging, keep raw clone paths out of logical records, and apply changes back only via explicit 3-way conflict checks excluding `.git/` and build caches.

#### Scenario: Full-tree CoW clone isolates host git state and reuses build caches
- **WHEN** Host Mount Service forks a concurrent or speculative workspace for an authorized project
- **THEN** it creates an isolated full-tree CoW clone under Host-private storage and grants a narrowed `ReadWrite` mount to only that clone
- **AND** child writes to `.git/` or source files inside the clone leave the Host repository untouched while retaining cloned build caches

#### Scenario: Concurrent Host edit fails sync-back closed without overwriting
- **WHEN** an operator confirms sync-back from a CoW workspace after a tracked file was modified differently on the Host directory
- **THEN** Host Mount Service aborts sync-back with a typed conflict list and leaves Host files unmodified
- **AND** clean unified patch export remains available without losing workspace evidence
