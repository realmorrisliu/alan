## 1. Product Constitution

- [x] 1.1 Capture Alan as the repo-level programmable personal computing
  environment for Alan Agent, Alan for macOS, Alan Shell, Alan Apps, hosts,
  adapters, and future products.
- [x] 1.2 Define Alan OS as the operating-system boundary for Alan rather than
  a CLI, daemon, HTTP/WS server, TUI, macOS app, or agent engine.
- [x] 1.3 Record that Alan OS must stay UNIX- and Plan 9-shaped: files,
  descriptors, processes, namespaces, mounts, and file-server services first.
- [x] 1.4 Record the data/activity boundary: data may remain in existing
  filesystems and services while Alan organizes work through mounted files,
  process surfaces, app files, and host views.
- [x] 1.5 Preserve out-of-the-box usefulness as a first principle.

## 2. Product Family And Spec Alignment

- [x] 2.1 Define the product-family role model: Alan Kernel, Service Manager,
  file-server service, built-in Alan App, Alan App, host/frame surface, native
  adapter, compatibility transport, and legacy surface.
- [x] 2.2 Require future and aligned existing specs to state their Alan OS role,
  file/process mapping, native source-of-truth boundary, host/rendering
  boundary, and deferred compatibility boundary.
- [x] 2.3 Classify `introduce-alan-kernel-runtime` as a Kernel incubation slice,
  not the whole Alan product and not middleware between agent runtime and host.
- [x] 2.4 Classify Alan Agent as a built-in optional Agent Workspace over Agent
  Processes.
- [x] 2.5 Classify Alan Shell and Alan for macOS as hosts responsible for
  interaction, rendering, layout, native chrome, and mounting Alan OS surfaces.
- [x] 2.6 Classify `add-macos-shell-component-system` as a macOS host
  surface/design-system capability, not Alan Kernel.
- [x] 2.7 Classify Groove Master and future app products as Alan Apps with
  app-owned domain cores plus Alan OS adapters.
- [x] 2.8 Align the constitution with `define-agent-process-os-model` by making
  Agent Process the standard OS execution form for agent work.

## 3. File, Process, Agent, Tool, And Skill Contracts

- [x] 3.1 Define Alan Kernel around its primitives only — namespace engine,
  mounts, files, descriptors, access rights, credentials, a single `Process`
  category (agent-ness is a file-layout convention; ADR-0024 D3), the process
  table, `/proc`, and `/srv`. `/agent`, `/bin`, `/lib`, `/man`, and `/mnt` are
  standard-namespace trees assembled by file-server services above the kernel
  (ADR-0024 D9), not part of `alan-kernel`.
- [x] 3.1a Define standard namespace layering so Alan-specific packages and
  mounted service trees live under `/lib` or `/mnt`, not new top-level roots by
  default.
- [x] 3.2 Define Service Manager as the canonical lifecycle manager for system
  file-server services and boot units.
- [x] 3.3 Define Agent Runtime Service as the file-server service that executes
  Agent Processes and serves AgentFS at `/agent`.
- [x] 3.4 Define Root Agent Process as the root of the agent-process tree, not
  OS PID 1, Service Manager, Alan Agent, or a global chat session.
- [x] 3.5 Define Agent Executable, Tool, Skill, Memory Store, policy descriptor,
  and app context as descriptor/file concepts rather than RPC-style agent API
  concepts.
- [x] 3.6 Define current `alan-runtime` as the current Agent Execution Engine
  that can back Agent Runtime Service, not Alan OS or Alan Kernel.
- [x] 3.7 Define the current HTTP/WS server as compatibility transport and
  legacy service implementation during migration.
- [x] 3.8 Define Alan Apps as real user-facing products rather than OS demos.
- [x] 3.9 Define Rust core plus WASM Component Model and WIT interfaces as the
  extension direction with explicit descriptor/access grants.

## 4. Incubation Decomposition

- [x] 4.1 Identify that first follow-up changes should validate product
  assumptions rather than implement the complete Alan OS.
- [x] 4.2 Require the first follow-up prototype or MVP to prove local-first
  discovery of real work.
- [x] 4.3 Require the first follow-up prototype or MVP to prove one workflow
  through files, process surfaces, commands, views, host rendering, and agent
  participation.
- [x] 4.4 Require host surfaces used in incubation to render Alan state without
  owning Alan Kernel or app truth.
- [x] 4.5 Defer universal URI/resource addressing until filesystem-first and
  namespace/mount composition prove insufficient.
- [x] 4.6 Record the canonical roadmap order: Alan OS spine, Alan Agent as first
  built-in app, Alan Shell and Alan for macOS as hosts, Groove Master as first
  domain Alan App, and UPDF as complex content Alan App.

## 5. Existing-Spec Alignment Backlog

- [x] 5.1 Add an alignment note to `introduce-alan-kernel-runtime` identifying
  it as a Kernel incubation slice and recording what it proves or defers.
- [x] 5.2 Plan a follow-up review of `add-macos-shell-component-system` to add
  its macOS host-surface/design-system role without broadening it into runtime
  core.
- [x] 5.3 Plan a follow-up review of `define-groove-master-environment-app` so
  its app/domain-core/adapter split remains aligned after the constitution is
  accepted.
- [x] 5.4 Plan future reviews for UPDF-like product specs, terminal/content
  specs, and agent execution specs when they are next touched.

## 6. Verification

- [x] 6.1 Run `openspec validate define-programmable-environment-product --strict`.
- [x] 6.2 Run `git diff --check -- openspec/changes/define-programmable-environment-product`.
- [x] 6.3 Review proposal, design, specs, and tasks for placeholders,
  contradictions, premature implementation commitments, and unclear product
  boundaries.
- [x] 6.4 Confirm the change does not modify current Agent Execution Engine,
  HTTP/WS compatibility transport, terminal, CLI, or macOS behavior.

## 7. Archive Readiness

- [ ] 7.1 After review and merge, sync
  `programmable-environment-product` into `openspec/specs/`.
- [ ] 7.2 Archive the completed change only after the long-lived spec has been
  updated and validated.
