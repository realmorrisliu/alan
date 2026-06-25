## Why

alan needs a repo-level product constitution: a programmable personal
computing environment for humans and agents. This should become the organizing
model for Alan agent, Alan for macOS, environment apps such as Groove Master,
and future products in this repository.

The idea is broad enough that it should be protected before it is reduced into
a macOS shell feature, editor feature, AI IDE, agent chat UI, or one
implementation plan. At the same time, it should not remain a side product that
sits next to Alan. Existing and future Alan specs should be able to state how
they fit into this environment model.

## What Changes

- Define the programmable environment as the long-term product constitution for
  the alan repo: a local-first, UNIX-respecting programmable personal computing
  environment.
- Define a product-family role model for future and existing specs: environment
  core, agent runtime/actor, host surface, environment app, adapter, and
  legacy/compatibility surface.
- Establish the durable product identity: user activity happens inside one
  object/command/buffer/view/query runtime while data may remain in ordinary
  filesystems and external systems.
- Require future Alan specs, and selected existing active specs, to identify
  their environment role and their relationship to objects, commands, buffers,
  views, queries, actors, source-of-truth boundaries, and host responsibilities.
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
- Classify `introduce-workbench-runtime` as a candidate runtime substrate
  incubation slice rather than the complete programmable environment product.

## Capabilities

### New Capabilities

- `programmable-environment-product`: Owns the product constitution for the
  alan repo's programmable personal computing environment, including
  product-family roles, local-first data boundaries, core runtime abstractions,
  interaction principles, agent-native participation, extension principles,
  spec-alignment rules, and incubation criteria.

### Modified Capabilities

- None. This change intentionally does not modify existing Alan runtime,
  daemon, terminal, or macOS shell behavior.

## Impact

- Affected product planning: introduces a repo-level product constitution in
  OpenSpec.
- Affected architecture planning: future runtime, UI, agent, app, host, adapter,
  and extension proposals should trace back to this constitution or explicitly
  declare themselves legacy/compatibility work.
- Affected OpenSpec maintenance: selected active specs should be aligned over
  follow-up changes by adding environment role, abstraction mapping, and
  source-of-truth boundary notes. For example,
  `add-macos-shell-component-system` should be classified as a macOS host
  surface/design-system capability, not an environment core runtime.
- Affected current code: none in this change.
- Affected current Alan shell behavior: none; current Alan features may inform
  later work but are not the required implementation boundary for the
  programmable environment.
