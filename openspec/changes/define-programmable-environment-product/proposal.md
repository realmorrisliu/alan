## Why

Alan needs a repo-level product constitution: Alan is the programmable personal
computing environment for humans and agents. "Programmable environment" is the
category description, not a second product name. The durable product name is
Alan, and the OS-level substrate is Alan OS.

The constitution must protect Alan from being reduced to the current macOS
shell, terminal UI, HTTP server, agent chat UI, or one implementation plan. It
also must avoid drifting into an overbuilt semantic platform. The current target
is deliberately UNIX- and Plan 9-shaped: everything important is visible as
files, processes, descriptors, namespaces, and file-server services.

Alan Agent, Alan Shell, Alan for macOS, Groove Master, UPDF-like workflows, and
future Alan Apps should all be able to describe how they run on Alan OS rather
than defining their own private runtime model.

## What Changes

- Define Alan as the repo-level programmable personal computing environment and
  Alan OS as its operating-system boundary.
- Define the core product model around namespace, mounts, files, descriptors,
  access rights, a single `Process` category (agents are processes recognized by
  the agent file layout, not a separate kernel kind), file-server services,
  Service Manager, the standard namespace, AgentFS, Tools, Skills, Memory
  Stores, and app/host integration conventions.
- Establish "everything is file" as the default design rule for Alan OS: richer
  objects, buffers, views, queries, evidence, artifacts, and tasks are app,
  agent, service, or host interpretations over files and processes rather than
  Kernel primitives.
- Define Agent Process as the standard OS execution form for agent work. Apps
  request AI-mediated behavior by spawning Agent Executables with descriptors,
  not by calling an RPC-style agent API.
- Define Agent Runtime Service as a Plan 9-style file-server service managed by
  Service Manager. It serves AgentFS at `/agent` and executes Agent Processes.
- Define Alan Agent as a built-in but optional Agent Workspace app for
  inspecting, steering, and organizing Agent Processes. It is not the Root Agent
  Process, not Agent Runtime Service, and not required to run agents.
- Define Alan Shell as the primary Alan OS interaction surface, closer to a
  shell over namespaces/files/processes than to a compatibility-client product
  concept.
- Define Tool as an executable command file, with `--help`, `/man/1/<tool>`,
  and `/lib/exec/<tool>/manifest`; define Skill as manual-like knowledge
  installed under `/lib/skill/<name>` and documented under
  `/man/skill/<name>`.
- Define the canonical roadmap sequence: finish the Alan OS spine, migrate Alan
  Agent onto Agent Processes, migrate Alan Shell and Alan for macOS as hosts,
  then bring Groove Master and UPDF-like workflows onto Alan OS as real apps.
- Require future Alan specs, and selected active specs as touched, to identify
  their Alan OS role, file/process mapping, source-of-truth boundary, host
  boundary, and compatibility boundary.

## Capabilities

### New Capabilities

- `programmable-environment-product`: Owns the product constitution for Alan as
  the alan repo's programmable personal computing environment, including Alan
  OS roles, file/process/service principles, agent-native participation, app
  and host boundaries, extension direction, spec-alignment rules, and roadmap
  sequencing.

### Modified Capabilities

- None. This change intentionally does not modify current Agent Execution
  Engine, HTTP/WS compatibility transport, terminal, CLI, or macOS behavior.

## Impact

- Affected product planning: introduces Alan OS as the repo-level product
  constitution.
- Affected architecture planning: future Kernel, service, app, agent, host,
  adapter, and extension proposals should trace back to the file/process model
  or explicitly declare themselves legacy/compatibility work.
- Affected roadmap planning: implementation sequencing should preserve the Alan
  OS spine -> Alan Agent -> hosts -> domain apps order unless a future spec
  justifies a narrower compatibility exception.
- Affected agent planning: agent-enabled apps should align with
  `define-agent-process-os-model` by using Agent Processes, Agent Executables,
  Agent Runtime Service, AgentFS, Tools, Skills, descriptors, and access rights.
- Affected OpenSpec maintenance: selected active specs should be aligned over
  follow-up changes by adding Alan OS role, file/process mapping, and
  source-of-truth boundary notes. For example,
  `add-macos-shell-component-system` should be classified as a macOS host
  surface/design-system capability, not Alan Kernel.
- Affected current code: none in this change.
