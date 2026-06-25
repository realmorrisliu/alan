## 1. File-Server Boundary

- [x] 1.1 Define Agent Runtime Service as a file-server service managed by
  Service Manager.
- [x] 1.2 Define internal projection shapes for `/agent/<pid>/status`,
  `/io`, `/requests`, `/actions`, `/result`, `/children`, `/policy`,
  `/context`, and `/machine`.
- [x] 1.3 Decide which current routes remain compatibility transport over the
  file/process model.

## 2. Compatibility Projection

- [x] 2.1 Map current session creation/attach behavior into Agent Process
  projection.
- [x] 2.2 Preserve current session ids as runtime references, not OS identity.
- [x] 2.3 Map current event stream, yields, tool calls, child runs, rollout
  evidence, warnings, and errors into AgentFS status, IO, request, action, and
  machine surfaces.
- [x] 2.4 Keep sandbox execution, provider clients, memory stores, and runtime
  supervision out of Alan Kernel dependencies.

## 3. Compatibility Tests

- [x] 3.1 Add fixture tests for current execution input to AgentFS projection.
- [x] 3.2 Add fixture tests for event/yield/tool/child-run mapping into AgentFS.
- [x] 3.3 Add regression tests that existing session behavior remains compatible
  when the AgentFS projection is disabled.

## 4. Verification

- [x] 4.1 Run focused adapter/runtime/transport tests.
- [x] 4.2 Run formatting and relevant workspace checks.
- [x] 4.3 Run `openspec validate add-agent-runtime-service-filesystem --strict`.
- [x] 4.4 Run `openspec validate --all --strict`.
