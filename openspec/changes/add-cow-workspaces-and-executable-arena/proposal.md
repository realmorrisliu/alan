# Proposal: Native OS CoW Workspaces and Executable `alan-arena`

## Problem

Evaluating `Ephemeral-AI-Lab/layerfs` against Alan's architecture surfaced two immediate gaps in Alan's concurrent and speculative coding workflows:

1. **M8 Tier-3 write isolation loses build caches and leaves `.git` shared**: under [ADR-0062](../../../docs/adr/0062-project-workstreams-ledger-and-stateful-cron-streams.md), concurrent long-running write workstreams rely on `git worktree`. Every new `git worktree` drops ignored build caches (`target/`, `node_modules/`), omits uncommitted local edits, only works inside Git repositories, and shares mutable host `.git` internals with child processes.
2. **`alan-arena` is read-only design comparison only**: the preinstalled `alan-arena` skill (`add-personal-capability-extensions`) enforces `read_only = true` on all contenders, preventing evidence-backed bakeoffs where parallel contenders actually modify code, run compilers/tests in isolation, and get adjudicated by an independent read-only verifier on real diffs and exit codes. Meanwhile, `crates/branchfs` (`/mnt/branch`) currently stores only in-memory `alan-knowledge` DAGs rather than real code workspace candidates.

At the same time, importing LayerFS wholesale would violate Alan's architecture and performance needs: its `control/` layer duplicates Service Manager, System Store, Host Mount Service, and OS sandboxes, while its loopback NFS / SQLite store incurs ~0.31x–0.40x write throughput overhead and lacks GA durability guarantees.

## Proposal

Adopt LayerFS's strongest architectural ideas—ephemeral Copy-on-Write full-tree workspaces, conflict-checked branch convergence, and speculative multi-branch bakeoffs—on top of **native OS Copy-on-Write primitives**:

1. **Native OS CoW Workspace Substrate (`alan-os-host` / Host Mount Service)**:
   - Clone full project trees (including uncommitted edits, `.git/`, and `target/`/`node_modules/`) via macOS APFS `clonefile(2)` or Linux `FICLONE` reflinks into an instance-scoped directory under `System Store/workspaces/`, falling back to `git worktree` across volumes (`EXDEV`) or unsupported filesystems.
   - Bind each child Process with a narrowed read-write Host Mount covering only its assigned clone, keeping raw Host paths hidden from Ring 3/4 (`ADR-0050`).
   - Require explicit confirmation and 3-way conflict detection before syncing changes back to the Host directory (excluding `.git/` and build/cache paths by default), or exporting a clean unified `.patch` / Git branch.
2. **Plan 9 `/mnt/branch` (`alan-branchfs`) Coordination**:
   - Connect `alan-branchfs` to opaque `workspace_ref` metadata, diff summaries, explicit verifier scores, selection state, and blocking `events`.
3. **M8 Tier-3 & Executable `alan-arena` Consumption**:
   - Upgrade M8 Tier-3 concurrent writes to prefer native CoW workspaces (falling back to `git worktree`).
   - Add opt-in `mode = "executable"` to `alan-arena`, strictly gated behind active OS confinement (`Seatbelt` / `LinuxReifiedNamespace` / `Landlock`), with immediate losing-clone cleanup and exit-time `.patch` evidence retention + clone removal.

## Non-Goals

- Importing or hosting LayerFS's control plane (`~/.layerfs`, Lima/Docker VMs, daemon RPCs, or Node MCP server).
- Using a Disposable SQLite store as primary storage for user files.
- macOS loopback NFSv3 `aP` namespace reification (tracked separately as an independent follow-up Issue).
- Automatic per-turn `/rewind` CoW checkpointing (deferred to Wave 2).
- Silent overwrites of concurrent Host edits during workspace sync-back.
