## Context

The current repo already has a capable Agent Execution Engine and session
transport. The next step is to wrap that behavior as a file-server service:
Agent Runtime Service executes Agent Processes and serves AgentFS. Existing
session behavior remains compatibility transport until file-native surfaces are
complete.

## Goals / Non-Goals

**Goals:**

- Define Agent Runtime Service as a file-server service.
- Define the first AgentFS file surface over current runtime behavior.
- Preserve existing session/TUI behavior while adding Agent Process projection.
- Map yields to request files and tool calls to action files.
- Keep provider clients, memory stores, sandbox execution, and transport details
  out of Alan Kernel.

**Non-Goals:**

- Implement Service Manager.
- Remove existing compatibility session endpoints.
- Implement a full 9P protocol.
- Build Alan Agent workspace UI.
- Implement Root Agent Process resident behavior.

## Decisions

### 1. Agent Runtime Service is a file server

It posts a handle under `/srv/agent-runtime` and serves `/agent`. It is a
Process managed by Service Manager, not an app-facing HTTP service.

### 2. Current sessions project into AgentFS

The adapter can use current session ids internally, but durable target shape is
`/agent/<pid>`: status, IO, requests, actions, result, children, policy/context,
and machine files.

### 3. Requests and actions replace private operations

Yields become request file trees. Tool calls and other effects become action
file trees. Compatibility resume/tool APIs can remain as adapters over those
files during migration.

### 4. Internal Rust API comes before transport

The first implementation should expose internal file-surface/projection
interfaces before adding or changing public HTTP/WS routes. Later transport
adapters can wrap the file/process model.

## Risks / Trade-offs

- [Risk] Projection leaks session-specific concepts. -> Keep session ids as
  runtime references only.
- [Risk] AgentFS schema overfits current runtime. -> Keep V1 small and separate
  IO from machine state.
- [Risk] Compatibility transport becomes canonical again. -> Document it as
  transport only.
