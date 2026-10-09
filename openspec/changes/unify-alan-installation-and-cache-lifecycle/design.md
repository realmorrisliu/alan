# Design

## Context

Baseline inspected: `498f97e9f3604bfbc4462ea5a93155736d2591de`, Rust/Cargo 1.97.0. See proposal.md for motivation.

| Existing surface | Relevant finding |
| --- | --- |
| `crates/agent-engine/src/install_channel.rs`, `crates/auth/src/chatgpt.rs` | Duplicate environment/argv channel selection; Host policy leaks into engine/auth defaults. |
| `crates/os-host/src/paths.rs`, `local.rs`, `boot.rs` | Channel suffixes appear in durable paths, endpoint layout/status and boot construction. |
| `scripts/install-channel.sh`, installer/uninstaller, release assembly, `justfile` | Separate names/manifests and release alias; installer already has digest preflight and rollback worth retaining. |
| `scripts/check-quality.sh` | Owned `target/quality-gate` plus explicit host target is a correctness boundary required by the quality spec. |
| `scripts/install-cli.sh`, `assemble-cli-release.sh` | Another default target tree even for compatible ordinary builds; real cross-target release builds still need their distinct Cargo layout. |
| `crates/agent-engine/src/skills/mod.rs`, `crates/service-manager/src/runtime.rs` | Static per-PID extraction feeds both seeding and an ID-only enumeration, so merely asking for package IDs can materialize files. |
| `crates/service-manager/src/package/materializer.rs` | Existing snapshot entries, bounds, validation, digest and immutable revision storage can own embedded content without a Host-directory round trip. |
| `crates/service-manager/src/connection.rs`, `package.rs` | Ephemeral connections have UUID paths but no directory guard; Package Service already holds a temporary-store guard. |

The earlier cleanup measured about 127 GiB of allocated blocks in the external `direct-review-target`; Cargo reported a larger logical deletion size. These are different metrics, not two independent space savings. The directory was subsequently rebuilt. No savings percentage is promised for the proposed profile changes.

Research basis: [Cargo build cache](https://doc.rust-lang.org/cargo/reference/build-cache.html), [profiles](https://doc.rust-lang.org/cargo/reference/profiles.html), [global-cache GC scope](https://doc.rust-lang.org/cargo/reference/config.html#cache), [cargo clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html), [rust-cache](https://github.com/Swatinem/rust-cache#cache-details), [sccache Rust limits](https://github.com/mozilla/sccache/blob/main/docs/Rust.md), [TempDir cleanup](https://docs.rs/tempfile/latest/tempfile/struct.TempDir.html), and [static lifetime](https://doc.rust-lang.org/reference/items/static-items.html). Cargo GC does not reclaim project targets; static values do not receive exit-time Drop. Those facts rule out relying on automatic Cargo GC or adding a TempDir to the existing static OnceLock.

## Goals / Non-Goals

**Goals:** one product identity; explicit store ownership; cheap repeat builds within a checkout; bounded disposable output; no accumulating per-process Skill copies; safe migration of existing identities and credentials.

**Non-Goals:** changing Kernel identity, merging live Roots, automatic replay/recovery, arbitrary account/profile environments, rewriting historical ADRs, automatic deletion of durable execution evidence, a generic cache framework, mandatory sccache, or repairing all old Host layouts in one release.

## Decisions

### 1. One product identity with explicit test bindings

The supported executable is `alan`. `just install` installs it; debug/release are build profiles only. Remove `install-dev`, `uninstall-dev`, dev release aliases, and the runtime `InstallChannel` type/reexports. Legacy stable/dev names survive only in migration parsers and installer ownership receipts.

Remove runtime selection by `ALAN_INSTALL_CHANNEL`. A set obsolete variable or invocation under a retired `alan-dev` name produces an actionable error before opening writable stores; silently ignoring it could redirect an old test harness into personal data. This rejection does not select another runtime identity. `alan --version` and help remain processless and usable for installation verification.

Host composition supplies validated System/Host Store paths explicitly to services and auth. Delete the duplicated channel detector in auth; auth operations use the supplied credential path. Avoid a replacement global profile/environment manager. Tests inject paired temporary roots into existing constructors; subprocess fixtures provide an isolated platform data home before process creation, with tests verifying that no real-home path is touched. Do not reintroduce `test` as an installation identity or add a production data-home override solely for tests.

Default macOS backing becomes:

```text
~/Library/Application Support/Alan/System Store/services/<service>/...
~/Library/Application Support/Alan/Host Store/credentials/...
~/Library/Application Support/Alan/Host Store/auth.json
```

Other platforms retain their platform data-directory resolution and the same relative layout. Stable/dev subdirectories are legacy inputs, never runtime fallbacks. Keep sandbox protection for old secret locations while they may contain credentials.

Each invocation still owns a unique runtime directory, endpoint lock and boot ID. Use the existing `ALAN_INSTANCE_RUNTIME_DIR` explicit selection with a channel-free endpoint beneath that directory. Remove `channel_id` from current endpoint/status matching, version the affected receipt if needed, and fail closed on incompatible old receipts. Do not probe alternate channel endpoints. Durable recovery remains explicit; a shared store does not grant permission to resume another invocation automatically.

### 2. One-time adoption rather than implicit data merging

Extend the existing maintenance surface with:

```text
alan legacy-state inspect --json
alan legacy-state migrate-installation --from stable|dev --dry-run
alan legacy-state migrate-installation --from stable|dev
```

`--from` names a historical input layout, not a supported runtime channel. Inspect reports metadata and paths, never secrets. Installation changes executable ownership only and does not migrate stores.

When canonical data is absent but legacy stores exist, ordinary data-using commands report the available sources and migration command instead of silently creating an empty identity. Even one source requires explicit adoption. If both exist, select one complete paired System/Host Store; do not mix a default profile from one with credentials or rollout history from the other. If canonical data already exists, it wins and no old store is consumed. Independent import of another source's authored content uses existing owning commands; whole-store merges and last-writer-wins conflict resolution are excluded.

The migration command requires old and current writers to be stopped and fails if quiescence cannot be established. Use the existing endpoint/status and file-lock evidence; old versions do not honor a newly invented lock, so a new lock alone is insufficient. Recheck source manifests/digests before commit. Unknown schema, escaping symlink, unsupported backing reference, or unverified credential mapping is a failure before publication. Keep unknown source data intact.

Stage the selected service subtrees and Host-owned credentials within their respective destination filesystems. Preserve permissions and identifiers; owners validate/rewrite only their own absolute backing references. Confirm profile-to-credential resolution, package digests/references, rollout/checkpoint readability and memory/definition inventories offline. Do not send provider requests or replay work during verification. Generated cache/tmp and endpoint/PID state are excluded.

A small Host-owned journal outside the stores coordinates the paired publication. System Store and Host Store cannot be atomically renamed as one operation: all current store openers must refuse a pending transaction, including auth and metadata-only CLI paths. Publish both staged payloads, verify through the owning readers, then commit the journal. An interruption leaves readers blocked until the migration is resumed or rolled back from recorded destinations. Clean up staging only after successful completion. Existing legacy subdirectories do not count as canonical content; unrelated destination content is a conflict.

The canonical parent directories also contain retained `stable`/`dev` inputs, so publication must not rename or replace those parents. Journal the canonical payload components (`System Store/services`, `Host Store/credentials`, and managed auth) individually; the pending-transaction reader guard supplies paired visibility across those component renames. The source parents and their historical subtrees remain in place.

Keep the source stores unchanged by default, including the unselected source. A rerun of the identical committed transaction verifies the receipt and every published component against the saved snapshot, refusing missing or changed content/permissions without overwriting data; a different source cannot overwrite canonical data. Before any canonical writes, rollback can discard the new copy. After new writes, old sources are recovery snapshots, not automatic rollback targets; replacing them would lose new work. Source retirement is a separate explicit cleanup, never cache GC.

Alternatives rejected: defaulting to stable loses visibility of dev-only work; merging both stores risks ID/auth collisions; retaining a hidden stable/dev runtime selector leaves the same architectural complexity.

### 3. Preserve installer ownership while retiring duplicate commands

One `.alan-cli-manifest` owns the new installation. The installer recognizes old stable/dev manifests only for upgrade preflight. Verify all managed CLI, alias and retired Host paths before mutating any of them. A modified or unowned conflicting path aborts without partial replacement. Preserve handled-signal rollback across both old receipts. Never replace an unrelated `alan-dev` file merely because its name matches.

The release archive contains only `alan` and its manifest. Uninstall removes only verified owned executable artifacts and leaves data, cache policy and credentials alone. Current documentation, scripts, fixtures, AGENTS.md and the old local dev-verification skill must stop prescribing a retired channel or desktop App; archived records stay untouched.

### 4. Three build-output lifetimes, no global shared target

| Output | Location/lifetime | Policy |
| --- | --- | --- |
| Ordinary developer build/test/install | `<worktree>/target` | Reuse within the checkout; retain incremental builds. Installer reuses the just-built compatible artifact. |
| Canonical quality gate | `<worktree>/target/quality-gate` | Keep required host-target isolation, pin compiler/options, verify artifacts from the current run. |
| Disposable independent validation | A task-owned target under its checkout | Disable incremental compilation; remove after selected evidence is archived and no consumer remains. |

Release assembly uses the ordinary target by default and Cargo's explicit target-triple subdirectory. It copies the final binary into a small distribution stage, not a second full compiler tree. Existing intentional target overrides may remain for controlled callers, but an external directory requires explicit ownership and lifecycle registration by the invoking workflow. A preexisting unmarked directory is never silently claimed. A small receipt records canonical owner checkout, purpose and exact output path; do not build a database or daemon.

The maintenance entry points are `just cache-status` (read-only sizes, owners, active/unknown paths) and `just cache-clean` (dry-run by default, explicit `--apply` in the underlying script). Reuse repository scripting conventions. Use Cargo metadata/config to resolve output, including a separately configured build directory; do not assume every artifact is beneath `target`. Cargo 1.97 supports separate build directories, but relocating all intermediate output is unnecessary for this first delivery.

Clean entire retired task-owned output or use supported Cargo clean scopes, never guess at individual dependency/fingerprint lifetimes. Require an exclusive cleanup/build lease for managed external/task roots; every repository workflow using such a root participates. Use native Cargo locking plus process/open-file checks for unmanaged direct invocations, and skip uncertain or recreated paths. Registration and deletion use one bounded admission lock in the common Git directory across linked worktrees; compilation remains shared. Builders retain ancestor output leases through child processes, and cleanup children retain exclusion. A first producer waits up to 30 seconds for registration/cleanup admission, then validates ownership before joining. A status snapshot is not deletion authority. Do not clean an active checkout merely because its worktree is clean or its branch merged. Never follow symlinks out of an owned root, remove source/worktree registrations in a target-only operation, or evict artifacts a running binary needs.

No age-based deletion of the mixed historical `Library/Caches/Alan` tree is enabled. Its old source copies and patches require explicit classification. New verification evidence records source SHA, command, outcome and retained diagnostics outside disposable targets; only explicitly generated artifacts enter automatic task-end retirement. Long-lived developer targets get a size report and explicit cleanup, not an unbenchmarked hard quota.

Alternatives rejected: one global target couples parallel worktrees and cleanup; a new target per command multiplies output; cleaning after every ordinary build destroys useful reuse; adding nightly GC flags or mandatory cache tooling adds operational complexity without fixing ownership.

### 5. Tune verification profiles only after measurement

Keep ordinary dev/test profiles unchanged initially. For disposable verification, use `CARGO_INCREMENTAL=0`; CI already gets that policy from rust-cache. Trial reduced dev debug information with one consistent invocation configuration, and retain full information for interactive debugging. The implementation trial uses `debug = 1` (limited symbols); the measurements below retain the existing defaults. Adopt the lighter default only if full gates, stack traces and measured repeated-build latency remain acceptable. Do not disable assertions, overflow checks, error handling or change optimization semantics to shrink output.

Keep rust-cache in CI and explicitly map the quality target as `. -> target/quality-gate` so its dependency pruning covers the actual build root. Check other jobs' actual paths and compiler settings. Preserve the pinned toolchain. sccache is a later benchmark option: bounded shared compiled-dependency cache with isolated worktree outputs, not a replacement for target retirement; no dependency/configuration is introduced now.

### 6. Eliminate the redundant Skill extraction step

Keep embedded assets but expose their bounded relative entries and package IDs separately. Service Manager builds the existing `PackageSnapshot` from those entries and invokes ordinary Package Service validation/seeding. Keep asset filtering, package IDs, paths, bytes, executable semantics, bounds, deterministic ordering, digests and metadata parsing consistent with the existing directory path. Do not add an engine dependency on Service Manager; the composition adapter constructs the service-owned snapshot.

The ID-only projection loop enumerates descriptors without touching disk. Package Service remains the only durable materialization owner, and Processes receive ordinary immutable package references. Tests that require real directories use scoped fixtures; remove the static path cache and per-PID materialization API after every caller is migrated. Equivalence tests must compare package IDs, exported bytes and digests before deleting the old path.

Ephemeral Connection Service owns one TempDir containing both metadata and lock files, following the existing Package Service pattern. Consumers must finish before owner release. Explicit shutdown surfaces cleanup errors; Drop is best effort. Existing native scratch shutdown and package lease/revision GC remain in charge of their own data. For abrupt-exit leftovers, a maintenance sweep is limited to marked generated roots, rejects live owners and unknown paths, and never treats PID absence alone as proof of ownership. This sweep is separate from durable store migration and does not delete rollout/checkpoint evidence.

## Risks / Trade-offs

- [Old dev-only credentials/history appear missing] → explicit source inspection/adoption, retain both old sources, no implicit merge.
- [New debug build now accesses personal data] → one intentional product identity; tests inject roots; obsolete channel variables fail before access.
- [Migration interruption exposes half a store pair] → pending-journal gate in every opener, staged validation and resume/rollback tests.
- [Old binaries keep writing] → migration requires verified quiescence and stable source validation; block when uncertain.
- [Smaller artifacts slow repeated edits or weaken debugging] → benchmark before changing defaults; keep full-debug path.
- [Cleanup races with another task] → exact ownership, leases for managed roots, native locks and live checks, skip unknown/recreated output.
- [Direct embedded seeding changes package revisions] → preserve bytes/metadata semantics and compare snapshot/digest fixtures before rollout.
- [Single stores have concurrent writers] → retain and test existing owner transaction/lease mechanisms across independent invocations; no global Root or manager.

## Migration Plan

1. Implement and test build-artifact lifecycle independently; establish baseline disk and build-time measurements without cleaning live work.
2. Implement direct embedded seeding and scoped service scratch cleanup; verify repeated-start behavior.
3. Deliver the migration reader/writer, installer ownership upgrade, channel-free runtime/auth paths and test binding conversion as one compatible single-product release. Do not ship channel deletion before migration is usable.
4. Validate clean install, stable-only, dev-only, both legacy roots, existing canonical data, partial/corrupt stores, concurrent writers and interrupted publication in isolated fixtures on macOS/Linux.
5. Update current docs and active input-lifecycle storage wording without claiming unfinished input routing or recovery qualification is delivered. User data migration remains an explicit operational action after implementation.
6. Run required checks/review on the implementation head, merge, sync delta specs and archive only after acceptance. This proposal alone changes no installed binary, credentials or runtime store.
