## 1. Contract Adoption

- [x] 1.1 Rewrite `add-proactive-memory-v2` so Memory Stores own durable writes,
  review, revert, and retention through mounted file trees rather than runtime
  storage and daemon APIs.
- [x] 1.2 Rewrite `add-cognitive-model-routing` so model bindings are
  namespace-visible LLM Connections and routing state is observable through
  files and streams rather than session/daemon DTOs.
- [x] 1.3 Rewrite `define-groove-master-alan-app` as an Alan App with an
  app-owned domain core, file-server adapter, mounted tree, and descriptor-spawn
  producer agent.
- [x] 1.4 Rewrite `add-alan-voice-mvp` so host recognition is projected through a
  file-server adapter and intent execution becomes file operations, Tool spawn,
  or Agent Executable spawn.
- [x] 1.5 Rewrite `spike-macos-matter-controller` so the host-backed Matter
  boundary is an aP file tree rather than a typed RPC/tool provider.
- [x] 1.6 Align `add-macos-shell-component-system` and
  `define-updf-product-umbrella` terminology with Alan Apps, app-owned domain
  truth, and explicit compatibility bridges.

## 2. Cross-Change Contract Audit

- [x] 2.1 Verify each dependent change names its domain owner, service handle,
  mount path, file/stream/`ctl` operation shape, and durability owner where
  applicable.
- [x] 2.2 Verify every app-to-agent path uses bounded descriptors, namespace
  construction, and Agent Executable spawn rather than an embedded engine or
  daemon/session API.
- [x] 2.3 Verify any remaining host callback, XPC/RPC hop, daemon DTO, or legacy
  content action is behind a named compatibility bridge with a deletion gate.
- [x] 2.4 Verify no dependent change adds a top-level namespace root or treats a
  `/srv` name, opaque id, or hash as authority outside namespace reachability.

## 3. Verification And Archive Readiness

- [x] 3.1 Run `openspec validate define-alan-app-service-integration --strict`.
- [x] 3.2 Run strict validation for every rewritten dependent change and
  `openspec validate --all --strict`.
- [x] 3.3 Run `git diff --check -- openspec/changes`.
- [ ] 3.4 After review and merge, sync
  `alan-app-service-integration` into `openspec/specs/` before archiving this
  change.
