## Why

Alan OS needs agents to be first-class operating-system citizens without
inventing an API layer that sits beside files and processes. The previous
Agent Capability / Agent Run model was too service-shaped and drifted away from
the Plan 9 direction: services export file trees, processes are created by
system calls, and users or apps interact by mounting, opening, reading, writing,
watching, and spawning.

This change reframes agent work around Agent Processes, AgentFS, Service
Manager, file-server services, Tools as executables, and Skills as manual-like
knowledge packages. Existing Alan Agent runtime work remains valuable, but it
must migrate into the Agent Runtime Service and Agent Process file model rather
than become a product-specific backend or HTTP service.

## What Changes

- Define an Agent Process as an ordinary `Process` recognized by the agent file
  layout, NOT a kernel category. The kernel has a single `Process` category
  (ADR-0024 D3); agent-ness is a file-layout/AgentFS convention discovered by
  walking the process directory.
- Define Root Agent Process as the always-available root of the agent process
  tree, exposed through `/agent/root`, not as root permission or a global chat
  session.
- Define Service Manager as the canonical lifecycle manager replacing the
  former daemon concept. System services are long-running Processes that export
  file trees.
- Adopt Plan 9-style service exposure: file servers post mountable handles under
  `/srv`; canonical service trees are mounted into the namespace, such as
  `/agent`, `/proc`, and `/mnt/service`.
- Define Agent Runtime Service as the file-server service that executes Agent
  Processes and serves AgentFS at `/agent`.
- Retire Agent Capability as a core model. Ability entrypoints are Agent
  Executables, Tools are executable files, Skills are manual-like packages, and
  authority is Descriptor + Access Rights + policy.
- Replace Context Grant and Result Contract API language with descriptor
  passing, Agent IO, result files, request files, action files, and exit status.
- Reframe Session as compatibility terminology decomposed into Agent Process
  status, IO streams, machine tape, machine events, and checkpoints.
- Clarify that Alan Agent is built-in but optional: an Agent Workspace app over
  Agent Processes, not the required path to run agents.

## Capabilities

### New Capabilities

- `agent-process-os-model`: Defines Alan OS's agent model as a file-layout
  convention over a single `Process` category (not a kernel type), plus AgentFS,
  Service Manager, file-server services, Root Agent Process, Agent Executables,
  Tools, Skills, namespace-bound context, requests, actions, machine state, and
  migration of existing Alan Agent runtime concepts.

### Modified Capabilities

- None.

## Impact

- Affected OpenSpec planning: follow-up changes should use Agent Process,
  AgentFS, Service Manager, Tool, Skill, Descriptor, and Access Rights language
  instead of Agent Capability Service, Agent Run, Context Grant, Result
  Contract, or HTTP/WS session transport as target architecture.
- Affected architecture: `alan-kernel` should define a single `Process` identity
  plus file/descriptor/access primitives; agent-ness is AgentFS/file-layout
  conformance above the kernel (ADR-0024 D3), while Agent Runtime Service serves
  `/agent` and owns Turing-machine execution.
- Affected current implementation: the current HTTP/WS compatibility server and
  session APIs become legacy compatibility transport/implementation details to
  be retired from product concepts.
- Affected Alan Agent migration: existing session, tape, tool, skill, policy,
  sandbox, memory, child-agent, rollout, and conversation behavior must map to
  Agent Process files, executable Tools, descriptor-passed Skills, Memory Store
  descriptors, request files, action files, and optional workspace UI.
