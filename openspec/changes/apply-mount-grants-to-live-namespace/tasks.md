## 1. Live Process Namespace Mount Handle

- [ ] 1.1 Add a host-agnostic live namespace mount handle in Alan Kernel that can
  mount or replace an exact namespace path using `InProcessTransport` and
  `Access`, and increments a generation on successful live mutation.
- [ ] 1.2 Update `MountFs`, `ProcFs::for_spawner`, process namespace
  descriptions, and `/proc/clone` child namespace snapshots to read or snapshot
  from the same live handle while preserving `MountFs::new(namespace)`
  compatibility.
- [ ] 1.3 Add kernel tests proving future walks see a newly mounted path,
  `/proc/<pid>/namespace` reads see the new path, child spawns inherit the new
  path, `/mnt` synthetic listing qids/versions and `/proc/<pid>/namespace`
  qids/versions change after mutation, duplicate exact-path replacement does not
  accumulate duplicate descriptions, and already-open fids continue to use their
  resolved backing tree.

## 2. Runtime Applicator Boundary

- [ ] 2.1 Add an engine-owned approved mount grant payload and
  host-provided applicator interface without importing `alan_hostfs` into
  `alan-agent-engine`.
- [ ] 2.2 Thread the optional applicator through namespace runtime environment
  state so `request_mount` resume can attempt live namespace application.
- [ ] 2.3 Preserve current behavior for runtimes without an applicator, including
  explicit `namespace_applied = false` reporting.

## 3. Alan Composition Applicator

- [ ] 3.1 Implement the `alan` composition-root applicator by converting approved
  grants into `HostMountDeclaration` / `HostDirFs` and applying them through the
  live namespace handle.
- [ ] 3.2 Wire standard namespace runtime assembly to create one live process
  namespace handle and pass it to `MountFs`, `ProcFs`/spawner state, process
  records, and the applicator.
- [ ] 3.3 Add tests proving read-write grants become writable aP mounts and
  read-only grants become read-only aP mounts without expanding sandbox writable
  roots.

## 4. Request Mount Resume Reporting

- [ ] 4.1 Update mount escalation resume handling to call the applicator on
  approval and include `namespace_applied` / `namespace_error` in both tool
  result and `host_mount_grant` event.
- [ ] 4.2 Cover approved read-write apply, approved read-only apply, duplicate
  replacement, rejected no-op, missing applicator, and applicator failure with
  focused runtime tests.
- [ ] 4.3 Keep `tool_sandbox_applied` and `namespace_applied` independent and
  ensure no result claims Linux reification or native `/mnt` visibility.
- [ ] 4.4 Update the `agent-mount-escalation` capability delta to retire the
  earlier approved-but-not-live-applied result language for runtimes with live
  namespace application.

## 5. Verification And PR

- [ ] 5.1 Run focused Rust tests for kernel live namespace mutation, host mount
  application, and mount escalation resume reporting.
- [ ] 5.2 Run clippy for touched crates, OpenSpec strict validate, and diff
  checks.
- [ ] 5.3 Update parent namespace-driven sandbox task state to record this live
  namespace projection slice while leaving Linux reification pending.
- [ ] 5.4 Commit the slice and open a ready stacked PR above
  `feat/northstar-tool-sandbox-mount-grants`.
