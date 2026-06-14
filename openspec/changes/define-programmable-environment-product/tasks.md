## 1. Product Constitution

- [ ] 1.1 Capture the product as an independent product line in the repo,
  separate from current Alan macOS shell and terminal workflow implementation.
- [ ] 1.2 Define the product as a programmable personal computing environment
  organized around objects, commands, buffers, views, queries, humans, agents,
  and extensions.
- [ ] 1.3 Record local-first, filesystem-first, UNIX-like composition as a core
  product principle.
- [ ] 1.4 Record the data/activity boundary: data may remain in existing systems
  while activity is organized inside the environment.
- [ ] 1.5 Preserve out-of-the-box usefulness as a first principle.

## 2. Runtime Abstraction Contracts

- [ ] 2.1 Define Object, Command, Buffer, View, and Query as stable
  product-level runtime abstractions.
- [ ] 2.2 Define ordinary UI and modal grammar as invocation layers over the
  same command model.
- [ ] 2.3 Define agents as first-class actors mediated by runtime abstractions,
  permissions, and audit surfaces.
- [ ] 2.4 Define Rust core plus WASM Component Model and WIT interfaces as the
  extension direction with explicit capability grants.

## 3. Incubation Decomposition

- [ ] 3.1 Identify that the first follow-up change should validate product
  assumptions rather than implement the complete environment.
- [ ] 3.2 Require the first follow-up prototype or MVP to prove local-first
  discovery of real work.
- [ ] 3.3 Require the first follow-up prototype or MVP to prove one workflow
  through object, buffer, view, command, query, and agent participation.
- [ ] 3.4 Defer the concrete first interface choice until a focused incubation
  proposal selects editor-like environment, object workspace, agent workspace,
  or another form.
- [ ] 3.5 Defer universal URI/resource addressing until a later architecture RFC
  proves it is needed beyond filesystem-first composition.

## 4. Verification

- [ ] 4.1 Run `openspec validate define-programmable-environment-product --strict`.
- [ ] 4.2 Run `git diff --check -- openspec/changes/define-programmable-environment-product`.
- [ ] 4.3 Review proposal, design, specs, and tasks for placeholders,
  contradictions, premature implementation commitments, and unclear product
  boundaries.
- [ ] 4.4 Confirm the change does not modify current Alan runtime, daemon,
  terminal, or macOS shell behavior.

## 5. Archive Readiness

- [ ] 5.1 After review and merge, sync
  `programmable-environment-product` into `openspec/specs/`.
- [ ] 5.2 Archive the completed change only after the long-lived spec has been
  updated and validated.
