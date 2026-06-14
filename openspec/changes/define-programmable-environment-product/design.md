## Context

This change defines a product constitution for a new product line inside the
alan repo. The product is a programmable personal computing environment:
local-first, UNIX-respecting, object-oriented, command-driven, extensible, and
agent-native.

The current Alan product already has useful ideas around agents, terminal-first
work, permissions, session state, and OpenSpec-driven planning. Those are
references, not constraints. This product line should not be forced into the
current macOS shell, daemon, or terminal-only surface.

The key product boundary is that data ownership and activity organization are
separate. Data may remain in files, directories, Git repositories, external
services, databases, or OS resources. The environment organizes activity around
runtime objects, commands, buffers, views, queries, humans, agents, and
extensions.

## Goals / Non-Goals

**Goals:**

- Define the product identity before implementation begins.
- Treat the product as an independent product line within the repo.
- Establish local-first, filesystem-first, UNIX-like composition as a core
  principle.
- Define object, command, buffer, view, and query as the core runtime
  abstractions.
- Make human and agent first-class actors in the same runtime.
- Require ordinary UI for out-of-the-box use and modal grammar for power use.
- Make Rust core plus WASM Component Model extensibility part of the product
  identity.
- Define incubation criteria without freezing the first interface or MVP.

**Non-Goals:**

- Specify a concrete implementation architecture, crate layout, UI, or storage
  engine.
- Merge this product into the current Alan macOS shell.
- Require all data to be imported into a private object store.
- Define a universal URI/resource protocol.
- Replace current Alan runtime, daemon, or shell behavior.
- Choose the first visual interface between editor-like, object workspace, or
  agent workspace forms.

## Decisions

### 1. Product constitution first

The first OpenSpec should define what the product is, not how to implement it.
The idea is broad enough that an implementation-oriented first change would
collapse it into whichever subsystem is easiest to build first.

Alternative considered: write an architecture RFC for object graph, command
registry, buffer/view, query, WASM, and agent actors. That is useful later, but
too early for the first artifact because it would prematurely freeze technical
interfaces.

### 2. Independent product line inside the repo

The product should live as a new product line in this repo while its identity is
still being formed. It can reuse Alan engineering practices and later share
runtime pieces where appropriate, but it is not a feature of the existing
terminal shell.

Alternative considered: describe this as Alan's long-term evolution. That would
make current Alan shell concerns too influential and would encourage forced
integration before the product boundary is clear.

### 3. Local-first and filesystem-first

The environment should respect UNIX philosophy: simple artifacts, inspectable
state, ordinary files and directories where possible, composable commands, and
clear capability boundaries. The runtime may add object identity, metadata,
relationships, history, views, queries, and permissions, but it should not start
by hiding user work behind a proprietary object store or complex URI layer.

Alternative considered: define a unified resource addressing model from the
start. That would be powerful, but it is too platform-shaped for the
constitution and would obscure the simpler local-first boundary.

### 4. Activity happens in the environment; data can stay where it is

The long-term goal is for users to do more of their daily work inside the
environment. That does not require the environment to own all data. Files,
projects, tasks, terminal sessions, notes, Git state, mail, calendar items, and
remote records can be represented as objects while their source of truth remains
elsewhere.

Alternative considered: make the environment's private workspace the primary
storage model. That may become useful for some object types, but it should not
be the first product principle.

### 5. Ordinary UI plus modal power layer

Out-of-the-box use needs ordinary UI: users should be able to browse objects,
open buffers, switch views, run commands, and ask agents without first learning
a grammar. Modal interaction remains part of the product identity as a power
layer. Ordinary UI, command palette, automation, modal grammar, and agents all
route through the same command model.

Alternative considered: make modal interaction the mandatory primary interface.
That would protect the Vim-like model but conflict with the out-of-the-box
principle.

### 6. Agents are actors, not a chat plugin

Agents should participate through the same runtime abstractions as humans. They
can query, inspect, create buffers, propose changes, and execute commands, but
they do not bypass runtime permissions, command mediation, or audit surfaces.

Alternative considered: make agents an overlay on the human UI. That would be
easier to add to an existing interface, but it would miss the product premise:
human and agent operate in one environment.

### 7. WASM extensibility is part of the identity

The product should inherit the programmability spirit of Emacs without adopting
Lisp as the embedded substrate. Rust core plus WASM Component Model and WIT
interfaces should be treated as the extension direction, with explicit
capability grants.

Alternative considered: leave extension technology to later RFCs. That would
keep the constitution more abstract, but extensibility without Lisp is a core
differentiator in the original idea.

## Risks / Trade-offs

- [Risk] A product constitution can stay too abstract to guide work. →
  Include incubation criteria that future MVP/spec work must satisfy.
- [Risk] Filesystem-first can be mistaken for "only files". → State that
  external systems may be projected as objects, while filesystem remains the
  default substrate and inspection boundary.
- [Risk] Independent product line can drift away from Alan's engineering
  strengths. → Allow reuse of Alan concepts where they fit, but do not force
  current shell integration.
- [Risk] Modal grammar can create onboarding friction. → Require ordinary UI to
  be sufficient for first-use workflows.
- [Risk] WASM as identity can overconstrain the first implementation. → Keep
  the constitution at the principle level; detailed WIT interfaces belong in
  later architecture changes.
- [Risk] Agent-native can be misread as an AI chat app. → Require agents to act
  through object, command, buffer, view, query, permission, and audit surfaces.

## Incubation Path

The next product work should validate product assumptions rather than build a
complete environment. A future incubation or MVP change should prove:

1. A first launch can reveal and organize real local work from the filesystem or
   an existing project/workspace.
2. One real personal workflow can move through file/project/task/terminal
   objects, buffers, views, commands, queries, and agent help.
3. Ordinary UI and modal grammar execute the same command surface.
4. Agent actions are mediated by runtime abstractions and leave inspectable
   results.
5. Extension boundaries are capability-shaped from the start, even if the first
   prototype only exposes a small subset.

## Open Questions

- What is the product name?
- What is the first concrete interface: editor-like environment, object
  workspace, agent workspace, or another form?
- Which local-first object types are mandatory for the first prototype?
- Which commands prove the shared command model without overbuilding the
  registry?
- Which WASM extension hook should be validated first?
