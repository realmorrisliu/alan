## Context

This change defines the product constitution for the alan repo. The long-term
product model is a programmable personal computing environment: local-first,
UNIX-respecting, object-oriented, command-driven, extensible, and agent-native.

This is larger than adding a new product beside Alan. Alan agent, Alan for
macOS, future environment apps such as Groove Master, host surfaces, runtime
substrates, and adapters should all be able to describe their role in this
environment. The constitution is therefore a repo-level organizing model, not a
current implementation boundary.

The current Alan product already has useful ideas around agents, terminal-first
work, permissions, session state, shell workspaces, content instances, and
OpenSpec-driven planning. Those are real assets, but they are not allowed to
define the whole environment by accident. The programmable environment should
not be forced into the current macOS shell, daemon, Rust TUI, or terminal-only
surface. Conversely, existing Alan work should not be treated as unrelated to
the environment direction. It should be classified and aligned deliberately.

The key product boundary is that data ownership and activity organization are
separate. Data may remain in files, directories, Git repositories, external
services, databases, or OS resources. The environment organizes activity around
runtime objects, commands, buffers, views, queries, humans, agents, extensions,
and host surfaces.

## Goals / Non-Goals

**Goals:**

- Define the repo-level product identity before implementation expands.
- Treat the programmable environment as the long-term organizing model for Alan
  agent, Alan for macOS, environment apps, hosts, adapters, and future products.
- Preserve the product boundary from being collapsed into the current macOS
  shell, TUI, daemon, or agent session protocol.
- Establish local-first, filesystem-first, UNIX-like composition as a core
  principle.
- Define object, command, buffer, view, and query as the core runtime
  abstractions.
- Make human and agent first-class actors in the same runtime.
- Require ordinary UI for out-of-the-box use and modal grammar for power use.
- Make Rust core plus WASM Component Model extensibility part of the product
  identity.
- Define how existing and future OpenSpec changes declare their environment
  role and migration boundary.
- Define incubation criteria without freezing the first interface or MVP.

**Non-Goals:**

- Specify a concrete implementation architecture, crate layout, UI, or storage
  engine for the entire environment.
- Replace current Alan runtime, daemon, Rust TUI, terminal, or macOS shell
  behavior in this change.
- Treat the current Alan shell as the mandatory implementation boundary for the
  environment.
- Require all data to be imported into a private object store.
- Define a universal URI/resource protocol.
- Rewrite every existing OpenSpec in this change.
- Choose the first complete visual interface between editor-like environment,
  object workspace, agent workspace, app launcher, or another form.

## Product Family Model

Future specs should classify themselves against this map:

```text
Programmable Environment Constitution
  |
  +-- Environment core
  |     objects, commands, buffers, views, queries, ledgers,
  |     permissions, extension capabilities
  |
  +-- Agent runtime / actors
  |     Alan agent sessions, tools, child runs, plans, memory,
  |     task execution, governed side effects
  |
  +-- Host surfaces
  |     Alan for macOS, Rust TUI, future iOS/iPadOS/web hosts;
  |     physical layout, native chrome, input, rendering, windowing
  |
  +-- Environment apps
  |     Groove Master, UPDF-like workflows, future domain products;
  |     app-owned domain models plus environment adapters
  |
  +-- Adapters
  |     filesystem, Git, terminal, mail, calendar, databases,
  |     remote services, model/provider systems
  |
  +-- Legacy / compatibility surfaces
        current implementation paths that must stay stable while
        they are adapted, retired, or reclassified
```

This model lets a spec be concrete without pretending to be the whole product.
For example:

- `introduce-workbench-runtime` is a candidate environment-core/runtime
  substrate incubation slice.
- Alan agent work is agent runtime/actor work and should project into objects,
  commands, buffers, views, queries, tasks, artifacts, and evidence rather than
  remaining only a chat/session UI.
- Alan for macOS is a native host surface. Its Spaces, Tabs, PaneSlots,
  ContentInstances, command menus, and design system are host responsibilities
  that mount and render environment objects/buffers/views.
- `add-macos-shell-component-system` is a macOS host surface/design-system
  capability. It can make environment views coherent in the macOS host, but it
  is not environment core.
- Groove Master is an environment app. Its domain core owns musical practice
  behavior while an Alan adapter maps sessions, loops, journals, and producer
  agent behavior into environment abstractions.

## Decisions

### 1. Product constitution first

The first OpenSpec should define what the product family is, not how to
implement it. The idea is broad enough that an implementation-oriented first
change would collapse it into whichever subsystem is easiest to build first.

Alternative considered: write an architecture RFC for object graph, command
registry, buffer/view, query, WASM, and agent actors. That is useful later, but
too early for the first artifact because it would prematurely freeze technical
interfaces.

### 2. Repo-level organizing model, not a side product

The programmable environment should become the repo's long-term product model.
That does not mean every current Alan subsystem is immediately rewritten. It
means every substantial future spec should know whether it is environment core,
agent runtime, host surface, environment app, adapter, or legacy/compatibility
work.

Alternative considered: describe the programmable environment as an independent
new product line next to Alan. That protected the idea from being swallowed by
the current shell, but it was too weak: the intended direction is broader than a
side product. It should organize Alan itself.

### 3. Current Alan is source material, not the product boundary

Current Alan agent, daemon, macOS shell, Rust TUI, terminal runtime, and
OpenSpec workflow are valid starting points. They should be mined for durable
concepts and kept stable while migration happens. They should not force the
environment to inherit their current boundaries, naming, or UI shape.

For instance, a macOS `ContentInstance` is a host/content mounting concept. It
may later mount an environment buffer or view, but it is not the universal
environment object model. Similarly, an Alan agent session is an agent runtime
authority that can project into environment tasks, buffers, and views; it is not
the whole product model.

### 4. Specs need environment alignment metadata

Future specs, and selected active specs as they are touched, should include a
short alignment statement:

```text
Environment role: core | agent-runtime | host-surface | environment-app |
  adapter | legacy-compat
Runtime mapping: objects / commands / buffers / views / queries / actors
Native authority: which file, service, runtime, domain store, or host owns truth
Host boundary: what is renderer/window/layout/input-specific
Deferred migration: what stays compatibility-only for now
```

This prevents two common failures: host-specific specs accidentally defining the
whole product, and product/app specs ignoring real host and runtime constraints.

### 5. Local-first and filesystem-first

The environment should respect UNIX philosophy: simple artifacts, inspectable
state, ordinary files and directories where possible, composable commands, and
clear capability boundaries. The runtime may add object identity, metadata,
relationships, history, views, queries, and permissions, but it should not start
by hiding user work behind a proprietary object store or complex URI layer.

Alternative considered: define a unified resource addressing model from the
start. That would be powerful, but it is too platform-shaped for the
constitution and would obscure the simpler local-first boundary.

### 6. Activity happens in the environment; data can stay where it is

The long-term goal is for users to do more of their daily work inside the
environment. That does not require the environment to own all data. Files,
projects, tasks, terminal sessions, notes, Git state, mail, calendar items,
recordings, and remote records can be represented as objects while their source
of truth remains elsewhere.

Alternative considered: make the environment's private workspace the primary
storage model. That may become useful for some object types, but it should not
be the first product principle.

### 7. Ordinary UI plus modal power layer

Out-of-the-box use needs ordinary UI: users should be able to browse objects,
open buffers, switch views, run commands, and ask agents without first learning
a grammar. Modal interaction remains part of the product identity as a power
layer. Ordinary UI, command palette, automation, modal grammar, and agents all
route through the same command model.

Alternative considered: make modal interaction the mandatory primary interface.
That would protect the Vim-like model but conflict with the out-of-the-box
principle.

### 8. Agents are actors, not a chat plugin

Agents should participate through the same runtime abstractions as humans. They
can query, inspect, create buffers, propose changes, and execute commands, but
they do not bypass runtime permissions, command mediation, or audit surfaces.

Alternative considered: make agents an overlay on the human UI. That would be
easier to add to an existing interface, but it would miss the product premise:
human and agent operate in one environment.

### 9. Environment apps are real products

Domain products such as Groove Master should be treated as real apps inside the
environment, not demos whose primary job is to prove Alan. Their domain cores
own product logic. The environment adapter maps their objects, commands,
buffers, views, queries, and agent behavior into the shared runtime.

Alternative considered: build demo apps primarily to exercise platform
features. That produces shallow examples and lets platform needs dominate the
user-facing product.

### 10. WASM extensibility is part of the identity

The product should inherit the programmability spirit of Emacs without adopting
Lisp as the embedded substrate. Rust core plus WASM Component Model and WIT
interfaces should be treated as the extension direction, with explicit
capability grants.

Alternative considered: leave extension technology to later RFCs. That would
keep the constitution more abstract, but extensibility without Lisp is a core
differentiator in the original idea.

## Risks / Trade-offs

- [Risk] A repo-level constitution can stay too abstract to guide work. ->
  Include alignment metadata and incubation criteria that future specs must use.
- [Risk] Existing Alan specs could be invalidated too broadly. -> Classify and
  migrate specs incrementally; do not rewrite current behavior in this change.
- [Risk] Host surfaces may accidentally define environment core. -> Require
  host specs to name host-owned concerns such as layout, rendering, windowing,
  input, and native chrome.
- [Risk] Product apps can become platform demos. -> Require environment apps to
  state their user-facing product boundary before their Alan adapter.
- [Risk] Filesystem-first can be mistaken for "only files". -> State that
  external systems may be projected as objects, while filesystem remains the
  default substrate and inspection boundary.
- [Risk] Modal grammar can create onboarding friction. -> Require ordinary UI
  to be sufficient for first-use workflows.
- [Risk] WASM as identity can overconstrain the first implementation. -> Keep
  the constitution at the principle level; detailed WIT interfaces belong in
  later architecture changes.
- [Risk] Agent-native can be misread as an AI chat app. -> Require agents to
  act through object, command, buffer, view, query, permission, and audit
  surfaces.

## Incubation Path

The next product work should validate product assumptions rather than build a
complete environment. A future incubation or MVP change should prove:

1. A first launch can reveal and organize real local work from the filesystem or
   an existing project/workspace.
2. One real personal workflow can move through file/project/task/terminal or
   app-domain objects, buffers, views, commands, queries, and agent help.
3. Ordinary UI and modal grammar execute the same command surface.
4. Agent actions are mediated by runtime abstractions and leave inspectable
   results.
5. Extension boundaries are capability-shaped from the start, even if the first
   prototype only exposes a small subset.
6. Host surfaces can render the workflow without owning the product runtime.
7. Existing Alan subsystems used by the slice are classified as core, agent,
   host, app, adapter, or compatibility boundaries.

Candidate sequencing:

1. Accept this constitution as the long-lived spec baseline.
2. Add alignment notes to active specs that are likely to shape the
   environment, starting with `introduce-workbench-runtime`,
   `add-macos-shell-component-system`, `define-groove-master-environment-app`,
   and future UPDF-style product specs.
3. Implement the workbench runtime as a small environment-core substrate slice,
   not as the full product.
4. Prove one end-to-end workflow through a real host and a real product or
   workspace surface.

## Open Questions

- What is the product name?
- Which existing active specs should be aligned before the constitution is
  archived, and which should wait until touched?
- What is the first concrete interface: editor-like environment, object
  workspace, agent workspace, app workspace, or another form?
- Which local-first object types are mandatory for the first prototype?
- Which commands prove the shared command model without overbuilding the
  registry?
- Which WASM extension hook should be validated first?
