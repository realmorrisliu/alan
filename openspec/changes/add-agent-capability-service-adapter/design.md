## Context

The OS model says Agent Capability Service is a Host Service API. The
current repo already has a capable Agent Execution Engine, daemon session APIs,
event streams, policy, approvals, sandbox selection, memory, child runs, and
rollout persistence. The first service adapter should reuse that work instead
of building a new runtime.

## Goals / Non-Goals

**Goals:**

- Define the Host Service API shape for Agent Capability Service.
- Adapt the current Agent Execution Engine and daemon-backed session APIs.
- Preserve existing session/TUI behavior while adding a semantic Agent Run path.
- Map Context Grants and Result Contracts to current inputs and outputs.
- Surface streaming, yields, tool calls, child runs, evidence, and audit in
  Agent Run terms.

**Non-Goals:**

- Rewrite the Agent Execution Engine.
- Move execution into Alan Kernel.
- Ship a resident System Agent Supervisor.
- Require domain apps to open Alan Agent UI for ordinary app assistance.
- Remove existing daemon session endpoints.

## Decisions

### 1. Adapter wraps current execution first

The first implementation should call the existing runtime/session machinery and
translate at the boundary. This keeps proven governance, approvals, sandbox, and
rollout behavior intact.

### 2. Context Grants are the OS input

Apps pass Context Grants, not raw prompt dumps. The adapter may assemble prompts
internally for the current Agent Execution Engine, but that is not the public
contract.

### 3. Result Contracts are the OS output

The adapter maps current streamed text, tool summaries, child runs, artifacts,
evidence, and terminal state into Result Contract fields. If the current engine
cannot satisfy a field structurally yet, the adapter marks it unsupported or
partial instead of hiding it in plain text.

### 4. Existing session endpoints remain compatibility paths

Daemon session APIs continue to serve Alan TUI and existing clients. Agent
Capability Service can initially sit beside those APIs, then share more
implementation as the semantic path proves parity.

### 5. Internal Rust API comes before daemon protocol

The first slice defines an internal Rust Host Service API in the `alan` crate:
request/response/event shapes plus a service trait for start, event reads,
resume, cancel, and adapter-recorded completion. It does not add public daemon
HTTP or WebSocket routes yet. Later daemon routes should wrap this boundary
after the compatibility adapter proves mapping parity.

Alternative considered: add `/api/v1/agent_capabilities/*` immediately. That
would expose a still-moving contract before Context Grant translation, Result
Contract reporting, and compatibility behavior have real fixture coverage.

## Risks / Trade-offs

- [Risk] Adapter leaks session-specific concepts. -> Keep Agent Run and Result
  Contract as the external API and mark raw session ids as native references.
- [Risk] Current engine cannot satisfy every result field. -> Return partial
  structured results with evidence and gaps.
- [Risk] Host Service API grows too broad. -> Start with V1 descriptors only.
