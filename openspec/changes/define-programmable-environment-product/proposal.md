## Why

alan needs a clear product-level contract for a new, independent product line:
a programmable personal computing environment for humans and agents. The idea is
large enough that it should be protected as a product constitution before it is
reduced into a macOS shell feature, editor feature, AI IDE, or implementation
plan.

## What Changes

- Define a new independent product line inside this repo: a local-first,
  UNIX-respecting programmable personal computing environment.
- Establish the durable product identity: user activity happens inside one
  object/command/buffer/view/query runtime while data may remain in ordinary
  filesystems and external systems.
- Make out-of-the-box usefulness a first principle: first launch should reveal
  and organize real personal work rather than presenting an empty programmable
  substrate.
- Define human and agent as first-class runtime actors that share the same
  object, command, buffer, view, query, permission, and audit model.
- Define Rust core plus WASM Component Model extensions as a core product
  principle, while keeping detailed extension interfaces for later architecture
  changes.
- Define modal interaction as a power layer over the same command model used by
  ordinary UI, command palettes, automation, and agents.
- Add an incubation path that validates the product assumptions without binding
  the first interface to editor, IDE, app launcher, or current Alan shell
  implementation choices.

## Capabilities

### New Capabilities

- `programmable-environment-product`: Owns the product constitution for the
  independent programmable personal computing environment, including local-first
  data boundaries, core runtime abstractions, interaction principles,
  agent-native participation, extension principles, and incubation criteria.

### Modified Capabilities

- None. This change intentionally does not modify existing Alan runtime,
  daemon, terminal, or macOS shell behavior.

## Impact

- Affected product planning: introduces a new product-line contract in
  OpenSpec.
- Affected architecture planning: future runtime, UI, agent, and extension
  proposals for this product should trace back to this constitution.
- Affected current code: none in this change.
- Affected current Alan shell behavior: none; current Alan features may inform
  later work but are not the implementation boundary for this product line.
