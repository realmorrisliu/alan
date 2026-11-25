# Tasks

## 1. OpenSpec & Companion Skills in Package Service
- [x] 1.1 Sync and archive `enable-self-iteration-loop` specs into `openspec/specs/` and draft `complete-quartermaster-plugin-lifecycle` specs cleanly.
- [x] 1.2 Extend `materializer_exports.rs` and `package/store.rs` so `tool`, `mcp`, `provider`, and `fileserver` packages can bundle Companion Skills strictly beneath `skills/`.
- [x] 1.3 Extend `crates/service-manager/src/runtime/packages.rs` with `PackageProjectionMode::{SkillsOnly, Full}` so knowledge-only projection mounts `/lib/pkg/<id>` without starting processes or projecting tools/MCPs.

## 2. Durable Root Package Bindings
- [x] 2.1 Implement versioned `root-bindings.json` persistence and lock-safe APIs (`bind_for_root`, `unbind_for_root`, `list_root_bindings`, `root_binding`, `load_root_bindings_from_store`) in `crates/service-manager/src/package/root_bindings.rs`, enforcing Skill ID collision checks and uninstall cleanup.
- [x] 2.2 Re-export public Root binding types cleanly in `crates/service-manager/src/lib.rs` and wire `RootPackageBinding` loading/override into `HostBootConfig` (`crates/os-host/src/boot.rs`).

## 3. ServiceManager & CLI Integration
- [x] 3.1 Wire `FileserverManager` lifecycle (`start` on `Active` fileserver bindings, dual-mount to Root + `LocalEntryService`, `stop_all` on `ServiceManager::shutdown`) inside `crates/service-manager/src/runtime/`.
- [x] 3.2 Update `alan q` (`install` auto-attaches `SkillsOnly` unless `--catalog-only`; add `enable` and `disable`; enrich `list` and `inspect`).
- [x] 3.3 Add repeatable `--package` and `--fileserver` CLI flags in `crates/alan/src/cli/trust.rs` / `dispatch.rs` and `crates/alan/src/foreground.rs` with least-privilege replacement for `--ephemeral` invocations.

## 4. Workflow Package Wiring, `workflow watch`, and `github-fileserver` Onboarding Bundle
- [x] 4.1 Extend `WorkflowManifest` and `build_alan_args` with explicit `packages` and `fileservers` declarations.
- [x] 4.2 Add `alan workflow watch <id>` (`--once`, `--dry-run`, `--poll-secs`, `--json`) composing `FileserverManager` + `RouteFs` + `GithubEventDispatcher` with Ctrl+C/SIGTERM cleanup.
- [x] 4.3 Add generic `skills/github-setup/SKILL.md`, `skills/self-iterate/SKILL.md`, and `skills/release-prep/SKILL.md` Companion Skills to `crates/service-manager/packages/github-fileserver` and update `/man` (`1/q`, `1/alan`).

## 5. Verification, Review, and Archive Readiness
- [x] 5.1 Add unit & integration tests across `alan-service-manager` and `alan`, verify `./scripts/check-rust-source-size.sh`, `./scripts/check-rust-architecture.sh`, `just openspec-check`, `just quality`, and `cargo test --workspace --locked`.
- [x] 5.2 Complete PR review (16/16 CI checks + clean Codex review) on `feat/quartermaster-plugin-lifecycle` without leaving unresolved threads.
- [x] 5.3 After merge, sync deltas into `openspec/specs/` and move this change to `openspec/changes/archive/YYYY-MM-DD-complete-quartermaster-plugin-lifecycle/`.
