# Disposition

Status: planned (CoW workspace substrate for M8 Tier-3 parallel writes and Executable `alan-arena`; Track B macOS `aP -> loopback NFSv3` namespace reification split into a separate follow-up GitHub Issue).

- This change consolidates the Q1–Q7 alignment decisions from the 2026-11-25 `layerfs` evaluation and `/grill-me` review:
  1. **Native OS CoW over imported LayerFS runtime**: adopt macOS APFS `clonefile(2)` and Linux `FICLONE` reflinks (with `git worktree` fallback when CoW is unavailable), preserving 100% native I/O throughput, zero standing daemons, and zero LayerFS control-plane coupling;
  2. **Unified CoW Workspace primitive**: power both M8 Tier-3 concurrent long-running write workstreams (`add-project-workstreams`) and `alan-arena` Executable Mode (`mode = "executable"`) over `/mnt/branch` (`alan-branchfs`);
  3. **Full-tree clone + explicit 3-way safe sync-back**: protect Host `.git/`, preserve dirty uncommitted edits and build caches (`target/`, `node_modules/`), exclude `.git/` and build caches on sync-back, and fail closed on concurrent Host modifications;
  4. **Strong OS sandbox gate + clean exit**: require active OS confinement (`Seatbelt`, `LinuxReifiedNamespace`, or `Landlock`) for Executable `alan-arena`, persist clean `.patch` evidence on exit, and delete physical CoW clones so multi-gigabyte directories never accumulate;
  5. **Scoped deferrals**: per-turn `/rewind` CoW time-travel stays deferred to Wave 2, and macOS zero-root `aP -> loopback NFSv3` namespace reification is tracked independently as a follow-up GitHub Issue.
