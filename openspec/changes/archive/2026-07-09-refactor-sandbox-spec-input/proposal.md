## Why

`define-namespace-driven-sandbox` fixed the thesis: one mount declaration list
projects into two enforcement surfaces — the namespace (in-process aP) and a
**sandbox manifest** (`SandboxSpec`) the OS sandbox enforces for native
subprocesses. This change (P1 in that explore's sequencing) welds the second
seam and nothing else.

Today `alan-agent-engine`'s `Sandbox` takes a single hard-coded
`workspace_root: PathBuf`, and `sandbox_backend::seatbelt_profile` /
`apply_landlock` derive their write-confinement from that one path. The
confinement boundary is therefore wired to "the workspace," not to a projected
set of allowed roots. Before host directories can be mounted (P2), the sandbox
must accept its confinement as a **value projected from a manifest** rather than
a lone path.

P1 does this as a **pure refactor with zero behavior change**: the manifest has
exactly one seed entry (the workspace), so the generated Seatbelt/Landlock rules
are byte-for-byte what they are today. The point is to move the *shape* of the
input, not the behavior, so P2 only has to add manifest entries.

## What Changes

- Introduce `SandboxSpec { writable_roots: Vec<PathBuf>, read_denylist:
  Vec<PathBuf>, network: NetworkPosture }` in `alan-agent-engine` — the value the
  OS sandbox confines from. At this stage `writable_roots = [workspace]`,
  `read_denylist = []`, `network = Deny` (today's defaults).
- Change `Sandbox` to hold a `SandboxSpec` instead of a bare `workspace_root`.
  Add `Sandbox::from_spec(SandboxSpec)`; keep `Sandbox::new(workspace_root)` as a
  thin shim that builds the single-entry seed spec, so callers migrate without
  behavior change.
- Generalize `sandbox_backend::seatbelt_profile` and `apply_landlock` to confine
  writes to a **set of writable roots** (plus the temp dirs they already add) and
  to honor a `read_denylist` (empty here) — instead of a single `workspace_root`.
  With one root the emitted profile/ruleset is unchanged.
- Migrate the two production constructors (`tools::context` builder and
  `runtime::tool_orchestrator`) and the test helpers to the new shape. Retain
  `with_backend` (test-only) on the spec-based constructor.
- Preserve every existing invariant: safe degradation, protected-subpath parser,
  per-invocation approved-network override, `.git`/`.alan`/`.agents` handling.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `os-sandbox-enforcement`: the confinement input is generalized from a single
  `workspace_root` to a `SandboxSpec` (writable roots + read denylist + network
  posture) projected from the mount manifest, with the workspace as the seed
  entry. Enforcement semantics are unchanged when the spec holds one writable
  root and an empty denylist.

## Impact

- Scope is confined to `alan-agent-engine` (`tools::sandbox`,
  `tools::sandbox_backend`, `tools::context`, `runtime::tool_orchestrator`, and
  their tests). No new crate dependencies; `alan-agent-engine` still does not
  depend on `alan-kernel`.
- Behavior is provably unchanged: existing `sandbox_backend` tests
  (Seatbelt/Landlock write-boundary and network) must pass untouched, and the
  single-root spec must emit the same profile the `workspace_root` path emits
  today.
- Unblocks P2 (`add-host-dir-file-server`): a multi-entry manifest flows into
  this same `SandboxSpec` with no further sandbox-backend changes.
- Honors `define-namespace-driven-sandbox` design D4 (layering: the projection
  lives in the composition root; the engine only consumes a `SandboxSpec`) and D6
  (P1 = zero behavior change, seam-only).
