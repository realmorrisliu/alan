## Context

Agent Process is now the Alan OS primitive for agent work. Kernel should expose
only the minimal anchors that make Agent Processes first-class: process table
identity, parentage, credentials, descriptors, access checks, lifecycle, status,
and namespace/file links. The actual agent runtime remains above Kernel.

## Goals / Non-Goals

**Goals:**

- Add Process and Agent Process anchors to `alan-kernel`.
- Add mount/path/file anchors for `/proc`, `/srv`, service trees, and future
  AgentFS attachment.
- Keep compatibility ids for existing sessions as runtime references only.
- Prove `alan-kernel` remains independent from runtime, provider, transport,
  memory, and sandbox crates.

**Non-Goals:**

- Implement Agent Runtime Service.
- Implement AgentFS schemas.
- Implement Tool manifests, Skills, Memory Stores, policy evaluation, or
  request/action execution.
- Connect to current compatibility session transport.

## Decisions

### 1. Agent Process anchors are Kernel-owned

Agent Process is first-class enough that Kernel should know its process kind,
identity, parentage, credentials, descriptors, and lifecycle. Kernel should not
know model providers, tape schema, prompts, Tool schemas, or memory stores.

### 2. Service file trees are mounted, not embedded

Kernel can model paths, mounts, descriptors, and service handles that allow
the standard namespace roots `/proc`, `/agent`, `/srv`, `/bin`, `/lib`, `/man`,
and `/mnt` to exist. Concrete service behavior remains file-server service
behavior.

### 3. Compatibility ids are runtime references

Current session ids, rollout ids, protocol ids, and projection ids may appear as
runtime references during migration. They are not durable Kernel identity.

## Risks / Trade-offs

- [Risk] Kernel grows AgentFS schema too early. -> Keep request/action/tape/result
  schema above Kernel.
- [Risk] Compatibility ids become durable. -> Mark them as runtime references.
- [Risk] Tool/policy concepts drift back into Kernel. -> Keep only descriptors,
  access rights, credentials, and process anchors in this change.
