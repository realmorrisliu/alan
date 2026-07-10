## Context

`Sandbox` (`crates/agent-engine/src/tools/sandbox.rs`) holds
`{ workspace_root: PathBuf, backend_override: Option<SandboxBackendKind> }`. Its
`build_confined_command` calls `sandbox_backend::seatbelt_profile(&self.workspace_root,
allow_network)` (macOS) or applies `apply_landlock(&workspace_root, allow_network)`
in a `pre_exec` hook (Linux). Both hard-code confinement to the one workspace path
(each internally also adds temp dirs and standard writable device files).

Production constructors:

- `crates/agent-engine/src/tools/context.rs:113` —
  `Sandbox::new(self.require_workspace_root()?.to_path_buf())`
- `crates/agent-engine/src/runtime/tool_orchestrator.rs:1184` —
  `Sandbox::new(workspace_root.clone())`

Test constructors: `Sandbox::new` / `Sandbox::with_backend(workspace_root, kind)`
across `sandbox_tests.rs` (~30 sites).

The `define-namespace-driven-sandbox` explore established that the OS sandbox
should confine from a **projected manifest**, not a lone path. This change moves
that shape into place with no behavior change.

## Goals / Non-Goals

**Goals**

- Replace the sandbox's `workspace_root` input with a `SandboxSpec` value.
- Keep the emitted Seatbelt profile / Landlock ruleset byte-for-byte identical
  when the spec carries one writable root and an empty read denylist.
- Leave every existing invariant intact (safe degradation, protected-subpath
  parser, per-call approved-network override).

**Non-Goals**

- `HostDirFs`, `mount_host`, or any multi-entry manifest (P2).
- Populating `read_denylist` with real sensitive paths (P3).
- Any dependency from `alan-agent-engine` on `alan-kernel` (the projection that
  builds a `SandboxSpec` from a namespace lives in the `alan` composition root,
  future work — here the spec is still seeded from the workspace).
- Changing network semantics.

## Decisions

- **`SandboxSpec` shape.**
  ```
  pub struct SandboxSpec {
      pub writable_roots: Vec<PathBuf>,   // seed: [workspace]
      pub read_denylist: Vec<PathBuf>,    // seed: []
      pub network: NetworkPosture,        // seed: Deny (today's default)
  }
  ```
  `NetworkPosture` captures only the *default* posture. The existing
  per-invocation approved-network override (an approved Network capability runs
  the confined command with network allowed) is unchanged and stays a per-call
  argument, not a spec field.

- **`writable_roots`, not one root.** `seatbelt_profile` and `apply_landlock`
  take `&[PathBuf]` writable roots. Temp dirs and device files they already add
  stay internal. Emitting rules for a single-element slice must produce exactly
  the current output — locked by a test asserting equality with the prior
  single-`workspace_root` profile string.

- **`read_denylist` plumbed but empty.** The parameter threads through to the
  backends now (Seatbelt can emit `deny file-read* (subpath …)`; Landlock cannot
  and ignores it) so P3 needs no signature churn, but it is empty at P1, so no
  read rule is emitted and behavior is unchanged. Document the Landlock
  limitation inline.

- **Back-compat shim.** Keep `Sandbox::new(workspace_root: PathBuf)` as
  `Self::from_spec(SandboxSpec::seed(workspace_root))`. Add `Sandbox::from_spec`
  and `Sandbox::from_spec_with_backend` (test-only). This lets most call sites and
  all tests stay one-line while the internal representation becomes the spec.

- **Seed constructor.** `SandboxSpec::seed(workspace_root)` builds
  `{ writable_roots: vec![workspace_root], read_denylist: vec![], network:
  Deny }` — the single place that encodes "the workspace is the seed entry of the
  manifest," the concept P2 generalizes.

- **The workspace_root many call paths still need.** The path-guard parser and
  path canonicalization in `sandbox.rs` reference `self.workspace_root` widely.
  Keep a `workspace_root()` accessor returning `writable_roots[0]` (the seed) so
  the parser code is untouched — P1 does not refactor the parser, only the OS
  backend input. This preserves the protected-subpath behavior verbatim.

## Verification strategy

- Add a test: for a single writable root, the generated Seatbelt profile equals
  the string the old `seatbelt_profile(&workspace_root, …)` produced (golden
  comparison), proving zero behavior change.
- Existing `sandbox_backend` OS-enforcement tests (macOS write boundary, Linux
  write boundary, Linux network) run unchanged and must pass.
- `just verify` (fmt + lint + test + mock smoke) is the gate.
