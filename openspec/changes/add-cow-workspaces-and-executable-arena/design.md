# Design: Native OS CoW Workspaces and Executable `alan-arena`

## Context & Empirical Validation

On macOS APFS and modern Linux CoW filesystems (Btrfs / XFS), the operating system already provides copy-on-write cloning without a user-space filesystem daemon or FUSE/NFS kernel mount.

### Measured Local Benchmark (Apple Silicon macOS APFS, 2026-11-25)

Measured against this repository (`st_dev = 16777233`, including a `3.2 GB` Cargo `target/` directory and `.git/`):

| Operation | Measured Result | Notes |
| --- | --- | --- |
| Warm `cargo check -p alan-branchfs` in source | `6.14 s` | Baseline source tree |
| Full-tree `libc::clonefile(src, dst, 0)` | **`312.0 ms`** | Atomic full repo clone with `3.2 GB` `target/` + `.git/` |
| Full-tree `cp -c -R` (`st_dev` matched) | `5.43 s` | `df_avail_delta = -39.6 MiB` (near-zero disk copy) |
| Warm `cargo check -p alan-branchfs` in clone | **`0.39–0.46 s`** | `0` crates recompiled (100% cache hit at new path) |
| Edit 1 line in `crates/branchfs/src/lib.rs` + `cargo check` | **`0.81 s`** | `1` crate recompiled; zero dependencies rebuilt |
| Cleanup (`rmtree` of full clone) | `1.85–2.22 s` | Reclaims clone directory cleanly |

This eliminates the need for LayerFS's SQLite store or loopback NFS for build isolation while achieving sub-second workspace startup and native OS I/O throughput.

## Architecture Overview

```text
Host Mount Service (alan-os-host, owns real paths & CoW clones)
  ├── Primary Grant: /path/to/host/project (same APFS/Btrfs/XFS volume check)
  ├── Creates CoW clone under System Store/workspaces/<instance_id>/<workspace_id>/
  │     ├── macOS APFS: libc::clonefile(src, dst, 0) (EXDEV -> git worktree fallback)
  │     ├── Linux: FICLONE reflink walk (unsupported FS -> git worktree fallback)
  │     └── Captures lightweight BaseManifest (paths, sizes, hashes, default excludes)
  ├── Projects narrowed ReadWrite Host Mount into child Process (/mnt/work)
  └── Exposes ONLY opaque workspace_ref + diff stats + /mnt path to Ring 3/4

Plan 9 Coordination (/mnt/branch via alan-branchfs, zero raw Host paths)
  ├── branches/<id>: BranchRecord { base, workspace_ref, diff_summary, score, status }
  ├── ctl: fork / score / select / discard
  ├── selected: winning candidate id & provenance
  └── events: blocking NDJSON lifecycle stream

Consumers:
  ├── M8 Tier-3 Concurrent Writes: write_mode = "cow_workspace" (fallback: "worktree")
  └── Executable alan-arena (opt-in mode = "executable"):
        ├── Fail-closed OS sandbox gate (Seatbelt | LinuxReifiedNamespace | Landlock)
        ├── N parallel contenders (capped, default max = 4, network denied by default)
        ├── Independent read_only Verifier reads diffs, test exits, and RO candidate views
        ├── Immediate cleanup of discarded/losing clones
        └── Exit cleanup: exports unapplied winning unified .patch to System Store, removes clones
```

## Key Design Contracts

### 1. Host-Owned CoW Staging, Concurrency Cap & Fallback (`alan-os-host`)

- **Staging & Ownership**: CoW clones are materialized inside the Host-private `System Store/workspaces/<instance_id>/<workspace_id>/` tree (respecting `ADR-0050` and `ADR-0053`; never created inside the user's project directory).
- **Same-Volume Check & Fallback**: before cloning, Host Mount Service checks filesystem device compatibility (`st_dev`) and CoW support. On cross-volume staging (`EXDEV`) or non-CoW filesystems (such as tmpfs/ext4 without reflink), Alan falls back deterministically to `git worktree` (preserving clear provenance in `WorkspaceBacking::GitWorktree` vs `WorkspaceBacking::NativeCow`), or refuses cleanly when targeting a non-Git folder without CoW if explicit fallback is requested.
- **Full-Tree Isolation (`.git/` Protected)**: cloning the entire tree copies uncommitted dirty edits, `.git/`, and build caches (`target/`, `node_modules/`). Child subprocesses never receive write access to the Host's real `.git/` directory, closing the residual `.git` mutation exposure noted in `crates/agent-engine/src/tools/sandbox_backend.rs`.
- **Resource & Orphan Bounds**: each foreground `alan` instance enforces a configurable active clone cap (default `max_active_cow_workspaces = 4`) and sweeps stale crashed-instance directories under `System Store/workspaces/` on Host startup once PID locks are dead.

### 2. Lightweight Base Manifest & 3-Way Safe Sync-Back

- **Base Manifest at Fork Time**: when creating a CoW workspace, Host Mount Service records a lightweight manifest (`BaseManifest`) over source files while skipping default build/VCS exclusions (`.git/`, `target/`, `node_modules/`, `.venv/`, and project `.gitignore` patterns when appropriate, plus explicit exclusion overrides).
- **Explicit Sync-Back Only**: workspace mutations never sync back to the Host directory automatically when a child finishes.
- **3-Way Conflict Detection**: when the user or Root confirms `apply` on a selected workspace, Host Mount Service compares `(base_manifest, workspace_now, host_now)` per modified/deleted/added path:
  - if Host did not touch that path since clone creation, apply the workspace change;
  - if Host also modified, created, or deleted that path cleanly in conflicting fashion (`workspace_now != host_now && host_now != base`), **fail closed with a typed conflict list** instead of overwriting concurrent Host edits.
- **Non-Destructive Alternative Exports**: the user or Root can always export a clean unified `.patch` file (excluding `.git/` and build artifacts) or materialize the winning delta onto a fresh Git branch in the Host repository.
- **Intent Provenance (`workstream` + `rollout_ref` + `why`)**: exported unified `.patch` headers, System Store sync-back audit records, and optional Git branch commits attach lightweight plain-text provenance metadata (`workstream`, opaque `rollout_ref` / `turn_id`, and a concise 1-line `why` rationale—plus standard `Alan-Workstream:` / `Alan-Rollout:` trailers on opt-in Git branch commits). Operators can trace any code change back to its originating Agent rollout locally without CRDTs, cloud sync, or leaking raw Host backing paths.

### 3. `/mnt/branch` (`alan-branchfs`) Alignment

- `crates/branchfs` already provides `ctl`, `branches/<id>`, `selected`, and blocking `events` over `aP`, backed by `KnowledgeStore`.
- Rather than storing raw Host paths in `branchfs` (which would violate `ADR-0050` and `host-mount-service`), `BranchRecord` grows optional opaque workspace fields (`workspace_ref`, `backing_kind`, `diff_summary`, `checks_summary`) referencing Host-owned CoW clones.
- Fork/discard/select commands written to `ctl` stay authoritative at the file boundary while Host adapters handle physical clone creation, test execution, and directory deletion.

### 4. Executable `alan-arena` Mode & Security Guardrails

- **Read-Only Default Preserved**: invoking `alan-arena` without `mode = "executable"` continues to run strictly read-only architectural bakeoffs.
- **Fail-Closed OS Confinement Gate**: `mode = "executable"` requires `SandboxBackendKind::Seatbelt`, `LinuxReifiedNamespace`, or `Landlock`. When only `HostMountPathGuard` is available, Alan refuses `mode = "executable"` cleanly before creating any CoW workspace or spawning contenders.
- **Narrowed Contender Capabilities**: each contender receives a narrowed `ReadWrite` Host Mount covering strictly its own CoW workspace path, `network_denied` by default, zero Host Store secrets, and bounded `task_budget` / execution timeout.
- **Independent Read-Only Verifier**: the verifier subagent runs strictly `read_only = true`, inspecting candidate `diff_summary` patches, command/test exit codes, and read-only views of candidate trees to publish scores to `/mnt/branch`.
- **Exit Cleanup & Patch Persistence**:
  - losing/discarded contenders have their physical CoW clones deleted immediately upon verdict/discard;
  - on normal `alan` exit, any selected winning CoW workspace that has not yet been synced back to the Host automatically writes its verified unified `.patch` and scorecard summary into System Store rollout evidence, then deletes the physical CoW clone directory so multi-gigabyte `target/` clones are never left stranded.

## Risks & Mitigations

| Risk | Mitigation |
| --- | --- |
| Cross-volume staging causes `clonefile` `EXDEV` (`System Store` on separate mount from project) | Verify `st_dev` match upfront; support project-volume sibling staging or clean `git worktree` fallback |
| Concurrent user edits in Host directory collide with winning CoW workspace | Mandatory base manifest + 3-way conflict check on sync-back; fail closed with conflict list and offer `.patch` export |
| Large `target/` directories accumulate if `alan` crashes mid-bakeoff | Immediate deletion on discard, active clone cap (default 4), exit cleanup after `.patch` export, and startup dead-PID orphan GC |
| Contender runs unsafe commands when OS sandbox is absent | Fail closed if active backend is `HostMountPathGuard`; never silently downgrade executable arena to unguarded execution |
