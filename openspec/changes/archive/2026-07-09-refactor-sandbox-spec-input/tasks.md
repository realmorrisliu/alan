# Tasks

> **Parked (2026-07-02).** Sequenced behind `refactor-engine-namespace-native`
> Slice B: this refactor migrates constructors in `runtime::tool_orchestrator`,
> which the engine-native rewrite is actively restructuring (tools become
> `/bin` executables spawned via `/proc/clone`). Land it after Slice B settles
> the tool-execution seam so the `SandboxSpec` projection welds onto the final
> spawn path, not the `ToolRegistry` one. The design itself (SandboxSpec as a
> value projected from the mount manifest) is unaffected.

Pure refactor, zero behavior change. Every step must keep existing sandbox tests
green; the golden-profile test (1.4) is the proof of no behavior change.

## 1. Introduce SandboxSpec

- [x] 1.1 Add `SandboxSpec { writable_roots: Vec<PathBuf>, read_denylist:
  Vec<PathBuf>, network: NetworkPosture }` and `NetworkPosture` (default `Deny`)
  in `crates/agent-engine/src/tools/sandbox.rs` (or a small `sandbox_spec.rs`
  submodule).
- [x] 1.2 Add `SandboxSpec::seed(workspace_root: PathBuf) -> SandboxSpec`
  producing `{ writable_roots: vec![workspace_root], read_denylist: vec![],
  network: Deny }`.
- [x] 1.3 Change `Sandbox` to hold a `SandboxSpec`; add `Sandbox::from_spec` and
  (`#[cfg(test)]`) `Sandbox::from_spec_with_backend`. Keep `Sandbox::new` and
  `with_backend` as shims delegating to the spec constructors via `seed`.
- [x] 1.4 Add an internal `workspace_root()` accessor returning
  `writable_roots[0]` so the existing path-guard parser / canonicalization code
  in `sandbox.rs` is untouched.

## 2. Generalize the OS backends to a set of writable roots

- [x] 2.1 Change `sandbox_backend::seatbelt_profile` to take
  `writable_roots: &[PathBuf]` (plus the temp dirs it already adds) and a
  `read_denylist: &[PathBuf]`; emit one `(allow file-write* (subpath …))` per
  root and, for a non-empty denylist, `(deny file-read* (subpath …))`.
- [x] 2.2 Change `apply_landlock` to take `writable_roots: &[PathBuf]`; grant
  write to each. Accept `read_denylist` for signature parity but document that
  Landlock's allow-list model cannot express it (ignored at P1).
- [x] 2.3 Update `Sandbox::build_confined_command` to pass the spec's writable
  roots and denylist through. Preserve the per-invocation approved-network
  override exactly.

## 3. Migrate call sites

- [x] 3.1 `crates/agent-engine/src/tools/context.rs:113` — keep `Sandbox::new`
  (shim) or switch to `Sandbox::from_spec(SandboxSpec::seed(root))`; no behavior
  change.
- [x] 3.2 `crates/agent-engine/src/runtime/tool_orchestrator.rs:1184` — same.
- [x] 3.3 `crates/agent-engine/src/tools/sandbox_tests.rs` — leave `new` /
  `with_backend` call sites working via the shims; no test assertion changes.

## 4. Prove zero behavior change

- [x] 4.1 Add a golden test: for a single writable root, `seatbelt_profile`
  output equals the pre-refactor single-`workspace_root` profile string.
- [x] 4.2 Run the existing OS-enforcement tests unchanged
  (`seatbelt_enforces_workspace_write_boundary_on_macos`,
  `landlock_enforces_workspace_write_boundary_on_linux`,
  `landlock_confines_network_on_linux`) — all must pass. On the current macOS
  host, the Seatbelt test passed; Linux Landlock tests are `target_os = "linux"`
  gated and are not compiled on this host.
- [x] 4.3 `just verify` (fmt + lint + test + mock smoke) green.

## 5. Close out

- [x] 5.1 Update `define-namespace-driven-sandbox/tasks.md` item 1.1 to reference
  this change as landed.
- [x] 5.2 Confirm no new dependency edge from `alan-agent-engine` to
  `alan-kernel` was introduced.
