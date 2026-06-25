## Context

`crates/tui` is currently a compatibility Alan Shell path over the existing
session transport. It owns terminal input, attaches to current runtime sessions,
hydrates history, streams events, and submits operations. That works, but it is
not the Alan OS target model.

Alan OS now follows a Plan 9-style design: services are file-server Processes,
service handles are posted under `/srv`, useful service trees are mounted into a
process namespace, and users/apps compose work by opening files and spawning
processes. Agent work is represented by first-class Agent Processes, not API
objects or run records beside the process table.

This change remains the first Kernel spine slice. Its job is to create the
small substrate that later Agent Runtime Service, AgentFS, Alan Shell, optional
Alan Agent workspace, Alan for macOS, Groove Master, and UPDF can use.

## Goals / Non-Goals

**Goals:**

- Define Alan Kernel around namespace, mounts, paths, files, descriptors, access
  rights, credentials, ordinary Processes, Agent Processes, and the process
  table.
- Define streams as file kinds that can be read, tailed, watched, and resumed
  from offsets.
- Define `/proc`, `/srv`, and AgentFS anchors without implementing their full
  service backends.
- Keep Artifact, Evidence, Task, View, Command, Query, Subscription, and
  Snapshot language above Kernel as app/service/host interpretations over files
  and processes.
- Keep the current compatibility session path working while projection moves
  toward Agent Process status, IO, request, action, and machine files.
- Keep `alan-kernel` independent from protocol, renderer, provider, runtime,
  memory store, sandbox, and transport implementation crates.

**Non-Goals:**

- Implement Service Manager, Agent Runtime Service, AgentFS, `/srv`, `/proc`, or
  a real 9P-like protocol in this slice.
- Implement model/provider execution, memory storage, tool execution, sandbox
  execution, or agent policy evaluation inside Kernel.
- Make Alan Agent required for agent work.
- Remove the current compatibility transport immediately.

## Decisions

### 1. Alan Kernel is a small semantic UNIX substrate

Kernel concepts must be explainable as namespace, mount, path, file, stream
file, descriptor, access right, credential, Process, Agent Process, or process
table entry. Higher-level names can exist in compatibility adapters, but they
must resolve back to files and processes when they need durable semantics.

Alternative considered: keep V1 object/task/view/command/query/subscription ids
as durable Kernel ontology. That would create a private object system beside the
file tree.

### 2. Agent Process is a Kernel-visible process category

Agent Process belongs in Kernel because agent is an OS-level execution form.
Kernel still only owns minimal anchors: process identity, parentage,
credentials, descriptors, lifecycle, status, and access checks. AgentFS schemas,
tape, model calls, requests, actions, Tool manifests, Skill packages, memory,
policy, and execution guards stay above Kernel.

Alternative considered: keep agents as ordinary Processes with no Kernel-visible
agent distinction. That would make Root Agent Process and child Agent Process
trees second-class conventions.

### 3. Services are mounted file trees, not Kernel modules

Service Manager, Agent Runtime Service, credential/profile services, memory
services, and package services are file-server Processes. Kernel may define
mount and handle primitives that let the standard namespace roots exist:
`/proc`, `/agent`, `/srv`, `/bin`, `/lib`, `/man`, and `/mnt`. Concrete service
behavior belongs to mounted services.

Alternative considered: define Host Service APIs as the canonical boundary.
That retains the current server/API mental model instead of moving toward Plan
9-style composition.

### 4. Current sessions are compatibility transport

The current Agent Execution Engine and session protocol should project into
Agent Process file surfaces during migration. Session metadata maps to status,
conversation maps to Agent IO, yields map to requests, tool calls map to
actions, tape maps to Agent Machine, and recovery maps to checkpoints.

Alternative considered: make session protocol the substrate. That would bias the
Kernel toward current transport events and make Alan Shell a client of a server
instead of a shell over files and processes.

### 5. Artifacts and evidence stay above Kernel

Kernel owns files, stream offsets, process ids, descriptors, and native
references. Agent/App layers may interpret those anchors as artifacts or
evidence. Kernel should not own evidence databases, artifact records, or
ProvenanceRef.

Alternative considered: make evidence a Kernel primitive. That was too
agent-specific and too broad for non-agent apps.

### 6. Operation surfaces are compatibility layers over executables and files

Commands are executable files or operation descriptors that spawn Processes or
write files. Queries inspect files or snapshots. Subscriptions watch files,
processes, or stream files. Registries can cache discovery, but the namespace is
the authority.

Alternative considered: make command/query/subscription registries the source of
truth. That weakens namespace composition.

### 7. Alan Shell is the first host path

Alan Shell should become the primary interaction surface over the standard
namespace: `/proc`, `/agent`, `/srv`, `/bin`, `/lib`, `/man`, and `/mnt`.
The current Ratatui path remains a compatibility implementation while file
surfaces reach parity.

Alternative considered: make Alan Agent the first required app path. That
couples agent execution to one product UI.

### 8. Alan Agent is optional workspace

Alan Agent can provide richer inspection, steering, and organization over Agent
Processes, requests, actions, memory, evidence, and cross-app work. It should be
built in, but not required. It is an app over files, not the runtime backend.

Alternative considered: keep Alan Agent as the mandatory agent workspace. That
would undermine Alan Shell and the OS model.

## Migration Plan

1. Keep the first Kernel slice focused on file/process/descriptor primitives and
   Agent Process anchors.
2. Add compatibility projections from current sessions into Agent Process file
   surfaces.
3. Keep `crates/tui` working while moving Alan Shell semantics toward file and
   process operations.
4. Split Agent Runtime Service and AgentFS implementation into a follow-up over
   the existing Agent Execution Engine.
5. Split Tool/Skill package layout into a follow-up that installs Tools as
   executables and Skills as manual-like file trees.
6. Treat HTTP/WS routes and current session commands as compatibility transport
   until spawn/open/watch file surfaces reach parity.

## Rollback

Rollback for this slice is removing the new Kernel substrate types and
compatibility projections while preserving current runtime/session behavior.
Because concrete runtime execution remains outside Kernel, rollback should not
affect existing model/provider execution or shell compatibility behavior.
