## Context

P1 (`refactor-sandbox-spec-input`) changed the OS sandbox input from a single
workspace path to `SandboxSpec`. The remaining gap is that Alan still has no
host-directory-backed aP file server and no declaration list that projects a host
mount into both enforcement surfaces.

Existing relevant contracts:

- `alan-ap::FileServer` is the durable file-server interface: fid lifecycle,
  walk/open/read/write/stat/create/remove/clunk.
- `alan-kernel::Namespace::mount(at, tree, Access)` records an aP path, server,
  and mount access only. It intentionally records no host path.
- `alan-kernel::MountFs` exposes a namespace as one aP file server and enforces
  mount `Access` over mutating operations.
- `alan-agent-engine::tools::Sandbox` can now consume a `SandboxSpec`, but most
  production call sites still seed it from `workspace_root`.

## Goals / Non-Goals

**Goals:**

- Add `HostDirFs`, a host-directory-backed aP `FileServer`.
- Add a composition-root declaration list whose host entries carry
  `(namespace_path, host_path, Access)`.
- Project each host declaration into the namespace and into `SandboxSpec` at the
  declaration site.
- Preserve layering: `alan-kernel` remains host-path agnostic; the engine consumes
  `SandboxSpec` and does not introspect a namespace.
- Keep landing mounts human/config-declared only. The agent still has no mount
  tool.

**Non-Goals:**

- Agent-requestable mounts or PolicyEngine mount escalation.
- macOS sensitive-read denylist defaults.
- Linux namespace reification / unified path space for native subprocesses.
- Remote host-dir transport. This change is local host filesystem only.

## Decisions

### D1. Implement `HostDirFs` as a standalone file-server crate

`HostDirFs` belongs beside `memfs`, `llmfs`, `agentfs`, `routefs`, `editfs`, and
`branchfs`: it is a mountable file server, not Alan Kernel state. A crate such as
`alan-hostfs` keeps host-path code out of `alan-kernel` and lets tests exercise
the file-server contract directly without booting the full runtime.

The server root is a canonicalized host directory. Every walked/resolved path is
joined below that root and then validated so symlink or parent traversal cannot
escape the exported tree. Directories read as newline-separated child names,
matching the existing simple file-server convention used by `MountFs` tests and
other in-process servers.

### D2. Mount access remains enforced twice

`HostDirFs` should reject writes when opened read-only, but the authoritative
namespace authority boundary is still `MountFs` masking mutating operations based
on mount `Access`. This gives defense in depth:

- a read-only namespace mount cannot write even if the backing server would allow
  it; and
- a read-only `HostDirFs` instance is safe if it is accidentally mounted with RW.

### D3. A declaration list, not namespace introspection

The `alan` composition root owns `MountDeclaration` values. For P2, the durable
shape is:

```rust
struct HostMountDeclaration {
    namespace_path: String,
    host_path: PathBuf,
    access: Access,
}
```

Applying a declaration:

1. canonicalizes the host path;
2. mounts `HostDirFs::new(host_path, hostfs_access)` at `namespace_path`; and
3. records host provenance for sandbox projection.

The projection is intentionally written at the declaration site. `Namespace` does
not grow `host_path`, and `SandboxSpec` is not reconstructed by inspecting
`Namespace::describe()`.

### D4. Sandbox projection uses write access only

Native subprocesses cannot see the aP namespace, so they are confined to host
paths. P2 projects host declarations into `SandboxSpec` as follows:

- every session includes the workspace as the seed RW host mount;
- a host declaration with RW access contributes its canonical `host_path` to
  `writable_roots`;
- a host declaration with RO access contributes no writable root;
- virtual mounts contribute nothing.

Reads remain broad at P2. The honest isolation statement stays: write+network
isolation now, macOS sensitive-read denylist next, full read isolation only with
later reification.

### D5. Keep the first implementation slice narrow

This change should land as a vertical but still small slice:

- build and test `HostDirFs`;
- add a projection helper that turns seed workspace + host declarations into
  `SandboxSpec`;
- wire the current workspace-only runtime path through the helper so behavior is
  unchanged for existing sessions;
- add tests for one extra RW mount and one RO mount.

Broader config surfaces can follow once the mount/projection primitive is
available.

## Risks / Trade-offs

- **Symlink escape risk** -> Canonicalize existing paths and validate resolved
  descendants stay under the root before every host filesystem operation.
- **Path-space confusion** -> Specs and tests must keep aP paths
  (`/mnt/project`) separate from host paths (`/Users/.../project`). Native
  subprocesses only see host paths through `SandboxSpec`.
- **Over-broad first user surface** -> Landing is composition-root/helper first,
  not an agent-visible mount tool.
- **Platform differences in read isolation** -> P2 does not claim read isolation;
  it preserves the ADR-0027 honest isolation narrative.

## Migration Plan

1. Introduce `alan-hostfs` and unit tests.
2. Add mount declaration/projection helpers in the `alan` composition layer.
3. Switch current workspace-only sandbox construction to use the seed projection
   helper while preserving behavior.
4. Add tests proving RW host mounts enter `SandboxSpec`, RO mounts do not, and
   namespace access masks writes.

Rollback is straightforward: the new helper can be bypassed by returning to
`SandboxSpec::seed(workspace_root)` and the new crate has no persisted data.
