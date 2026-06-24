## Context

Agent Capability is an OS ability, but execution belongs behind Host
Service APIs. The first code slice therefore needs a small Kernel vocabulary
that apps, hosts, and service adapters can share without linking Kernel to the
current Agent Execution Engine.

## Goals / Non-Goals

**Goals:**

- Add typed Kernel identities and descriptors for Agent Capability.
- Model bounded Agent Runs without implementing execution.
- Model Context Grants and Result Contracts without prompt dumps.
- Model command risk, effect classes, execution guard metadata, evidence, and
  audit references in a way Command Governance can later consume.
- Keep dependency boundaries clean.

**Non-Goals:**

- Implement Agent Capability Service.
- Start model/provider execution.
- Connect to daemon sessions, `alan-runtime`, `alan-protocol`, memory storage,
  sandbox backends, or TUI rendering.
- Implement System Agent Supervisor runtime behavior.

## Decisions

### 1. Kernel owns semantic shape only

Kernel types describe what an Agent Capability request and result mean. They do
not execute work, hold provider clients, or supervise sessions.

### 2. Descriptor taxonomy is small

The first descriptor set is `explain`, `summarize`, `plan`,
`propose_commands`, and `delegate`. `transform` and `remember` are deferred
until draft-object/edit and memory-write contracts are ready.

### 3. Context Grants and Result Contracts are structured

Context Grants should name app identity, target refs, view refs, selected
ranges, allowed reads, allowed commands, privacy policy, and evidence
requirements. Result Contracts should request typed answer, summary, plan,
proposed commands, citations, evidence, follow-up questions, uncertainty, and
audit summary fields.

### 4. Execution Guard is metadata here

Kernel can record requested or observed guard strength, guard kind, target
scope, and auditability. Concrete sandboxing, workspace path guards, app object
guards, and human approval gates are Host Service implementation concerns.

## Risks / Trade-offs

- [Risk] Types become too abstract. -> Keep V1 descriptor taxonomy narrow and
  test it with representative app requests.
- [Risk] Kernel accidentally imports runtime dependencies. -> Add dependency
  boundary tests.
- [Risk] Result Contracts overfit text answers. -> Include citations, evidence,
  proposed commands, uncertainty, and audit summary from the start.

