## 1. API Boundary

- [x] 1.1 Define the Agent Capability Service Host Service trait or daemon API
  boundary for start, stream, yield, resume, cancel, and completion.
- [x] 1.2 Define request/response shapes using Agent Capability descriptors,
  Context Grants, Result Contracts, Agent Run ids, task ids, evidence refs, and
  audit refs.
- [x] 1.3 Decide which surface is internal Rust API first and which, if any,
  becomes daemon HTTP/WS API in the first slice.

## 2. Compatibility Adapter

- [x] 2.1 Map `agent.explain`, `agent.summarize`, `agent.plan`, and
  `agent.propose_commands` into current Agent Execution Engine inputs.
- [x] 2.2 Preserve current daemon-backed session creation/attach behavior as a
  native reference, not the OS Agent Run identity.
- [x] 2.3 Map current event stream, yields, tool calls, child runs, rollout
  evidence, warnings, and errors into Agent Run lifecycle events.
- [x] 2.4 Map current outputs into Result Contract fields, including partial or
  unsupported fields where needed.
- [x] 2.5 Keep sandbox execution, provider clients, memory stores, and runtime
  supervision out of Alan Kernel dependencies.

## 3. Compatibility Tests

- [x] 3.1 Add fixture tests for Context Grant to current execution input mapping.
- [x] 3.2 Add fixture tests for event/yield/tool/child-run mapping into Agent
  Run lifecycle events.
- [x] 3.3 Add fixture tests for Result Contract output mapping and partial
  result reporting.
- [x] 3.4 Add regression tests that existing daemon/TUI session behavior remains
  compatible when the adapter is disabled.

## 4. Verification

- [x] 4.1 Run focused adapter/runtime/daemon tests.
- [x] 4.2 Run formatting and relevant workspace checks.
- [x] 4.3 Run `openspec validate add-agent-capability-service-adapter --strict`.
- [x] 4.4 Run `openspec validate --all --strict`.
