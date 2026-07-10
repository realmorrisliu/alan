## 1. HostDirFs File Server

- [x] 1.1 Add an `alan-hostfs` workspace crate beside the other file-server
  crates.
- [x] 1.2 Implement `HostDirFs::new(root, access)` with root canonicalization and
  read-only/read-write backing access.
- [x] 1.3 Implement fid lifecycle for walk/open/read/write/stat/create/remove/clunk
  over host files and directories.
- [x] 1.4 Confine every resolved host path below the canonical root, including
  `..` traversal and symlink escape attempts.
- [x] 1.5 Add unit tests for read, directory listing, write/create/remove, read-only
  rejection, and escape rejection.

## 2. Mount Declaration And Sandbox Projection

- [x] 2.1 Add a composition-root declaration type for host directory mounts carrying
  namespace path, host path, and `Access`.
- [x] 2.2 Add a helper that applies host declarations to `alan-kernel::Namespace`
  by mounting `HostDirFs` at the declared aP path.
- [x] 2.3 Add a helper that projects workspace seed + host declarations into
  `alan_agent_engine::tools::SandboxSpec`.
- [x] 2.4 Keep virtual mounts out of the sandbox projection and include only RW
  host declarations as additional writable roots.
- [x] 2.5 Wire the current workspace-only runtime path through the seed projection
  helper without changing existing behavior.

## 3. Integration Coverage

- [x] 3.1 Add tests proving a writable host declaration is reachable through
  `MountFs` and contributes a `SandboxSpec` writable root.
- [x] 3.2 Add tests proving a read-only host declaration is readable through
  `MountFs` but is not a `SandboxSpec` writable root.
- [x] 3.3 Add tests proving virtual mounts do not contribute sandbox roots.
- [x] 3.4 Confirm no `host_path` or declaration provenance is added to
  `alan-kernel`.
- [x] 3.5 Confirm no new dependency edge from `alan-agent-engine` to
  `alan-kernel`.

## 4. OpenSpec And Upstream Framing

- [x] 4.1 Validate `add-host-dir-file-server` with `openspec validate --strict`.
- [x] 4.2 Update `define-namespace-driven-sandbox/tasks.md` item 1.2 to reference
  this change as created/implemented.
- [x] 4.3 Ensure the P2 proposal/design reference D5: mounts are human/config
  declared at landing, and the workspace is the seed host mount.

## 5. Verification And PR Hygiene

- [x] 5.1 Run focused `alan-hostfs` tests.
- [x] 5.2 Run focused integration/projection tests.
- [x] 5.3 Run `cargo fmt --all`.
- [x] 5.4 Run `cargo clippy` for touched crates with `-D warnings`.
- [x] 5.5 Run `just verify`.
- [x] 5.6 Commit this slice separately from the `SandboxSpec` PR.
- [x] 5.7 Open a ready stacked PR on top of `feat/northstar-sandbox-spec-input`.
