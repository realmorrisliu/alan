# Tasks

## 1. Phase 1 — Native OS CoW Workspace Substrate (`alan-os-host`)

- [ ] 1.1 Implement native OS CoW workspace lifecycle in `alan-os-host` / Host Mount Service (`clonefile(2)` on macOS APFS, `FICLONE` reflink on Linux Btrfs/XFS, `git worktree` fallback on `EXDEV`/unsupported FS), staged under `System Store/workspaces/` with `st_dev` verification, active clone cap (`max_active_cow_workspaces = 4`), and startup dead-PID orphan GC.
- [ ] 1.2 Implement lightweight `BaseManifest` capture at clone time (excluding `.git/`, `target/`, `node_modules/` on sync-back), explicit 3-way conflict-checked sync-back (`base` vs `workspace` vs `host_now`), and unified `.patch` / new-Git-branch export helpers attaching `workstream` + `rollout_ref` + `why` intent provenance headers (and optional `Alan-Workstream:` / `Alan-Rollout:` commit trailers).
- [ ] 1.3 Add unit/integration tests covering `.git/` isolation from child writes, APFS/reflink cloning, `EXDEV`/non-CoW fallback, concurrent Host modification conflict detection (fail-closed), and clean non-Git directory support.

## 2. Phase 2 — Plan 9 `/mnt/branch` (`alan-branchfs`) Coordination

- [ ] 2.1 Extend `BranchRecord` and `BranchCommand` in `crates/branchfs` with opaque `workspace_ref`, `backing_kind`, `diff_summary`, and `checks_summary` metadata while enforcing zero raw Host paths in Ring 3/4 records (`ADR-0050`).
- [ ] 2.2 Connect `alan-os-host` CoW workspace creation, scoring, selection, and immediate discard deletion to `/mnt/branch` lifecycle events.

## 3. Phase 3 — M8 Tier-3 Integration & Executable `alan-arena`

- [ ] 3.1 Wire `write_mode = "cow_workspace"` into M8 Tier-3 concurrent write isolation (`add-project-workstreams` Slice 3), preserving single-writer `shared_queue` for routine edits and `git worktree` fallback when native CoW is unavailable.
- [ ] 3.2 Extend `crates/service-manager/packages/alan-arena/` and `spawn_subagent` orchestration with opt-in `mode = "executable"`, fail-closed OS sandbox gating (`Seatbelt`, `LinuxReifiedNamespace`, or `Landlock`), narrowed single-clone `ReadWrite` contender mounts, and `read_only = true` Verifier scoring.
- [ ] 3.3 Implement exit-time cleanup: immediately purge discarded candidate clones and flush unapplied winning `.patch` + scorecard evidence to System Store before deleting physical CoW clone directories.

## 4. Verification, Review, and Archive Readiness

- [ ] 4.1 Verify `just test`, `just quality`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `bash scripts/check-openspec-current-surfaces.sh`, and `openspec validate add-cow-workspaces-and-executable-arena --strict`.
- [ ] 4.2 Complete security and architecture review against `ADR-0050`, `ADR-0060`, `ADR-0061`, and `ADR-0062`.
- [ ] 4.3 After implementation PRs merge on `main` and CI passes, sync the delta specs into `openspec/specs/` and archive this change under `openspec/changes/archive/YYYY-MM-DD-add-cow-workspaces-and-executable-arena/`.
