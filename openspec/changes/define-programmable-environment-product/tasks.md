## 1. Product Constitution

- [x] 1.1 Capture the programmable environment as the repo-level product
  constitution for Alan agent, Alan for macOS, environment apps, hosts,
  adapters, and future products.
- [x] 1.2 Define the product as a programmable personal computing environment
  organized around objects, commands, buffers, views, queries, humans, agents,
  and extensions.
- [x] 1.3 Record that the product must not collapse into the current macOS shell,
  Rust TUI, daemon, terminal workflow, or agent session UI.
- [x] 1.4 Record local-first, filesystem-first, UNIX-like composition as a core
  product principle.
- [x] 1.5 Record the data/activity boundary: data may remain in existing systems
  while activity is organized inside the environment.
- [x] 1.6 Preserve out-of-the-box usefulness as a first principle.

## 2. Product Family And Spec Alignment

- [x] 2.1 Define the product-family role model: environment core, agent
  runtime/actor, host surface, environment app, adapter, and
  legacy/compatibility surface.
- [x] 2.2 Require future and aligned existing specs to state their environment
  role, runtime abstraction mapping, native source-of-truth boundary,
  host/rendering boundary, and deferred compatibility boundary.
- [x] 2.3 Classify `introduce-workbench-runtime` as a candidate environment-core
  runtime substrate incubation slice rather than the complete product.
- [x] 2.4 Classify Alan agent work as agent runtime/actor work that should project
  into environment tasks, objects, buffers, views, commands, artifacts, evidence,
  permissions, and audit surfaces.
- [x] 2.5 Classify Alan for macOS as a host surface responsible for native layout,
  windowing, input, rendering, chrome, and mounting environment views.
- [x] 2.6 Classify `add-macos-shell-component-system` as a macOS host
  surface/design-system capability, not environment core.
- [x] 2.7 Classify Groove Master and future app products as environment apps with
  app-owned domain cores plus Alan environment adapters.

## 3. Runtime Abstraction Contracts

- [x] 3.1 Define Object, Command, Buffer, View, Query, and Actor as stable
  product-level runtime abstractions.
- [x] 3.2 Define ordinary UI and modal grammar as invocation layers over the same
  command model.
- [x] 3.3 Define agents as first-class actors mediated by runtime abstractions,
  permissions, and audit surfaces.
- [x] 3.4 Define environment apps as real user-facing products rather than
  platform demos.
- [x] 3.5 Define Rust core plus WASM Component Model and WIT interfaces as the
  extension direction with explicit capability grants.

## 4. Incubation Decomposition

- [x] 4.1 Identify that the first follow-up changes should validate product
  assumptions rather than implement the complete environment.
- [x] 4.2 Require the first follow-up prototype or MVP to prove local-first
  discovery of real work.
- [x] 4.3 Require the first follow-up prototype or MVP to prove one workflow
  through object, buffer, view, command, query, and agent participation.
- [x] 4.4 Require host surfaces used in incubation to render environment state
  without owning product runtime truth.
- [x] 4.5 Defer the concrete first complete interface choice until a focused
  incubation proposal selects editor-like environment, object workspace, agent
  workspace, app workspace, or another form.
- [x] 4.6 Defer universal URI/resource addressing until a later architecture RFC
  proves it is needed beyond filesystem-first composition.

## 5. Existing-Spec Alignment Backlog

- [x] 5.1 Add an alignment note to `introduce-workbench-runtime` that identifies
  it as a runtime substrate slice and records which constitution criteria it
  proves or defers.
- [x] 5.2 Plan a follow-up review of `add-macos-shell-component-system` to add its
  macOS host-surface/design-system role without broadening it into runtime core.
- [x] 5.3 Plan a follow-up review of `define-groove-master-environment-app` so its
  app/domain-core/adapter split remains aligned after the constitution is
  accepted.
- [x] 5.4 Plan future reviews for UPDF-like product specs, terminal/content specs,
  and agent runtime specs when they are next touched.

## 6. Verification

- [x] 6.1 Run `openspec validate define-programmable-environment-product --strict`.
- [x] 6.2 Run `git diff --check -- openspec/changes/define-programmable-environment-product`.
- [x] 6.3 Review proposal, design, specs, and tasks for placeholders,
  contradictions, premature implementation commitments, and unclear product
  boundaries.
- [x] 6.4 Confirm the change does not modify current Alan runtime, daemon,
  terminal, or macOS shell behavior.

## 7. Archive Readiness

- [ ] 7.1 After review and merge, sync
  `programmable-environment-product` into `openspec/specs/`.
- [ ] 7.2 Archive the completed change only after the long-lived spec has been
  updated and validated.
